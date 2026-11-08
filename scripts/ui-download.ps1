# A transient gateway failure must not skip UI acceptance or accept unverified bytes.
function Invoke-VerifiedUiRendererDownload {
    param(
        [Parameter(Mandatory)][string]$Uri,
        [Parameter(Mandatory)][string]$Destination,
        [Parameter(Mandatory)][string]$ExpectedSha256,
        [ValidateRange(1, 5)][int]$MaxAttempts = 3,
        [ValidateRange(0, 5000)][int]$RetryDelayMilliseconds = 750,
        [scriptblock]$Transfer = {
            param([string]$Url, [string]$OutFile)
            Invoke-WebRequest -Uri $Url -OutFile $OutFile -ErrorAction Stop
        }
    )
    $expected = $ExpectedSha256.ToLowerInvariant()
    if ($expected -notmatch '^[0-9a-f]{64}$') { throw 'Invalid expected renderer SHA-256' }
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop }
        try {
            & $Transfer $Uri $Destination | Out-Null
        } catch {
            if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop }
            if ($attempt -eq $MaxAttempts) {
                throw "Software renderer download failed after $MaxAttempts attempts: $($_.Exception.Message)"
            }
            Start-Sleep -Milliseconds ($RetryDelayMilliseconds * $attempt)
            continue
        }
        if (-not (Test-Path -LiteralPath $Destination -PathType Leaf)) { throw 'Software renderer download returned no archive' }
        if ((Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
            Remove-Item -LiteralPath $Destination -Force -ErrorAction Stop
            throw 'Software renderer checksum mismatch'
        }
        return
    }
}
