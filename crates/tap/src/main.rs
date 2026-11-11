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
//!
//! The process helpers and the Rust host's `TapStream` live in `src/lib.rs` (P2). This
//! binary keeps its own real-time pacer: the Python bot reads stdout at its pace.

use std::io::Write;
use std::time::{Duration, Instant};

use deets_airplay::capture::Capture;
use deets_airplay::mixer::running_exes;
use deets_tap::{list_apps, meters, resolve_exe, BLOCK_FRAMES, PRIME_FRAMES, RATE};

/// One Discord frame: 20 ms.
const BLOCK: Duration = Duration::from_millis(20);
/// Bumped whenever the stdout format changes; bot/voice.py refuses a helper whose
/// contract differs from the one it was written for (docs/ops/TAP.md §3).
const CONTRACT: u32 = 1;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("--info") => {
            println!(
                "{{\"name\":\"deets-tap\",\"version\":\"{}\",\"contract\":{CONTRACT},\"rate\":{RATE},\"channels\":2,\"bits\":16,\"frame_bytes\":{}}}",
                env!("CARGO_PKG_VERSION"),
                BLOCK_FRAMES * 4
            );
            0
        }
        Some("--list") => list(),
        Some("--meters") => print_meters(),
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

/// Apps with an audio session on the default output right now, one name per line.
fn list() -> i32 {
    match list_apps() {
        Ok(names) => {
            let mut out = std::io::stdout().lock();
            for n in names {
                writeln!(out, "{n}").ok();
            }
            0
        }
        Err(e) => fail(&e),
    }
}

/// Diagnostics: every active output device (id, default or not) and, under it, each
/// audio session's app and peak level over one second (0..1).
fn print_meters() -> i32 {
    match meters() {
        Ok(devices) => {
            for d in devices {
                println!("{}{}", d.id, if d.default { "  (default)" } else { "" });
                for s in d.sessions {
                    let vol = s.mixer.map(|(v, m)| format!("mixer vol {v:.2}{}", if m { " MUTED" } else { "" })).unwrap_or_default();
                    println!("    pid {:>6}  {:<24} peak {:.3}  {vol}", s.pid, s.app, s.peak);
                }
            }
            0
        }
        Err(e) => fail(&e),
    }
}

fn stream(pid: u32) -> i32 {
    let capture = match Capture::start_process_at(pid, RATE) {
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
