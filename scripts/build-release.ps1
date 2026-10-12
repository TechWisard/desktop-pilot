param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (-not $SkipChecks) {
        cargo fmt --check
        if ($LASTEXITCODE) { throw 'Formatting failed' }
        cargo test --locked
        if ($LASTEXITCODE) { throw 'Tests failed' }
        cargo clippy --locked --all-targets -- -D warnings
        if ($LASTEXITCODE) { throw 'Clippy failed' }
    }
    # Remove local build-machine paths from distributed executable strings.
    $remaps = @{}
    $remaps[$projectRoot] = '/project'
    $remaps[$env:USERPROFILE] = '/user'
    $cargoDirectory = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $remaps[$cargoDirectory] = '/cargo'
    $buildFlags = @()
    foreach ($entry in $remaps.GetEnumerator()) {
        $buildFlags += '--remap-path-prefix=' + $entry.Key + '=' + $entry.Value
        $buildFlags += '--remap-path-prefix=' + $entry.Key.Replace('\','/') + '=' + $entry.Value
    }
    $savedFlags = $env:CARGO_ENCODED_RUSTFLAGS
    try {
        $env:CARGO_ENCODED_RUSTFLAGS = $buildFlags -join [char]31
        cargo build --locked --release
        if ($LASTEXITCODE) { throw 'Release build failed' }
    } finally { $env:CARGO_ENCODED_RUSTFLAGS = $savedFlags }
    Write-Host 'Built target/release/desktop-pilot.exe'
} finally { Pop-Location }
