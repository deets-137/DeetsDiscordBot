//! `deets-tap`: one Windows app's sound as raw PCM on stdout, for Lil Deets' /vc.
//! docs/architecture/VOICE-PIPELINE.md and docs/ops/TAP.md are its docs.
//!
//! ```text
//! deets-tap --list              apps that have an audio session now, one per line
//! deets-tap --exe deetsmusic.exe
//! deets-tap --pid 1234
//! deets-tap --meters            every output device, each app's level and mixer volume
//! deets-tap --info              the stream contract as one JSON line (the bot checks it)
//! ```
//!
//! Streaming writes 48 kHz / 16-bit / stereo little-endian PCM, paced in real time in
//! 20 ms blocks (3840 bytes): exactly what discord.py's `PCMAudio` reads. Status goes to
//! stderr. It exits when stdout closes (the bot stopped reading) or the app quits.
//!
//! The capture is the crate's per-process loopback at Discord's own rate
//! ([`Capture::start_process_at`] with 48 kHz), so nothing is resampled here. DeetsMusic
//! pins the crate by git rev and keeps its 44.1 kHz `start_process`.
//!
//! Process loopback hears the app after its own volume slider (and its mute), and the
//! PC plays it wherever the app's output device is: point the app at an output nobody
//! listens to (Windows › Volume mixer) to hear it only through Discord. `--meters` shows
//! each app's level and mixer volume per output device.
//!
//! A WebView2 host (DeetsMusic, any Tauri app) makes its sound in an
//! `msedgewebview2.exe` child that the "include tree" mode does not reach from the host,
//! so the capture targets that child (see `mixer::children_named`).

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::time::{Duration, Instant};

use deets_airplay::capture::Capture;
use deets_airplay::mixer::{children_named, running_exes};

use windows::core::Interface;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS};

/// Discord's rate; the process-loopback device converts to it directly, so there is no
/// resampling here (until 2026-11-10 the capture ran at 44.1 kHz and `Sinc` took it back up).
const OUT_RATE: u32 = 48_000;
/// One Discord frame: 20 ms.
const BLOCK: Duration = Duration::from_millis(20);
const BLOCK_FRAMES: usize = 960;
/// Audio held back before playing (and again after an underrun), so the capture's
/// ~10 ms packets never land a hair late and click.
const PRIME_FRAMES: usize = BLOCK_FRAMES * 2;
const WEBVIEW: &str = "msedgewebview2.exe";
/// Bumped whenever the stdout format changes; bot/voice.py refuses a helper whose
/// contract differs from the one it was written for (docs/ops/TAP.md §3).
const CONTRACT: u32 = 1;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("--info") => {
            println!(
                "{{\"name\":\"deets-tap\",\"version\":\"{}\",\"contract\":{CONTRACT},\"rate\":{OUT_RATE},\"channels\":2,\"bits\":16,\"frame_bytes\":{}}}",
                env!("CARGO_PKG_VERSION"),
                BLOCK_FRAMES * 4
            );
            0
        }
        Some("--list") => list(),
        Some("--meters") => meters(),
        Some("--exe") if args.len() == 2 => match resolve_exe(&args[1]) {
            Ok(pid) => stream(pid),
            Err(e) => fail(&e),
        },
        Some("--pid") if args.len() == 2 => match args[1].parse::<u32>() {
            Ok(pid) => stream(pid),
            Err(_) => fail("--pid wants a number"),
        },
        _ => fail("usage: deets-tap --info | --list | --meters | --exe <name.exe> | --pid <pid>"),
    };
    std::process::exit(code);
}

fn fail(msg: &str) -> i32 {
    eprintln!("deets-tap: {msg}");
    2
}

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

/// The app a session belongs to, by name: the session's own exe, except that a
/// WebView2 process is named after the nearest ancestor that is not WebView2 (the
/// Tauri/Edge host), since that is the name a person knows and passes to `--exe`.
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

