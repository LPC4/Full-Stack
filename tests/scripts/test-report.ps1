# Runs the test suite via cargo-nextest and renders an HTML report.
#
# Usage:
#   tests\scripts\test-report.ps1                 # full suite, opens report
#   tests\scripts\test-report.ps1 -NoOpen         # do not open the browser
#   tests\scripts\test-report.ps1 -NoRun          # re-render from last junit.xml
#   tests\scripts\test-report.ps1 golden          # extra args pass through to nextest

param(
    [switch]$NoOpen,
    [switch]$NoRun
)
$NextestArgs = $args

if (-not $NextestArgs) { $NextestArgs = @('--workspace') }

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$junitPath = Join-Path $repo 'target\nextest\default\junit.xml'
$outDir = Join-Path $repo 'target\test-report'
$lastRunPath = Join-Path $outDir 'last-run.json'
$htmlPath = Join-Path $outDir 'index.html'

if (-not $NoRun) {
    Push-Location $repo
    try {
        # Test failures must not abort the script, the report shows them.
        & cargo nextest run @NextestArgs
        $runExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
} else {
    $runExit = $null
}

if (-not (Test-Path $junitPath)) {
    Write-Error "No junit.xml at $junitPath. Run without -NoRun first."
}

# --- Parse junit.xml into a flat test list ---

[xml]$junit = Get-Content $junitPath -Raw
$tests = [System.Collections.Generic.List[object]]::new()
foreach ($suite in $junit.testsuites.testsuite) {
    foreach ($case in $suite.testcase) {
        $status = 'passed'
        $detail = ''
        $failNode = if ($case.failure) { $case.failure } elseif ($case.error) { $case.error }
        if ($failNode) {
            $status = 'failed'
            $detail = "$($failNode.message)`n$($failNode.'#text')".Trim()
        } elseif ($case.skipped) {
            $status = 'skipped'
        }
        $tests.Add([pscustomobject]@{
            suite  = [string]$case.classname
            name   = [string]$case.name
            time   = [double]$case.time
            status = $status
            detail = $detail
        })
    }
}

# --- Newly-broken detection against the previous run ---

$prevFailed = @{}
$prevSeen = @{}
if (Test-Path $lastRunPath) {
    foreach ($t in (Get-Content $lastRunPath -Raw | ConvertFrom-Json)) {
        $key = "$($t.suite)::$($t.name)"
        $prevSeen[$key] = $true
        if ($t.status -eq 'failed') { $prevFailed[$key] = $true }
    }
}
foreach ($t in $tests) {
    $key = "$($t.suite)::$($t.name)"
    $newly = ($t.status -eq 'failed') -and $prevSeen.Count -gt 0 -and -not $prevFailed[$key]
    $t | Add-Member -NotePropertyName newlyFailed -NotePropertyValue $newly
}

New-Item -ItemType Directory -Force $outDir | Out-Null
$tests | Select-Object suite, name, time, status |
    ConvertTo-Json -Depth 3 -Compress | Set-Content $lastRunPath -Encoding utf8

# --- Render the HTML report ---

$meta = [pscustomobject]@{
    generated = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    wallTime  = [double]$junit.testsuites.time
    filter    = if ($NextestArgs) { $NextestArgs -join ' ' } else { '' }
}
$payload = [pscustomobject]@{ meta = $meta; tests = $tests } |
    ConvertTo-Json -Depth 4 -Compress
# Keep embedded JSON from terminating the script tag early.
$payload = $payload.Replace('</', '<\/')

$template = Get-Content (Join-Path $PSScriptRoot 'test-report.html') -Raw
$template.Replace('"__DATA__"', $payload) | Set-Content $htmlPath -Encoding utf8

$fails = @($tests | Where-Object status -eq 'failed').Count
Write-Host "Report: $htmlPath ($($tests.Count) tests, $fails failed)"
if (-not $NoOpen) { Start-Process $htmlPath }
if ($runExit) { exit $runExit }
