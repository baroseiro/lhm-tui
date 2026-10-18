# lhm-tui soak test: run the TUI for N minutes, sampling RSS/CPU every minute.
# Usage:  pwsh scripts/soak.ps1 -Minutes 60
param(
    [int]$Minutes = 60,
    [string]$Url = "http://127.0.0.1:8085",
    [string]$Csv = "soak.csv",
    [string]$Binary = "$PSScriptRoot\..\target\release\lhm-tui.exe"
)

if (-not (Test-Path $Binary)) {
    Write-Error "Build first: cargo build --release ($Binary not found)"
    exit 1
}

$log = Join-Path $env:TEMP "lhm-tui-soak.log"
"minute,rss_mb,cpu_s" | Set-Content $Csv
$process = Start-Process -FilePath $Binary `
    -ArgumentList "--url", $Url, "--log", $log `
    -PassThru -WindowStyle Minimized

Write-Host "soaking for $Minutes minute(s); pid $($process.Id); press Ctrl+C on the window to stop early"
for ($minute = 1; $minute -le $Minutes; $minute++) {
    Start-Sleep -Seconds 60
    if ($process.HasExited) {
        Write-Host "process exited early with code $($process.ExitCode)"
        break
    }
    $sample = $process.Refresh() | Out-Null; $process
    $rss = [math]::Round($process.WorkingSet64 / 1MB, 1)
    $cpu = [math]::Round($process.TotalProcessorTime.TotalSeconds, 1)
    "$minute,$rss,$cpu" | Add-Content $Csv
    Write-Host "minute $minute`: rss ${rss} MB, cpu ${cpu}s"
}

if (-not $process.HasExited) {
    Stop-Process -Id $process.Id -Force
}
Write-Host "sample log: $Csv (watch for flat RSS and slow-growing cpu seconds)"
