//! deets-tap as a library (rust-port-plan.md §5 P2): one Windows app's sound, for Lil Deets.
//!
//! - [`TapStream`]: the app's sound as f32 LE stereo 48 kHz, a `symphonia` `MediaSource`
//!   that songbird reads through `RawAdapter::new(tap, 48_000, 2)`. No helper process, no
//!   pipe: the Rust host links this in.
//! - [`resolve_exe`], [`list_apps`], [`meters`]: what the `deets-tap` binary's flags print.
//!
//! The binary (`src/main.rs`) keeps its stdout contract for the Python bot unchanged.
//!
//! Threads: deets-airplay's capture thread, one per [`TapStream`] (it lives as long as the
//! stream; dropping the stream stops it). Nothing else.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use deets_airplay::capture::Capture;
use deets_airplay::mixer::children_named;
use symphonia_core::io::MediaSource;

use windows::core::Interface;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS};

/// Discord's rate; the process-loopback device converts to it, so nothing is resampled.
pub const RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// One Discord frame: 20 ms.
pub const BLOCK_FRAMES: usize = 960;
/// Audio held back before playing (and again after an underrun), so the capture's ~10 ms
/// packets never land a hair late and click.
pub const PRIME_FRAMES: usize = BLOCK_FRAMES * 2;
pub const WEBVIEW: &str = "msedgewebview2.exe";

// --- processes ---------------------------------------------------------------------------

/// Every process: (pid, parent pid, exe name lower-cased).
fn snapshot() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut e = PROCESSENTRY32 { dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32, ..Default::default() };
            if Process32First(snap, &mut e).is_ok() {
                loop {
                    let name: String = e.szExeFile.iter().take_while(|&&c| c != 0).map(|&c| c as u8 as char).collect();
                    out.push((e.th32ProcessID, e.th32ParentProcessID, name.to_ascii_lowercase()));
                    if Process32Next(snap, &mut e).is_err() {
                        break;
                    }
                }
            }
            CloseHandle(snap).ok();
        }
    }
    out
}

fn process_map() -> HashMap<u32, (u32, String)> {
    snapshot().into_iter().map(|(pid, parent, exe)| (pid, (parent, exe))).collect()
}

/// The app a session belongs to, by name: the session's own exe, except that a WebView2
/// process is named after the nearest ancestor that is not WebView2 (the Tauri/Edge host),
/// since that is the name a person knows and passes to `--exe`.
fn app_name(pid: u32, procs: &HashMap<u32, (u32, String)>) -> Option<String> {
    let (mut parent, mut name) = procs.get(&pid).cloned()?;
    let mut hops = 0;
    while name == WEBVIEW && hops < 16 {
        let (p, n) = procs.get(&parent).cloned()?;
        parent = p;
        name = n;
        hops += 1;
    }
    Some(name)
}

/// The pid to capture for an exe name: the top-most process of that name (a multi-process
/// app's children share its name), or its WebView2 child for a WebView2 host.
pub fn resolve_exe(exe: &str) -> Result<u32, String> {
    let want = exe.to_ascii_lowercase();
    let procs = snapshot();
    let parent_name: HashMap<u32, String> = procs.iter().map(|(pid, _, exe)| (*pid, exe.clone())).collect();
    let root = procs
        .iter()
        .filter(|(_, _, e)| *e == want)
        .find(|(_, parent, _)| parent_name.get(parent).is_none_or(|p| *p != want))
        .map(|(pid, _, _)| *pid)
        .ok_or_else(|| format!("{exe} is not running"))?;
    Ok(children_named(root, WEBVIEW).into_iter().next().unwrap_or(root))
}

// --- sessions ----------------------------------------------------------------------------

fn session_pids() -> Result<Vec<u32>, String> {
    let mut pids = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("MMDeviceEnumerator: {e}"))?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|e| format!("default render device: {e}"))?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).map_err(|e| format!("IAudioSessionManager2: {e}"))?;
        let sessions = manager.GetSessionEnumerator().map_err(|e| format!("GetSessionEnumerator: {e}"))?;
        let n = sessions.GetCount().map_err(|e| e.to_string())?;
        for i in 0..n {
            let Ok(ctl) = sessions.GetSession(i) else { continue };
            let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
            if let Ok(pid) = ctl2.GetProcessId() {
                pids.push(pid);
            }
        }
    }
    Ok(pids)
}

