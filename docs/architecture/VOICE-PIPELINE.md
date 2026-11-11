---
status: shipped
desk_test: passed 2026-11-10
sources: [crates/tap/src/lib.rs, crates/host/src/main.rs, crates/host/Cargo.toml]
updated: 2026-11-10
---
# The voice pipeline

How one Windows app's sound reaches a Discord call (the Rust bot, since 2026-11-10).

```
app (DeetsMusic's msedgewebview2.exe child)
  → Windows per-process loopback, 48 kHz stereo           deets_tap::TapStream (crates/tap, a library)
  → f32 PCM, 20 ms a tick, pulled by songbird's mixer    RawAdapter + symphonia's pcm decoder
  → track volume = the guild's gain (/dm volume)         lildeets-host
  → Opus encoder: Audio (music) mode, 128 kbps, no FEC   songbird 0.6.1
  → DAVE end-to-end encryption (Rust davey 0.1.5)        songbird
  → Discord voice server → the call
```

## 1. The capture
- **Per-process loopback** (`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`) records only what
  one process tree plays. No driver: the virtual cable in `../DeetsAudioDriver` is tabled for a
  signing certificate, and this does not need it.
- **It hears the app after the app's Windows mixer slider and mute.** At 17% the stream was
  near silence (peak 91 of 32767). Mute the app and the stream goes silent too (measured in
  DeetsMusic AIRPLAY.md §6).
- **It does not care which output device the app plays to.** Route the app to an output
  nobody listens to (2026-11-10: the Dell monitor's speakers) and the PC is quiet while the
  call hears everything. `ideas/auto-route.md` would do this by itself.
- **WebView2 apps** (DeetsMusic, any Tauri app) play from an `msedgewebview2.exe` child. The
  "include tree" flag does not reach it from the host exe, so `--exe deetsmusic.exe` resolves
  to that child (`mixer::children_named`).
- **48 kHz directly** (`Capture::start_process_at(pid, 48_000)`): the loopback device converts
  to the asked rate, so nothing is resampled in the helper. Until 2026-11-10 it captured
  44.1 kHz and a sinc resampler took it back to 48 kHz.

## 2. The pacing
songbird's mixer pulls 20 ms each tick on its own clock, so `TapStream` **never sleeps**: a
read hands over what the capture holds. After a start or an underrun it holds back until
40 ms are queued (`PRIME_FRAMES`), so the loopback's ~10 ms packets never land late and
click. While the app is quiet it gives 20 ms of silence. It never returns 0, which would end
the track. Settled by ear with the voice spike (2026-11-10, "clean").

## 3. The encoder
songbird encodes in Opus's Audio (music) mode at 128 kbps with no FEC: what the Python bot
had to ask discord.py for (its defaults, speech-capable modes and 15 % FEC, sounded grainy
and lost bass). A server's voice channel bitrate (64 kbps in an unboosted server) limits the
clients' own encoders; the bot sends its own bitrate.

## 4. DAVE
Discord requires end-to-end encryption (DAVE) in voice; songbird does it with the Rust
`davey` crate (0.1.5, pinned by songbird). Sound in a call proves DAVE works. songbird gives
no "DAVE ready" signal, so after a rejoin or a restart the host waits 2 s before DeetsMusic
plays again (`DAVE_SETTLE`, architecture/LIVE-RELOAD.md §4). `/vc tone` plays a 5 s tone to
check the path without an app.

## 5. Volume
`/dm volume` sets a per-guild gain (0–200 %) as the songbird track's volume. It never moves
DeetsMusic's slider, which the capture sits after.

## 6. The Python bot's pipeline (archived)
`docs/archive/bot/voice.py` ran `deets-tap.exe` as a helper and read its stdout (20 ms blocks
of 3840 bytes, paced by the helper; ops/TAP.md). It needed two fixes the Rust bot does not:
the encoder had to be set to music / no FEC / 128 kbps by hand, and **discord.py 2.7.1 needs
the Python `davey` below 0.2** (`docs/archive/bot/requirements-voice.txt`): davey 0.2.0
changed how a key change is applied, discord.py never calls it, and the call hears nothing.