/// Apps with an audio session on the default output right now, one name per line.
fn list() -> i32 {
    let procs: HashMap<u32, (u32, String)> = snapshot().into_iter().map(|(pid, parent, exe)| (pid, (parent, exe))).collect();
    let mut names = BTreeSet::new();
    let pids = match session_pids() {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    for pid in pids {
        if pid == 0 {
            continue; // system sounds
        }
        if let Some(name) = app_name(pid, &procs) {
            names.insert(name);
        }
    }
    let mut out = std::io::stdout().lock();
    for n in names {
        writeln!(out, "{n}").ok();
    }
    0
}

/// Diagnostics: every active output device (id, default or not) and, under it, each
/// audio session's app and peak level over one second (0..1).
fn meters() -> i32 {
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE;
    let procs: HashMap<u32, (u32, String)> = snapshot().into_iter().map(|(pid, parent, exe)| (pid, (parent, exe))).collect();
    let r: Result<(), String> = (|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let default_id = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.GetId()).map(|p| p.to_string().unwrap_or_default()).unwrap_or_default();
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|e| e.to_string())?;
        for i in 0..devices.GetCount().map_err(|e| e.to_string())? {
            let Ok(device) = devices.Item(i) else { continue };
            let id = device.GetId().map(|p| p.to_string().unwrap_or_default()).unwrap_or_default();
            println!("{}{}", id, if id == default_id { "  (default)" } else { "" });
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
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
            for ((pid, _, vol), peak) in rows.iter().zip(peaks) {
                let name = app_name(*pid, &procs).unwrap_or_else(|| "?".into());
                let vol = vol.map(|(v, m)| format!("mixer vol {v:.2}{}", if m { " MUTED" } else { "" })).unwrap_or_default();
                println!("    pid {pid:>6}  {name:<24} peak {peak:.3}  {vol}");
            }
        }
        Ok(())
    })();
    match r {
        Ok(()) => 0,
        Err(e) => fail(&e),
    }
}

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

/// The pid to capture for an exe name: the top-most process of that name (a
/// multi-process app's children share its name), or its WebView2 child for a
/// WebView2 host.
fn resolve_exe(exe: &str) -> Result<u32, String> {
    let want = exe.to_ascii_lowercase();
    let procs = snapshot();
    let parent_name: HashMap<u32, String> = procs.iter().map(|(pid, _, exe)| (*pid, exe.clone())).collect();
    let root = procs
        .iter()
        .filter(|(_, _, e)| *e == want)
        .find(|(_, parent, _)| parent_name.get(parent).map_or(true, |p| *p != want))
        .map(|(pid, _, _)| *pid)
        .ok_or_else(|| format!("{exe} is not running"))?;
    Ok(children_named(root, WEBVIEW).into_iter().next().unwrap_or(root))
}

fn stream(pid: u32) -> i32 {
    let capture = match Capture::start_process_at(pid, OUT_RATE) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    eprintln!("deets-tap: capturing {} -> stdout", capture.format_note);

    let mut raw = vec![0i16; BLOCK_FRAMES * 2];
    let mut bytes: Vec<u8> = Vec::with_capacity(BLOCK_FRAMES * 4);
    let mut out = std::io::stdout().lock();

    let mut primed = false;
    let mut next = Instant::now();
    let mut last_alive_check = Instant::now();
    loop {
        // Real-time pacing on an absolute schedule, so sleep jitter never accumulates.
        next += BLOCK;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > BLOCK * 10 {
            next = now; // stalled (machine slept): resync rather than burst
        }

        let queued = capture.ring.queued_frames();
        if !primed && queued >= PRIME_FRAMES {
            primed = true;
        }
        if primed && queued >= BLOCK_FRAMES {
            capture.ring.fill(&mut raw);
        } else {
            // Not enough yet, or the app went quiet: silence, and hold back again.
            primed = false;
            raw.iter_mut().for_each(|s| *s = 0);
        }

        bytes.clear();
        bytes.extend(raw.iter().flat_map(|s| s.to_le_bytes()));
        if out.write_all(&bytes).and_then(|_| out.flush()).is_err() {
            return 0; // the reader went away
        }

        if last_alive_check.elapsed() >= Duration::from_secs(2) {
            last_alive_check = Instant::now();
            if !running_exes().contains_key(&pid) {
                eprintln!("deets-tap: process {pid} exited");
                return 0;
            }
        }
    }
}
