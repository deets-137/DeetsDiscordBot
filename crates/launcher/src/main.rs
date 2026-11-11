//! `lildeets`: the launcher (the port of `bot/__main__.py`). It runs `lildeets-host.exe` (beside
//! this exe) and starts it again whenever the host exits with [`RESTART_CODE`]: that is how a
//! change to the host goes live at a song boundary. Any other exit ends the launcher with the
//! host's exit code. Ctrl+C reaches both (they share the console); the launcher waits for the
//! host to close, then exits.
//!
//! No threads, no dependencies.

use std::process::{Command, ExitCode};
use std::time::Instant;

/// EX_TEMPFAIL, "run me again" (bot/live.py `RESTART_CODE`).
const RESTART_CODE: i32 = 75;

fn main() -> ExitCode {
    let host = match std::env::current_exe() {
        Ok(me) => me.with_file_name("lildeets-host.exe"),
        Err(e) => {
            eprintln!("[launcher] can't find myself: {e}");
            return ExitCode::from(2);
        }
    };
    loop {
        let started = Instant::now();
        let status = match Command::new(&host).status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[launcher] can't start {}: {e}", host.display());
                return ExitCode::from(2);
            }
        };
        match status.code() {
            Some(RESTART_CODE) => {
                println!("[launcher] the host asked for a restart after {} s; starting it again.", started.elapsed().as_secs());
            }
            Some(code) => return ExitCode::from(code.clamp(0, 255) as u8),
            None => return ExitCode::from(1),
        }
    }
}
