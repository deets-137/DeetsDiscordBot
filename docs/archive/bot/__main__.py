"""
`python -m bot` — the launcher. It runs the bot (`python -m bot.app`) as a child process and
starts it again whenever the bot exits with RESTART_CODE: that is how a change to the voice
code or the bot's core goes live at a song boundary without anyone restarting by hand
(bot/live.py, docs/architecture/LIVE-RELOAD.md). Any other exit ends the launcher with the
bot's exit code. Ctrl+C reaches both (they share the console); the launcher waits for the
bot to close, then exits.
"""

import subprocess
import sys
import time

from bot.live import RESTART_CODE
from journal.paths import PROJECT_ROOT


def main() -> int:
    while True:
        started = time.monotonic()
        child = subprocess.Popen([sys.executable, "-m", "bot.app"], cwd=PROJECT_ROOT)
        try:
            code = child.wait()
        except KeyboardInterrupt:
            try:
                return child.wait(timeout=15)
            except (KeyboardInterrupt, subprocess.TimeoutExpired):
                child.kill()
                return 130
        if code != RESTART_CODE:
            return code
        print(f"[launcher] the bot asked for a restart after {time.monotonic() - started:.0f} s; starting it again.",
              flush=True)


if __name__ == "__main__":
    sys.exit(main())
