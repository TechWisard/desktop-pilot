param(
    [string]$Executable = 'target/debug/desktop-pilot.exe',
    [string]$ReportDirectory = 'target/native-checks'
)
$ErrorActionPreference = 'Stop'
$exe = (Resolve-Path -LiteralPath $Executable).Path
$reports = [System.IO.Path]::GetFullPath($ReportDirectory)
[System.IO.Directory]::CreateDirectory($reports) | Out-Null
$profile = Join-Path ([System.IO.Path]::GetTempPath()) ('test-desktop-pilot-' + [guid]::NewGuid().ToString('N'))
$report = Join-Path $reports 'native-integration.json'
$info = [System.Diagnostics.ProcessStartInfo]::new($exe)
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
foreach ($argument in @('--self-test', '--home', $profile, '--out', $report)) {
    $info.ArgumentList.Add($argument)
}
$process = [System.Diagnostics.Process]::Start($info)
$output = $process.StandardOutput.ReadToEndAsync()
$errors = $process.StandardError.ReadToEndAsync()
if (-not $process.WaitForExit(120000)) {
    $process.Kill($true)
    throw 'Native integration checks exceeded 120 seconds'
}
$stdout = $output.GetAwaiter().GetResult()
$stderr = $errors.GetAwaiter().GetResult()
[System.IO.File]::WriteAllText((Join-Path $reports 'stdout.txt'), $stdout)
[System.IO.File]::WriteAllText((Join-Path $reports 'stderr.txt'), $stderr)
if ($process.ExitCode -ne 0) { throw "Native integration checks failed: $stderr" }
if (-not (Test-Path -LiteralPath $report)) { throw 'Native integration report is missing' }
$result = Get-Content -Raw -LiteralPath $report | ConvertFrom-Json
if ($result.passed -ne $true -or $result.originals_unchanged -ne $true) {
    throw 'Native integration report did not confirm success'
}
Write-Host ("Native integration: {0} checks passed" -f $result.checks.Count)
$result.checks | ForEach-Object { Write-Host "  $_" }