/// Apps with an audio session on the default output right now, by name, sorted (`--list`;
/// `/vc play`'s autocomplete).
pub fn list_apps() -> Result<Vec<String>, String> {
    let procs = process_map();
    let mut names = BTreeSet::new();
    for pid in session_pids()? {
        if pid == 0 {
            continue; // system sounds
        }
        if let Some(name) = app_name(pid, &procs) {
            names.insert(name);
        }
    }
    Ok(names.into_iter().collect())
}

/// One audio session on a device, for `--meters`.
pub struct SessionMeter {
    pub pid: u32,
    pub app: String,
    /// The highest peak over one second, 0..1.
    pub peak: f32,
    /// The app's Windows mixer volume and mute, when it has a volume control.
    pub mixer: Option<(f32, bool)>,
}

/// One active output device and its sessions, for `--meters`.
pub struct DeviceMeters {
    pub id: String,
    pub default: bool,
    pub sessions: Vec<SessionMeter>,
}

/// Every active output device and each audio session's app and peak level over one second
/// per device (it takes about 1 s per device).
pub fn meters() -> Result<Vec<DeviceMeters>, String> {
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;
    let procs = process_map();
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let default_id = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.GetId()).map(|p| p.to_string().unwrap_or_default()).unwrap_or_default();
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|e| e.to_string())?;
        for i in 0..devices.GetCount().map_err(|e| e.to_string())? {
            let Ok(device) = devices.Item(i) else { continue };
            let id = device.GetId().map(|p| p.to_string().unwrap_or_default()).unwrap_or_default();
            let mut entry = DeviceMeters { default: id == default_id, id, sessions: Vec::new() };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                out.push(entry);
                continue;
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                out.push(entry);
                continue;
            };
            let mut rows = Vec::new();
            for j in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(ctl) = sessions.GetSession(j) else { continue };
                let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                let pid = ctl2.GetProcessId().unwrap_or(0);
                let Ok(meter) = ctl.cast::<IAudioMeterInformation>() else { continue };
                let vol = ctl
                    .cast::<windows::Win32::Media::Audio::ISimpleAudioVolume>()
                    .ok()
                    .map(|v| (v.GetMasterVolume().unwrap_or(-1.0), v.GetMute().map(|m| m.as_bool()).unwrap_or(false)));
                rows.push((pid, meter, vol));
            }
            let mut peaks = vec![0f32; rows.len()];
            for _ in 0..20 {
                for (k, (_, m, _)) in rows.iter().enumerate() {
                    peaks[k] = peaks[k].max(m.GetPeakValue().unwrap_or(0.0));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            for ((pid, _, mixer), peak) in rows.into_iter().zip(peaks) {
                let app = app_name(pid, &procs).unwrap_or_else(|| "?".into());
                entry.sessions.push(SessionMeter { pid, app, peak, mixer });
            }
            out.push(entry);
        }
    }
    Ok(out)
}

// --- the stream --------------------------------------------------------------------------

/// One app's sound as f32 LE stereo at [`RATE`], for songbird:
/// `Input::from(RawAdapter::new(tap, RATE, CHANNELS as u32))`.
///
/// songbird's mixer pulls 20 ms a tick on its own clock, so this **never sleeps**
/// (rust-port-plan.md §5 P2, settled by ear on 2026-11-10 with the voice spike): a read
/// hands over what the capture holds; after a start or an underrun it holds back until
/// [`PRIME_FRAMES`] are queued; while the app is quiet it gives 20 ms of silence. It never
/// returns 0, which would end the track.
pub struct TapStream {
    capture: Capture,
    primed: bool,
    pending: Vec<u8>,
    underruns: u64,
}

impl TapStream {
    /// Capture `pid` and its child processes.
    pub fn start(pid: u32) -> Result<Self, String> {
        Ok(Self::from_capture(Capture::start_process_at(pid, RATE)?))
    }

    /// Capture an app by exe name ([`resolve_exe`]).
    pub fn for_exe(exe: &str) -> Result<Self, String> {
        Self::start(resolve_exe(exe)?)
    }

    /// Over any capture (a test passes `Capture::from_ring`).
    pub fn from_capture(capture: Capture) -> Self {
        Self { capture, primed: false, pending: Vec::new(), underruns: 0 }
    }

