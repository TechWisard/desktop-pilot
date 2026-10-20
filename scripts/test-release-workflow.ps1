param([string]$PublishScript = (Join-Path $PSScriptRoot 'publish-release.ps1'))
$ErrorActionPreference = 'Stop'
$publish = (Resolve-Path -LiteralPath $PublishScript).Path
$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('test-pilot-publication-' + [guid]::NewGuid().ToString('N'))
$savedRepository = $env:GH_REPO
$savedCommit = $env:GITHUB_SHA

# These commands provide local API fixtures. No GitHub request or mutation is made.
function cargo {
    $global:LASTEXITCODE = 0
    '{"packages":[{"name":"desktop-pilot","version":"9.9.9"}]}'
}
function git {
    if (($args -join ' ') -ne 'rev-parse HEAD') { throw 'Unexpected git operation in release fixture' }
    $global:LASTEXITCODE = 0
    'a' * 40
}
function gh {
    $global:LASTEXITCODE = 0
    $arguments = @($args)
    $fixture.Calls.Add(($arguments -join ' '))
    if ($arguments[0] -eq 'api') {
        $endpoint = $arguments[1]
        if ($endpoint -eq 'repos/fixture/desktop-pilot/git/ref/heads/main') {
            $fixture.HeadReads++
            if ($fixture.Scenario -eq 'old-head' -or ($fixture.Scenario -eq 'moving-head' -and $fixture.HeadReads -gt 1)) { return ('b' * 40) }
            return ('a' * 40)
        }
        if ($endpoint -eq 'repos/fixture/desktop-pilot/git/matching-refs/tags/v9.9.9') {
            if ($fixture.Scenario -eq 'wrong-tag') {
                return '[{"ref":"refs/tags/v9.9.9","object":{"type":"commit","sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}]'
            }
            if ($fixture.Scenario -eq 'annotated-tag') {
                return '[{"ref":"refs/tags/v9.9.9","object":{"type":"tag","sha":"cccccccccccccccccccccccccccccccccccccccc"}}]'
            }
            return '[]'
        }
        if ($endpoint -eq 'repos/fixture/desktop-pilot/git/tags/cccccccccccccccccccccccccccccccccccccccc') {
            return '{"object":{"type":"commit","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}'
        }
        if ($endpoint -eq 'repos/fixture/desktop-pilot/releases/tags/v9.9.9') {
            $global:LASTEXITCODE = 1
            return '{"message":"Not Found"}'
        }
        if ($endpoint -eq 'https://api.github.com/repos/fixture/desktop-pilot/releases/12345') {
            $assets = @(Get-ChildItem -LiteralPath dist -File | ForEach-Object {
                @{ name = $_.Name; size = $_.Length; digest = ('sha256:' + (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()) }
            })
            if ($fixture.Scenario -eq 'wrong-digest') { $assets[0].digest = 'sha256:incorrect' }
            if ($fixture.Scenario -eq 'missing-asset') { $assets = @($assets | Select-Object -First 2) }
            return (@{ assets = $assets; tag_name = 'v9.9.9'; draft = $true; target_commitish = ('a' * 40) } | ConvertTo-Json -Depth 5 -Compress)
        }
        throw "Unexpected API request in release fixture: $endpoint"
    }
    if ($arguments[0] -eq 'release') {
        switch ($arguments[1]) {
            'view' {
                if ($arguments -contains 'apiUrl' -and $fixture.Uploaded) { return 'https://api.github.com/repos/fixture/desktop-pilot/releases/12345' }
                if ($fixture.Scenario -eq 'draft') { return '{"isDraft":true,"tagName":"v9.9.9"}' }
                if ($fixture.Scenario -eq 'published') { return '{"isDraft":false,"tagName":"v9.9.9"}' }
                $global:LASTEXITCODE = 1
                return
            }
            'create' { $fixture.Created = $true; return }
            'upload' { $fixture.Uploaded = $true; return }
            'edit' {
                $targetIndex = [Array]::IndexOf($arguments, '--target')
                if ($targetIndex -lt 0 -or $arguments[$targetIndex + 1] -ne $env:GITHUB_SHA) { throw 'Release target was not pinned to the source commit' }
                if ($arguments -contains '--draft=false') { $fixture.Published = $true }
                else { $fixture.UpdatedDraft = $true }
                return
            }
        }
    }
    throw ('Unexpected gh operation in release fixture: ' + ($arguments -join ' '))
}

[System.IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'docs/releases')) | Out-Null
[System.IO.Directory]::CreateDirectory((Join-Path $fixtureRoot 'dist')) | Out-Null
Push-Location $fixtureRoot
try {
    $env:GH_REPO = 'fixture/desktop-pilot'
    $env:GITHUB_SHA = 'a' * 40
    [System.IO.File]::WriteAllText((Join-Path $fixtureRoot 'docs/releases/9.9.9.md'), 'Release fixture')
    foreach ($name in @('DesktopPilot-9.9.9-windows-x64.zip', 'DesktopPilot-9.9.9-source.zip', 'SHA256SUMS.txt')) {
        [System.IO.File]::WriteAllText((Join-Path $fixtureRoot "dist/$name"), "Fixture asset: $name")
    }
    $cases = @(
        @{ Name = 'new'; Error = '' },
        @{ Name = 'draft'; Error = '' },
        @{ Name = 'annotated-tag'; Error = '' },
        @{ Name = 'wrong-tag'; Error = 'release tag points to a different commit' },
        @{ Name = 'published'; Error = 'already published' },
        @{ Name = 'wrong-digest'; Error = 'Uploaded asset verification failed' },
        @{ Name = 'missing-asset'; Error = 'Release assets are incomplete' },
        @{ Name = 'old-head'; Error = 'newer commit is already on main' },
        @{ Name = 'moving-head'; Error = 'Main changed during packaging' }
    )
    foreach ($case in $cases) {
        $fixture = @{
            Scenario = $case.Name; HeadReads = 0; Created = $false; Uploaded = $false
            Published = $false; UpdatedDraft = $false; Calls = [System.Collections.Generic.List[string]]::new()
        }
        $failure = ''
        try { & $publish } catch { $failure = $_.Exception.Message }
        if ($case.Error) {
            if (-not $failure.Contains($case.Error) -or $fixture.Published) {
                throw "Publication guard failed for $($case.Name): $failure"
            }
        } elseif ($failure -or -not $fixture.Published -or -not $fixture.Uploaded) {
            throw "Valid publication failed for $($case.Name): $failure"
        }
        if ($case.Name -eq 'draft' -and (-not $fixture.UpdatedDraft -or $fixture.Created)) {
            throw 'Resuming a draft did not update its commit target'
        }
        Write-Host "PASS publication: $($case.Name)"
    }
    Write-Host ("Release publication: {0} checks passed" -f $cases.Count)
} finally {
    Pop-Location
    $env:GH_REPO = $savedRepository
    $env:GITHUB_SHA = $savedCommit
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force
}
