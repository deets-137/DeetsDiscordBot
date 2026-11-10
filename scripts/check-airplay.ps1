# check-airplay.ps1 — what changed in DeetsAirplay's capture crate since crates/tap pinned it.
#
# deets-tap calls Capture::start_process_at, mixer::children_named and mixer::running_exes
# from deets-airplay, pinned by rev in crates/tap/Cargo.toml (docs/ops/TAP.md §3). Run this
# before bumping the rev: it lists the commits after the pin that touch capture.rs or
# mixer.rs, and the signatures deets-tap relies on as they are at the newest commit.
#
#   powershell -File scripts/check-airplay.ps1               # against origin/master
#   powershell -File scripts/check-airplay.ps1 -Ref <commit> # against another commit
#
# Read-only: it fetches DeetsAirplay's remote refs and reads its history. Nothing is written.

param(
    [string]$Ref = "origin/master",
    [string]$Airplay = (Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) "DeetsAirplay")
)

$ErrorActionPreference = "Stop"
$toml = Join-Path (Split-Path $PSScriptRoot -Parent) "crates\tap\Cargo.toml"

$m = Select-String -Path $toml -Pattern 'deets-airplay\s*=.*rev\s*=\s*"([0-9a-f]+)"'
if (-not $m) { Write-Host "No rev pin for deets-airplay in $toml."; exit 1 }
$pin = $m.Matches[0].Groups[1].Value
if (-not (Test-Path $Airplay)) { Write-Host "DeetsAirplay not found at $Airplay."; exit 1 }

git -C $Airplay fetch --quiet origin
$head = (git -C $Airplay rev-parse $Ref).Trim()
Write-Host "Pinned: $pin"
Write-Host "Newest: $head ($Ref)"
if ($head -eq $pin) { Write-Host "Up to date."; exit 0 }

$paths = @("crates/airplay/src/capture.rs", "crates/airplay/src/mixer.rs")
$log = git -C $Airplay log --oneline "$pin..$head" -- $paths
if ($log) {
    Write-Host ""
    Write-Host "Commits after the pin that touch capture.rs or mixer.rs:"
    $log | ForEach-Object { Write-Host "  $_" }
} else {
    Write-Host "No commit after the pin touches capture.rs or mixer.rs."
}

Write-Host ""
Write-Host "What deets-tap calls, at ${Ref}:"
$capture = git -C $Airplay show "${head}:crates/airplay/src/capture.rs"
$mixer = git -C $Airplay show "${head}:crates/airplay/src/mixer.rs"
$want = @(
    @{ Text = $capture; Pattern = 'pub fn start_process_at\(' },
    @{ Text = $mixer;   Pattern = 'pub fn children_named\(' },
    @{ Text = $mixer;   Pattern = 'pub fn running_exes\(' }
)
$missing = 0
foreach ($w in $want) {
    $line = $w.Text | Select-String -Pattern $w.Pattern | Select-Object -First 1
    if ($line) { Write-Host ("  " + $line.Line.Trim()) } else { Write-Host "  MISSING: $($w.Pattern)"; $missing++ }
}
if ($missing) { Write-Host ""; Write-Host "deets-tap would not build against $Ref without changes."; exit 2 }
