---
status: shipped
desk_test: passed 2026-11-10
sources: [bot/voice.py, crates/tap/src/main.rs, requirements-voice.txt]
updated: 2026-11-10
---
# The voice pipeline

How one Windows app's sound reaches a Discord call.

```
app (DeetsMusic's msedgewebview2.exe child)
  → Windows per-process loopback, 48 kHz / 16-bit / stereo     deets-tap (crates/tap)
  → stdout, 20 ms blocks of 3840 bytes, paced in real time
  → TapSource.read() → PCMVolumeTransformer (/dm volume)        bot/voice.py
  → Opus encoder: music, no FEC, 128 kbps                       discord.py
  → DAVE end-to-end encryption (davey 0.1.x)                    discord.py + davey
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

## 2. The helper's pacing
deets-tap writes one 20 ms block every 20 ms on an absolute schedule. It holds 40 ms back
before it starts (and again after an underrun), so the loopback's ~10 ms packets never land
late and click. It writes silence while the app is quiet. It exits when the bot stops reading
or the app quits. TAP.md has the flags and the contract.

## 3. The encoder
`voice.ENCODER = dict(signal_type="music", fec=False, bitrate=128)`, passed to every
`voice_client.play`. discord.py's defaults (`signal_type="auto"`, FEC on at 15% expected loss)
let Opus drop into its speech modes and spend bits on loss recovery: the stream sounded grainy
and lost bass. A server's voice channel bitrate (64 kbps in an unboosted server) limits the
clients' own encoders; the bot sends its own bitrate.

## 4. DAVE, and the davey pin
Discord requires end-to-end encryption (DAVE) in voice. discord.py 2.7.1 does it with the
`davey` package. **davey 0.2.0 changed how a key change is applied
(`prepare_transition` / `execute_transition`), and discord.py 2.7.1 never calls those.** The
DAVE session never becomes ready, frames go out unencrypted, and the call hears nothing while
the bot's ring shows green. Pinned `davey>=0.1.6,<0.2` in `requirements-voice.txt`.

`/vc tone` prints `voice E2EE (DAVE) version N, ready X`. `ready False` right after joining is
normal (the handshake finishes a moment later); silence with a working tone path is not.

## 5. Volume
`/dm volume` sets a per-guild gain on a `PCMVolumeTransformer` around the source (0–200%).
It never moves DeetsMusic's slider, which the capture sits after.
