# Starts Lil Deets (target\release\lildeets.exe) in this window and copies every log line to
# logs\lildeets-<date>.log, so a Claude session can read the log after the fact.
# The "Lil Deets" shortcut runs this (ops/SETUP.md, "The shortcut"). Ctrl+C stops the bot.
# Plain text (NO_COLOR): the console and the file get the same lines, with no colour codes.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$Host.UI.RawUI.WindowTitle = 'Lil Deets'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$env:NO_COLOR = '1'

$logs = Join-Path $root 'logs'
New-Item -ItemType Directory -Force $logs | Out-Null
# Keep 30 days of logs.
Get-ChildItem $logs -Filter 'lildeets-*.log' |
    Where-Object { $_.LastWriteTime -lt (Get-Date).AddDays(-30) } |
    Remove-Item -Force

$log = Join-Path $logs ('lildeets-{0:yyyy-MM-dd}.log' -f (Get-Date))
$writer = [System.IO.StreamWriter]::new($log, $true, [System.Text.UTF8Encoding]::new($false))
$writer.AutoFlush = $true
$writer.WriteLine("==== start $(Get-Date -Format 's') ====")
Write-Host "Lil Deets is starting. The log is also written to $log"
try {
    # cmd joins stderr (the launcher's lines) to stdout (tracing), so the file has both.
    cmd /c "target\release\lildeets.exe 2>&1" | ForEach-Object { $writer.WriteLine($_); $_ }
    $writer.WriteLine("==== exit $LASTEXITCODE at $(Get-Date -Format 's') ====")
}
finally {
    $writer.Close()
}
Write-Host ''
Write-Host 'Lil Deets has stopped. Close this window, or read the lines above.'
