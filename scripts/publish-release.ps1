$ErrorActionPreference = 'Stop'
if ($env:GITHUB_SHA -notmatch '^[0-9a-f]{40}$' -or -not $env:GH_REPO) { throw 'Run publication from the repository release workflow' }
$metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Could not read package version' }
$version = ($metadata.packages | Where-Object name -eq 'desktop-pilot').version
$tag = "v$version"
$notes = "docs/releases/$version.md"
if (-not (Test-Path -LiteralPath $notes)) { throw "Release notes are missing: $notes" }
if ((git rev-parse HEAD).Trim() -ne $env:GITHUB_SHA) { throw 'Release checkout does not match the triggering commit' }
$currentHead = gh api "repos/$env:GH_REPO/git/ref/heads/main" --jq '.object.sha'
if ($LASTEXITCODE) { throw 'Could not verify the current main branch' }
if ($currentHead.Trim() -ne $env:GITHUB_SHA) { throw 'A newer commit is already on main; refusing to publish an outdated build' }
# A resumed draft must never attach new binaries to an older tag.
$tagRefs = gh api "repos/$env:GH_REPO/git/matching-refs/tags/$tag" | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Could not verify the release tag' }
$tagRef = @($tagRefs | Where-Object ref -eq "refs/tags/$tag")
if ($tagRef.Count -gt 1) { throw 'Release tag is ambiguous' }
if ($tagRef.Count -eq 1) {
  $target = $tagRef[0].object
  for ($depth = 0; $target.type -eq 'tag' -and $depth -lt 5; $depth++) {
    $annotated = gh api "repos/$env:GH_REPO/git/tags/$($target.sha)" | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Could not resolve the annotated release tag' }
    $target = $annotated.object
  }
  if ($target.type -ne 'commit' -or $target.sha -ne $env:GITHUB_SHA) {
    throw 'The release tag points to a different commit; published history will not be rewritten'
  }
}
$existingText = gh release view $tag --json isDraft,tagName 2>$null
$existing = if ($LASTEXITCODE -eq 0) { $existingText | ConvertFrom-Json } else { $null }
if ($existing -and -not $existing.isDraft) { throw "Release $tag is already published" }
$assets = @("dist/DesktopPilot-$version-windows-x64.zip", "dist/DesktopPilot-$version-source.zip", 'dist/SHA256SUMS.txt')
if (-not $existing) {
  gh release create $tag --draft --target $env:GITHUB_SHA --title "Desktop Pilot $version" --notes-file $notes
  if ($LASTEXITCODE) { throw 'Could not prepare release' }
} else {
  gh release edit $tag --draft=true --target $env:GITHUB_SHA --title "Desktop Pilot $version" --notes-file $notes
  if ($LASTEXITCODE) { throw 'Could not update the release draft target' }
}
gh release upload $tag @assets --clobber
if ($LASTEXITCODE) { throw 'Could not upload release assets' }
# The tag endpoint only exposes published releases; resolve the draft through gh.
$releaseApiUrl = gh release view $tag --json apiUrl --jq '.apiUrl'
$releaseUrlPattern = '^' + [regex]::Escape("https://api.github.com/repos/$env:GH_REPO/releases/") + '[0-9]+$'
if ($LASTEXITCODE -or $releaseApiUrl -notmatch $releaseUrlPattern) { throw 'Could not resolve the release draft' }
$publishedAssets = gh api $releaseApiUrl | ConvertFrom-Json
if ($LASTEXITCODE) { throw 'Could not verify release assets' }
if ($publishedAssets.tag_name -ne $tag -or $publishedAssets.draft -ne $true -or $publishedAssets.target_commitish -ne $env:GITHUB_SHA) {
  throw 'The release draft no longer matches the verified source commit'
}
$expected = @($assets | ForEach-Object { [System.IO.Path]::GetFileName($_) })
if (@($publishedAssets.assets | Where-Object { $_.name -in $expected -and $_.size -gt 0 }).Count -ne $expected.Count) {
  throw 'Release assets are incomplete'
}
foreach ($asset in $assets) {
  $name = [System.IO.Path]::GetFileName($asset)
  $uploaded = @($publishedAssets.assets | Where-Object name -eq $name)
  $digest = 'sha256:' + (Get-FileHash -Algorithm SHA256 -LiteralPath $asset).Hash.ToLowerInvariant()
  if ($uploaded.Count -ne 1 -or $uploaded[0].size -ne (Get-Item -LiteralPath $asset).Length -or $uploaded[0].digest -ne $digest) {
    throw "Uploaded asset verification failed: $name"
  }
}
$currentHead = gh api "repos/$env:GH_REPO/git/ref/heads/main" --jq '.object.sha'
if ($LASTEXITCODE -or $currentHead.Trim() -ne $env:GITHUB_SHA) { throw 'Main changed during packaging; the draft was not published' }
gh release edit $tag --draft=false --latest --target $env:GITHUB_SHA --notes-file $notes
if ($LASTEXITCODE) { throw 'Could not publish release' }
