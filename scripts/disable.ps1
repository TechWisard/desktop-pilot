param([switch]$KeepShortcuts)
$ErrorActionPreference = 'Stop'
$pilotDirectory = if ((Test-Path -LiteralPath (Join-Path $PSScriptRoot 'desktop-pilot.exe')) -and (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'usage.sqlite'))) { $PSScriptRoot } else { Join-Path $env:LOCALAPPDATA 'DesktopPilot' }
$pilotExecutable = Join-Path $pilotDirectory 'desktop-pilot.exe'
if (-not (Test-Path -LiteralPath $pilotExecutable)) { throw 'Desktop Pilot is not installed' }
$reportDirectory = Join-Path $pilotDirectory ('disable-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $reportDirectory | Out-Null
function Invoke-Pilot([string[]]$Values) {
    $arguments = (@($Values) + @('--home', $pilotDirectory) | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $process = Start-Process -FilePath $pilotExecutable -ArgumentList $arguments -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw (Get-Content -Raw -LiteralPath (Join-Path $pilotDirectory 'last-error.txt') -Encoding UTF8) }
}
Invoke-Pilot @('--stop')
$stopped = $false
for ($attempt = 0; $attempt -lt 30; $attempt++) {
    $background = @(Get-CimInstance Win32_Process -Filter "Name='desktop-pilot.exe'" | Where-Object {
        $_.ExecutablePath -eq $pilotExecutable -and $_.CommandLine -match '(?:^|\s)"?--background"?(?:\s|$)'
    })
    if ($background.Count -eq 0) { $stopped = $true; break }
    Start-Sleep -Milliseconds 300
}
if (-not $stopped) { throw 'Background did not stop; no further changes made' }
Invoke-Pilot @('--disable', '--out', (Join-Path $reportDirectory 'disabled.json'))
if (-not $KeepShortcuts) {
    $finished = $false
    for ($step = 0; $step -lt 1000; $step++) {
        $result = Join-Path $reportDirectory ('undo-' + $step + '.json')
        Invoke-Pilot @('--rollback', '--out', $result)
        $message = Get-Content -Raw -LiteralPath $result -Encoding UTF8 | ConvertFrom-Json
        Write-Host $message
        if ($message -in @('Нет изменений для отката', 'No changes to undo')) { $finished = $true; break }
    }
    if (-not $finished) { throw 'Undo limit reached; run the script again to continue' }
}
Write-Host 'Desktop Pilot disabled. Originals and local data retained.'
