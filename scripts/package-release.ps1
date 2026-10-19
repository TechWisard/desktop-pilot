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
