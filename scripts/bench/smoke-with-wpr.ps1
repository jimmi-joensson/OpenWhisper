#requires -Version 5.1
<#
.SYNOPSIS
  Windows port of `smoke-with-powermetrics.sh`. Runs the cross-platform
  `bench-sherpa` Rust runner over each clip in `scripts/bench/clips/`
  (or a `-Clip` override), samples the bench process for CPU + working
  set, and writes a per-host-per-day report to
  `scripts/bench/results/<hostname>-<YYYY-MM-DD>.txt`.

.DESCRIPTION
  Sampling defaults to `Get-Counter` (no admin required). The script name
  references WPR because Windows Performance Recorder is the natural
  energy-profile equivalent to `powermetrics`, but WPR needs elevation -
  pass `-UseWpr` only when running from an admin shell. Without WPR, the
  `energy_J` column is `n/a`.

  Output shape mirrors what we want the Mac side
  (`smoke-with-powermetrics.sh`) to emit once it's updated for parity:
  one fixed-width row per clip, header with host / date / engine /
  sampler. Result file appends a new run on each invocation so multiple
  benches per day stay in one file.

.PARAMETER ClipDir
  Directory of `.wav` files to bench (one decode per file). Default
  `scripts/bench/clips/`. If empty, `-Clip` must be supplied.

.PARAMETER Clip
  Single wav override (skips ClipDir enumeration). Useful when clips/ is
  gitignored locally and only `apps/macos/Resources/samples/smoke-test.wav`
  is available.

.PARAMETER OutDir
  Where to write the report. Default `scripts/bench/results/`.

.PARAMETER IntervalMs
  Sampler period. Default 250 ms. Get-Counter has ~1 s overhead per call
  so the effective cadence will be slower; raise this to drop sample
  count.

.PARAMETER UseWpr
  Capture energy via Windows Performance Recorder energy profile.
  Requires admin. Off by default.

.EXAMPLE
  PS> .\smoke-with-wpr.ps1
  Iterates scripts\bench\clips\*.wav, writes
  scripts\bench\results\<host>-<date>.txt.

.EXAMPLE
  PS> .\smoke-with-wpr.ps1 -Clip ..\..\apps\macos\Resources\samples\smoke-test.wav
#>
[CmdletBinding()]
param(
  [string]$ClipDir,
  [string]$Clip,
  [string]$OutDir,
  [int]$IntervalMs = 250,
  [switch]$UseWpr
)

$ErrorActionPreference = 'Stop'

# --- resolve repo-relative paths from script location -----------------------
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $ClipDir) { $ClipDir = Join-Path $repoRoot 'scripts\bench\clips' }
if (-not $OutDir)  { $OutDir  = Join-Path $repoRoot 'scripts\bench\results' }
$benchExe = Join-Path $repoRoot 'target\release\bench-sherpa.exe'

if (-not (Test-Path $benchExe)) {
  throw "bench-sherpa.exe not found at $benchExe - build it first: cargo build --release -p bench-sherpa"
}

# --- enumerate clips --------------------------------------------------------
# Selection order, mirroring the Mac script's "always-runnable" default:
#   1. -Clip <path>           explicit single override
#   2. *.wav in -ClipDir      curated en/da set (gitignored, may be empty)
#   3. apps/macos/Resources/samples/smoke-test.wav   baked-in 5s EN baseline
$bakedSmoke = Join-Path $repoRoot 'apps\macos\Resources\samples\smoke-test.wav'
if ($Clip) {
  if (-not (Test-Path $Clip)) { throw "clip not found: $Clip" }
  $clips = @((Resolve-Path $Clip).Path)
} else {
  $clips = @()
  if (Test-Path $ClipDir) {
    $clips = @(Get-ChildItem -Path $ClipDir -Filter '*.wav' -File | ForEach-Object { $_.FullName })
  }
  if ($clips.Count -eq 0) {
    if (-not (Test-Path $bakedSmoke)) {
      throw "no clips in $ClipDir and baked-in smoke wav missing at $bakedSmoke"
    }
    Write-Host "[bench] no clips in $ClipDir; falling back to $bakedSmoke"
    $clips = @($bakedSmoke)
  }
}

# --- WPR setup (optional, admin only) ---------------------------------------
$wprEtl = $null
if ($UseWpr) {
  $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
              ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  if (-not $isAdmin) { throw "-UseWpr requires running from an elevated shell" }
  $wprEtl = Join-Path $env:TEMP "openwhisper-bench-$(Get-Date -Format 'yyyyMMdd-HHmmss').etl"
  & wpr.exe -start GeneralProfile -start CPU -start Power 2>&1 | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "wpr -start failed (exit $LASTEXITCODE)" }
}

# --- one-shot bench + sampler per clip --------------------------------------
$cpuCount = (Get-CimInstance Win32_Processor | Measure-Object NumberOfLogicalProcessors -Sum).Sum
$results = @()

