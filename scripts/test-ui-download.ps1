$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ui-download.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) ('desktop-pilot-ui-transfer-' + [guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($root) | Out-Null
$archive = Join-Path $root 'synthetic.7z'
$bytes = [Text.Encoding]::UTF8.GetBytes('synthetic verified renderer')
$hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
try {
    $script:attempts = 0
    $script:uris = @()
    $successOnThird = {
        param([string]$Url, [string]$OutFile)
        $script:attempts++
        $script:uris += $Url
        if ($script:attempts -gt 1 -and (Test-Path -LiteralPath $OutFile)) { throw 'Partial archive was reused' }
        if ($script:attempts -lt 3) {
            [IO.File]::WriteAllText($OutFile, 'partial response')
            throw '504 Gateway Time-out'
        }
        [IO.File]::WriteAllBytes($OutFile, $bytes)
    }
    Invoke-VerifiedUiRendererDownload -Uri 'https://example.invalid/renderer' -AlternateUri 'https://api.github.com/repos/test/release/assets/123' -Destination $archive -ExpectedSha256 $hash -Transfer $successOnThird -RetryDelayMilliseconds 0
    if ($script:uris.Count -ne 3 -or $script:uris[0] -ne 'https://example.invalid/renderer' -or $script:uris[1] -ne 'https://api.github.com/repos/test/release/assets/123' -or $script:uris[2] -ne 'https://api.github.com/repos/test/release/assets/123') { throw 'Fallback did not use the pinned alternate route' }
    if ($script:attempts -ne 3) { throw "Expected 3 transfer attempts: $script:attempts" }
    if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $hash) { throw 'Verified archive not retained' }

    $script:attempts = 0
    $corrupt = {
        param([string]$Url, [string]$OutFile)
        $script:attempts++
        [IO.File]::WriteAllText($OutFile, 'unexpected bytes')
    }
    $checksumFailed = $false
    try {
        Invoke-VerifiedUiRendererDownload -Uri 'https://example.invalid/renderer' -Destination $archive -ExpectedSha256 $hash -Transfer $corrupt -RetryDelayMilliseconds 0
    } catch { $checksumFailed = $_.Exception.Message -match 'checksum mismatch' }
    if (-not $checksumFailed -or $script:attempts -ne 1 -or (Test-Path -LiteralPath $archive)) { throw 'Checksum protection failed' }

    $script:attempts = 0
    $alwaysFails = {
        param([string]$Url, [string]$OutFile)
        $script:attempts++
        [IO.File]::WriteAllText($OutFile, 'partial download')
        throw '504 Gateway Time-out'
    }
    $retryExhausted = $false
    try {
        Invoke-VerifiedUiRendererDownload -Uri 'https://example.invalid/renderer' -Destination $archive -ExpectedSha256 $hash -Transfer $alwaysFails -RetryDelayMilliseconds 0
    } catch { $retryExhausted = $_.Exception.Message -match 'failed after 3 attempts' }
    if (-not $retryExhausted -or $script:attempts -ne 3 -or (Test-Path -LiteralPath $archive)) { throw 'Exhaustion protection failed' }
    Write-Host 'PASS verified renderer transfer: retries, checksum, failed-download cleanup'
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
