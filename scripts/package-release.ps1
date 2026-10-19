param([string]$OutputDirectory = 'dist')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Could not read package metadata' }
    $version = ($metadata.packages | Where-Object name -eq 'desktop-pilot').version
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version' }
    $executable = Join-Path $projectRoot 'target/release/desktop-pilot.exe'
    if (-not (Test-Path -LiteralPath $executable)) { throw 'Build the release executable first' }
    $productVersion = (Get-Item -LiteralPath $executable).VersionInfo.ProductVersion
    if ($productVersion -ne $version) { throw "Executable version $productVersion does not match $version" }
    # Inspect the PE import table; a build runner can hide a missing user prerequisite.
    $peBytes = [System.IO.File]::ReadAllBytes($executable)
    $peOffset = [BitConverter]::ToInt32($peBytes, 0x3c)
    if ([BitConverter]::ToUInt32($peBytes, $peOffset) -ne 0x4550 -or
        [BitConverter]::ToUInt16($peBytes, $peOffset + 4) -ne 0x8664 -or
        [BitConverter]::ToUInt16($peBytes, $peOffset + 24) -ne 0x20b) {
        throw 'Expected a Windows x64 executable'
    }
    $sectionCount = [BitConverter]::ToUInt16($peBytes, $peOffset + 6)
    $sectionStart = $peOffset + 24 + [BitConverter]::ToUInt16($peBytes, $peOffset + 20)
    function Get-PeFileOffset([uint32]$Rva) {
        for ($index = 0; $index -lt $sectionCount; $index++) {
            $section = $sectionStart + 40 * $index
            $start = [BitConverter]::ToUInt32($peBytes, $section + 12)
            $size = [Math]::Max([BitConverter]::ToUInt32($peBytes, $section + 8), [BitConverter]::ToUInt32($peBytes, $section + 16))
            if ($Rva -ge $start -and $Rva -lt ([uint64]$start + $size)) {
                return [int]([BitConverter]::ToUInt32($peBytes, $section + 20) + $Rva - $start)
            }
        }
        throw 'Invalid executable import table'
    }
    $importRva = [BitConverter]::ToUInt32($peBytes, $peOffset + 24 + 120)
    if ($importRva -eq 0) { throw 'Executable import table is missing' }
    $imports = Get-PeFileOffset $importRva
    $dependencies = [System.Collections.Generic.List[string]]::new()
    for ($entry = 0; $entry -lt 256; $entry++) {
        $nameRva = [BitConverter]::ToUInt32($peBytes, $imports + 20 * $entry + 12)
        if ($nameRva -eq 0) { break }
        $nameStart = Get-PeFileOffset $nameRva
        $nameEnd = $nameStart
        while ($nameEnd -lt $peBytes.Length -and $peBytes[$nameEnd] -ne 0) { $nameEnd++ }
        if ($nameEnd -eq $peBytes.Length) { throw 'Invalid executable dependency name' }
        $dependencies.Add([System.Text.Encoding]::ASCII.GetString($peBytes, $nameStart, $nameEnd - $nameStart))
    }
    if ($entry -eq 256) { throw 'Invalid executable dependency count' }
    $externalRuntime = @($dependencies | Where-Object { $_ -match '^(VCRUNTIME|MSVCP|MSVCR)\d.*\.dll$' })
    if ($externalRuntime.Count) { throw "Executable requires a separate Visual C++ runtime: $($externalRuntime -join ', ')" }
    Write-Host ('Verified Windows x64 dependencies: ' + ($dependencies -join ', '))
    $output = [System.IO.Path]::GetFullPath($OutputDirectory)
    [System.IO.Directory]::CreateDirectory($output) | Out-Null
    $staging = Join-Path $output ('package-' + [guid]::NewGuid().ToString('N'))
    [System.IO.Directory]::CreateDirectory($staging) | Out-Null
    try {
        Copy-Item -LiteralPath $executable -Destination $staging
        foreach ($file in @('README.md', 'README.ru.md', 'THIRD-PARTY-NOTICES.txt', 'BUILD-DEPENDENCY-NOTICES.txt')) {
            Copy-Item -LiteralPath (Join-Path $projectRoot $file) -Destination $staging
        }
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'disable.ps1') -Destination $staging
        Copy-Item -LiteralPath (Join-Path $projectRoot 'docs') -Destination $staging -Recurse
        $binaryZip = Join-Path $output "DesktopPilot-$version-windows-x64.zip"
        Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $binaryZip -Force
        $sourceZip = Join-Path $output "DesktopPilot-$version-source.zip"
        git archive --format=zip "--prefix=DesktopPilot-$version-source/" "--output=$sourceZip" HEAD
        if ($LASTEXITCODE) { throw 'Source archive failed' }
        $lines = foreach ($archive in @($binaryZip, $sourceZip)) {
            $digest = (Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash.ToLowerInvariant()
            "$digest  $([System.IO.Path]::GetFileName($archive))"
        }
        [System.IO.File]::WriteAllText((Join-Path $output 'SHA256SUMS.txt'), (($lines -join "`n") + "`n"), [System.Text.UTF8Encoding]::new($false))
        Write-Host "Prepared Desktop Pilot $version"
        Get-ChildItem -LiteralPath $output -File | Select-Object Name, Length
    } finally {
        if (Test-Path -LiteralPath $staging) { Remove-Item -LiteralPath $staging -Recurse -Force }
    }
} finally { Pop-Location }