    /// What the capture says it is ("process 1234 + children, 48.0 kHz / 16-bit, …").
    pub fn format_note(&self) -> &str {
        &self.capture.format_note
    }

    /// Times the capture ran dry after playing (each one holds back again).
    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    fn refill(&mut self) {
        let queued = self.capture.ring.queued_frames();
        if !self.primed && queued >= PRIME_FRAMES {
            self.primed = true;
        }
        if self.primed && queued == 0 {
            self.primed = false;
            self.underruns += 1;
        }
        let frames = if self.primed { queued } else { 0 };
        let mut raw = vec![0i16; frames.max(BLOCK_FRAMES) * CHANNELS];
        if frames > 0 {
            raw.truncate(frames * CHANNELS);
            self.capture.ring.fill(&mut raw);
        }
        self.pending.extend(raw.iter().flat_map(|&s| (s as f32 / 32768.0).to_le_bytes()));
    }
}

impl Read for TapStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.pending.is_empty() {
            self.refill();
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl Seek for TapStream {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}

impl MediaSource for TapStream {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deets_airplay::capture::Ring;
    use std::sync::Arc;

    fn stub() -> (Arc<Ring>, TapStream) {
        let ring = Arc::new(Ring::with_capacity(RATE as usize / 10));
        (ring.clone(), TapStream::from_capture(Capture::from_ring(ring, 0, "stub")))
    }

    fn read_all(tap: &mut TapStream, bytes: usize) -> Vec<f32> {
        let mut buf = vec![0u8; bytes];
        let mut got = 0;
        while got < bytes {
            let n = tap.read(&mut buf[got..]).unwrap();
            assert!(n > 0, "a read must never return 0: that ends the track");
            got += n;
        }
        buf.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect()
    }

    #[test]
    fn silence_until_primed_then_the_audio_as_f32() {
        let (ring, mut tap) = stub();
        // Less than PRIME_FRAMES queued: 20 ms of silence, and nothing taken from the ring.
        ring.push(&vec![16384i16; BLOCK_FRAMES * CHANNELS]);
        assert!(read_all(&mut tap, BLOCK_FRAMES * CHANNELS * 4).iter().all(|&s| s == 0.0));
        assert_eq!(ring.queued_frames(), BLOCK_FRAMES);
        // Primed: everything queued comes out, i16 / 32768.
        ring.push(&vec![-16384i16; BLOCK_FRAMES * CHANNELS]);
        let out = read_all(&mut tap, PRIME_FRAMES * CHANNELS * 4);
        assert_eq!(out[0], 0.5);
        assert_eq!(*out.last().unwrap(), -0.5);
        assert_eq!(ring.queued_frames(), 0);
    }

    #[test]
    fn an_underrun_gives_silence_and_holds_back_again() {
        let (ring, mut tap) = stub();
        ring.push(&vec![1000i16; PRIME_FRAMES * CHANNELS]);
        read_all(&mut tap, PRIME_FRAMES * CHANNELS * 4);
        // Dry: silence, one underrun, and it waits for PRIME_FRAMES again.
        assert!(read_all(&mut tap, BLOCK_FRAMES * CHANNELS * 4).iter().all(|&s| s == 0.0));
        assert_eq!(tap.underruns(), 1);
        ring.push(&vec![1000i16; BLOCK_FRAMES * CHANNELS]);
        assert!(read_all(&mut tap, BLOCK_FRAMES * CHANNELS * 4).iter().all(|&s| s == 0.0), "one block is not enough to prime");
    }

    #[test]
    fn small_reads_keep_every_sample_in_order() {
        let (ring, mut tap) = stub();
        let ramp: Vec<i16> = (0..(PRIME_FRAMES * CHANNELS) as i16).collect();
        ring.push(&ramp);
        let mut out = Vec::new();
        let mut buf = [0u8; 12]; // three samples: not a whole frame
        while out.len() < ramp.len() * 4 {
            let n = tap.read(&mut buf).unwrap();
            out.extend_from_slice(&buf[..n]);
        }
        let samples: Vec<f32> = out.chunks_exact(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
        let want: Vec<f32> = ramp.iter().map(|&s| s as f32 / 32768.0).collect();
        assert_eq!(samples, want);
    }
}
