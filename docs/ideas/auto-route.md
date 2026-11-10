---
status: idea
desk_test: none
sources: []
updated: 2026-11-10
---
# /vc play routes the app to a silent output by itself

## 1. What he said (2026-11-10)
"Can we have it disconnected from the system volume mixer? So I can hear it purely from
discord without listening to it here on my computer." Solved by hand: DeetsMusic's output set
to the Dell monitor at 100% in Windows' Volume mixer.

## 2. Claude's notes
- `/vc play` could set the app's output device to an unused one and restore it on `/vc stop`,
  through the undocumented `IAudioPolicyConfigFactory` (the interface EarTrumpet uses).
- **Windows keys the choice by the exe path.** DeetsMusic's sound comes from
  `msedgewebview2.exe`, shared by every WebView2 app (DeetsRGB, DeetsAirplay, Windows Search),
  so the route would move them all. Measure which identity Windows uses before building.
- It would also raise the app to 100% in the mixer for the stream and put it back after.
