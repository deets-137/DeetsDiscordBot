//! `lildeets`: the launcher (the port of `docs/archive/bot/__main__.py`). It runs `lildeets-host.exe` (beside
//! this exe) and starts it again whenever the host exits with [`RESTART_CODE`]: that is how a
//! change to the host goes live at a song boundary. Any other exit ends the launcher with the
//! host's exit code. Ctrl+C reaches both (they share the console); the launcher waits for the
//! host to close, then exits.
//!
//! **The host runs as a copy** (`lildeets-host.run.exe`), copied fresh before each start.
//! Windows locks a running exe, so without the copy `cargo build` could not write the new
//! host while the bot plays. The host watches the original and asks for the restart itself
//! (crates/host, "the song-boundary restart"). [`ENV_LAUNCHED`] tells it a launcher is there
//! to start it again; run alone, the host never restarts itself.
//!
//! No threads, no dependencies.

use std::process::{Command, ExitCode};
use std::time::Instant;

/// EX_TEMPFAIL, "run me again" (docs/archive/bot/live.py `RESTART_CODE`).
const RESTART_CODE: i32 = 75;
/// Set for the host: a launcher will start it again on [`RESTART_CODE`].
const ENV_LAUNCHED: &str = "LILDEETS_LAUNCHED";

fn main() -> ExitCode {
    let host = match std::env::current_exe() {
        Ok(me) => me.with_file_name("lildeets-host.exe"),
        Err(e) => {
            eprintln!("[launcher] can't find myself: {e}");
            return ExitCode::from(2);
        }
    };
    let run = host.with_file_name("lildeets-host.run.exe");
    loop {
        if let Err(e) = std::fs::copy(&host, &run) {
            eprintln!("[launcher] can't copy {} to {}: {e} (is another Lil Deets running?)", host.display(), run.display());
            return ExitCode::from(2);
        }
        let started = Instant::now();
        let status = match Command::new(&run).env(ENV_LAUNCHED, "1").status() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[launcher] can't start {}: {e}", run.display());
                return ExitCode::from(2);
            }
        };
        match status.code() {
            Some(RESTART_CODE) => {
                println!("[launcher] the host asked for a restart after {} s; starting the new one.", started.elapsed().as_secs());
            }
            other => {
                let _ = std::fs::remove_file(&run);
                return match other {
                    Some(code) => ExitCode::from(code.clamp(0, 255) as u8),
                    None => ExitCode::from(1),
                };
            }
        }
    }
}