foreach ($wav in $clips) {
  Write-Host "[bench] $wav"
  $stdoutFile = Join-Path $env:TEMP "bench-sherpa.out"
  $stderrFile = Join-Path $env:TEMP "bench-sherpa.err"

  $proc = Start-Process -FilePath $benchExe -ArgumentList @($wav) `
            -PassThru -NoNewWindow `
            -RedirectStandardOutput $stdoutFile `
            -RedirectStandardError  $stderrFile

  $pidPattern = "pid_$($proc.Id)" + "_"
  $cpuSamples = @()
  $rssSamples = @()
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while (-not $proc.HasExited) {
    $cpuRaw = (Get-Counter -Counter "\Process(*)\% Processor Time" -ErrorAction SilentlyContinue).CounterSamples |
              Where-Object { $_.InstanceName -eq $proc.ProcessName }
    if ($cpuRaw) {
      $cpuPct = ($cpuRaw | Measure-Object CookedValue -Sum).Sum / $cpuCount
      $cpuSamples += [math]::Round($cpuPct, 1)
    }
    try { $proc.Refresh(); $rssSamples += [math]::Round($proc.WorkingSet64 / 1MB, 1) } catch {}
    Start-Sleep -Milliseconds $IntervalMs
  }
  $proc.WaitForExit()
  $wallMs = $sw.ElapsedMilliseconds

  $stdout = (Get-Content $stdoutFile -Raw -ErrorAction SilentlyContinue)
  if (-not $stdout) {
    $stderr = (Get-Content $stderrFile -Raw -ErrorAction SilentlyContinue)
    throw "bench-sherpa produced no JSON. stderr: $stderr"
  }
  $json = $stdout | ConvertFrom-Json

  $avgCpu  = if ($cpuSamples) { [math]::Round(($cpuSamples | Measure-Object -Average).Average, 1) } else { 0 }
  $peakCpu = if ($cpuSamples) { ($cpuSamples | Measure-Object -Maximum).Maximum } else { 0 }
  $peakRss = if ($rssSamples) { ($rssSamples | Measure-Object -Maximum).Maximum } else { 0 }
  $rtX     = if ($json.decode_ms -gt 0) { [math]::Round($json.clip_seconds * 1000 / $json.decode_ms, 2) } else { 0 }

  $results += [pscustomobject]@{
    Clip        = (Split-Path $wav -Leaf)
    ClipSeconds = $json.clip_seconds
    LoadMs      = $json.load_ms
    DecodeMs    = $json.decode_ms
    RtX         = $rtX
    AvgCpu      = $avgCpu
    PeakCpu     = $peakCpu
    PeakRssMb   = $peakRss
    WallMs      = $wallMs
    Text        = ($json.text -replace '\s+', ' ').Substring(0, [math]::Min(60, $json.text.Length))
  }
}

# --- WPR teardown -----------------------------------------------------------
$energyAvailable = $false
if ($UseWpr -and $wprEtl) {
  & wpr.exe -stop $wprEtl 2>&1 | Out-Null
  $energyAvailable = ($LASTEXITCODE -eq 0)
}

# --- write report -----------------------------------------------------------
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
$hostName = $env:COMPUTERNAME
$date = Get-Date -Format 'yyyy-MM-dd'
$outFile = Join-Path $OutDir "$hostName-$date.txt"

$header = @(
  "# OpenWhisper recognizer bench"
  "host:    $hostName"
  "date:    $(Get-Date -Format 'yyyy-MM-ddTHH:mm:ssK')"
  "engine:  sherpa-onnx 1.12.40 / Parakeet-TDT v3 int8 / provider=coreml(falls back to CPU on Windows)"
  "sampler: Get-Counter (\Process %CPU + WorkingSet) @ ${IntervalMs}ms"
  "energy:  $(if ($energyAvailable) { "WPR ETL: $wprEtl" } else { 'n/a (no WPR; needs admin shell + -UseWpr)' })"
  ""
  ('{0,-32} {1,7} {2,8} {3,9} {4,7} {5,9} {6,9} {7,12}  {8}' -f `
     'clip','clip_s','load_ms','decode_ms','rt_x','avg_cpu%','peak_cpu%','peak_rss_mb','text')
  ('-' * 130)
)
$rows = $results | ForEach-Object {
  '{0,-32} {1,7:F2} {2,8} {3,9} {4,7:F2} {5,9:F1} {6,9:F1} {7,12:F1}  "{8}"' -f `
    $_.Clip, $_.ClipSeconds, $_.LoadMs, $_.DecodeMs, $_.RtX, $_.AvgCpu, $_.PeakCpu, $_.PeakRssMb, $_.Text
}

# Append (one file per host-day; multiple runs concatenate with a separator).
$separator = if (Test-Path $outFile) { @('', '---', '') } else { @() }
($separator + $header + $rows) | Out-File -FilePath $outFile -Encoding utf8 -Append

Write-Host ""
Write-Host "Wrote $outFile"
Get-Content $outFile | Select-Object -Last ($header.Count + $rows.Count + 1) | ForEach-Object { Write-Host $_ }
