#requires -Version 5.1
<#
.SYNOPSIS
  Multi-EP recognizer bench (TASK-40). Runs the same wav through each EP
  the binary supports (CPU + whichever the build enabled) and appends
  results to scripts/bench/results/<host>-<date>.txt. Captures
  nvidia-smi SM utilization during GPU EPs.

.DESCRIPTION
  Companion to smoke-with-wpr.ps1 — that script benches one EP at a
  time. This one iterates [cpu, dml, cuda, tensorrt] in priority order,
  skips any EP that fails (e.g. not compiled in), and writes a per-EP
  median over `-Reps` repetitions. Designed for the TASK-40 decision
  matrix: did DML clear CPU on this hardware?

.PARAMETER Clip
  Wav to decode. Default: apps/macos/Resources/samples/smoke-test.wav.

.PARAMETER Reps
  Repetitions per EP. Default 5 (matches TASK-39 harness).

.PARAMETER OutDir
  Where to append results. Default scripts/bench/results/.
#>
[CmdletBinding()]
param(
  [string]$Clip,
  [int]$Reps = 5,
  [string]$OutDir
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $Clip)   { $Clip   = Join-Path $repoRoot 'apps\macos\Resources\samples\smoke-test.wav' }
if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'scripts\bench\results' }
$benchExe = Join-Path $repoRoot 'target\release\bench-sherpa.exe'
if (-not (Test-Path $benchExe)) {
  throw "bench-sherpa.exe not found. Build with: cargo build --release -p bench-sherpa --features openwhisper-core/recognizer-directml"
}
if (-not (Test-Path $Clip)) { throw "clip not found: $Clip" }

# Wipe the EP cache before each run so the probe state doesn't bias load_ms.
$cache = Join-Path $env:USERPROFILE '.cache\openwhisper\ep-pref.json'
if (Test-Path $cache) { Remove-Item $cache -Force }

$eps = @('cpu', 'dml', 'cuda', 'tensorrt')
$results = @{}
$nvidiaSmiAvailable = $null -ne (Get-Command nvidia-smi -ErrorAction SilentlyContinue)

foreach ($ep in $eps) {
  Write-Host "[bench] EP=$ep ($Reps reps)"
  $env:OPENWHISPER_PROVIDER = $ep
  if (Test-Path $cache) { Remove-Item $cache -Force }

  # GPU util sampler — nvidia-smi for any GPU EP run on NVIDIA hardware.
  # DML on AMD/Intel would need PerfMon \GPU Engine counters; deferred
  # since this box is NVIDIA-only.
  $smiOut = $null
  $smi = $null
  if ($nvidiaSmiAvailable -and $ep -ne 'cpu') {
    $smiOut = Join-Path $env:TEMP "nvidia-smi.$ep.txt"
    $smi = Start-Process -FilePath nvidia-smi `
            -ArgumentList @('dmon', '-s', 'u', '-c', ($Reps * 4)) `
            -PassThru -RedirectStandardOutput $smiOut -NoNewWindow
  }

  $rows = @()
  for ($i = 1; $i -le $Reps; $i++) {
    $stdoutFile = Join-Path $env:TEMP "bench-sherpa-$ep-$i.out"
    $stderrFile = Join-Path $env:TEMP "bench-sherpa-$ep-$i.err"
    $proc = Start-Process -FilePath $benchExe -ArgumentList @($Clip) `
              -PassThru -NoNewWindow -Wait `
              -RedirectStandardOutput $stdoutFile -RedirectStandardError $stderrFile
    if ($proc.ExitCode -ne 0) {
      $err = Get-Content $stderrFile -Raw -ErrorAction SilentlyContinue
      Write-Warning "EP=$ep rep $i failed (exit $($proc.ExitCode)): $err"
      $rows = $null
      break
    }
    $stdout = Get-Content $stdoutFile -Raw -ErrorAction SilentlyContinue
    if (-not $stdout) {
      Write-Warning "EP=$ep rep $i no stdout"
      $rows = $null
      break
    }
    $j = $stdout | ConvertFrom-Json
    Write-Host ("  rep {0}: load_ms={1} decode_ms={2}" -f $i, $j.load_ms, $j.decode_ms)
    $rows += $j
  }

  if ($smi) {
    Wait-Process -Id $smi.Id -Timeout 30 -ErrorAction SilentlyContinue
  }

  if ($null -eq $rows) {
    $results[$ep] = $null
    continue
  }
  $sorted = $rows | Sort-Object decode_ms
  $median = $sorted[[Math]::Floor($sorted.Count / 2)].decode_ms
  $loadMedian = ($rows | Sort-Object load_ms)[[Math]::Floor($rows.Count / 2)].load_ms
  $results[$ep] = [pscustomobject]@{
    Reps          = $Reps
    DecodeMedian  = $median
    DecodeMin     = ($rows | Measure-Object decode_ms -Minimum).Minimum
    DecodeMax     = ($rows | Measure-Object decode_ms -Maximum).Maximum
    LoadMedian    = $loadMedian
    Text          = ($rows[0].text -replace '\s+', ' ')
    SmiSamples    = if ($smiOut -and (Test-Path $smiOut)) { Get-Content $smiOut -Raw } else { $null }
  }
}
Remove-Item Env:OPENWHISPER_PROVIDER -ErrorAction SilentlyContinue

# Append to <host>-<date>.txt
if (-not (Test-Path $OutDir)) { New-Item -ItemType Directory -Path $OutDir -Force | Out-Null }
$outFile = Join-Path $OutDir "$($env:COMPUTERNAME)-$(Get-Date -Format 'yyyy-MM-dd').txt"
$lines = @()
$lines += ''
$lines += '---'
$lines += ''
$lines += '## TASK-40 ort engine bench (CPU vs DirectML vs CUDA vs TRT)'
$lines += ('host:    {0}' -f $env:COMPUTERNAME)
$lines += ('date:    {0}' -f (Get-Date -Format 'yyyy-MM-ddTHH:mm:ssK'))
$lines += ('engine:  ort 2.0.0-rc.10 / Parakeet-TDT v3 int8 / TASK-40 engine swap')
$lines += ('clip:    {0}' -f (Resolve-Path $Clip).Path)
$lines += ''
$lines += ('{0,-12} {1,5} {2,12} {3,9} {4,9} {5,11}' -f 'ep','reps','decode_med','dec_min','dec_max','load_median')
$lines += ('-' * 70)
foreach ($ep in $eps) {
  $r = $results[$ep]
  if ($null -eq $r) {
    $lines += ('{0,-12} {1,5} {2,12} {3,9} {4,9} {5,11}' -f $ep, 0, 'n/a (skip)', '-', '-', '-')
  } else {
    $lines += ('{0,-12} {1,5} {2,12} {3,9} {4,9} {5,11}' -f $ep, $r.Reps, $r.DecodeMedian, $r.DecodeMin, $r.DecodeMax, $r.LoadMedian)
  }
}
$lines += ''
foreach ($ep in $eps) {
  $r = $results[$ep]
  if ($null -ne $r -and $r.SmiSamples) {
    $lines += ("nvidia-smi dmon -s u (during EP={0}):" -f $ep)
    $lines += $r.SmiSamples.TrimEnd()
    $lines += ''
  }
}
foreach ($ep in $eps) {
  $r = $results[$ep]
  if ($null -ne $r) {
    $lines += ('text({0}): "{1}"' -f $ep, $r.Text)
  }
}

$lines | Out-File -FilePath $outFile -Encoding utf8 -Append
Write-Host ""
Write-Host "Wrote $outFile"
$lines | ForEach-Object { Write-Host $_ }
