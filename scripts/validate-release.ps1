param(
    [string]$Tag = $env:GITHUB_REF_NAME,
    [string]$DefaultBranch = $env:DEFAULT_BRANCH,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$Token = $env:GH_TOKEN,
    [switch]$SkipRemoteChecks
)

$ErrorActionPreference = 'Stop'
if ($Tag -cnotmatch '^v([0-9]+\.[0-9]+\.[0-9]+)$') {
    throw "Release tag must be vMAJOR.MINOR.PATCH: $Tag"
}
$version = $Matches[1]
$root = Split-Path $PSScriptRoot -Parent
$metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$manifestVersion = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'veya-desktop').version
if ($version -ne $manifestVersion) {
    throw "Tag $Tag does not match veya-desktop version $manifestVersion"
}
$notes = Join-Path $root "docs\releases\$Tag.md"
if (-not (Test-Path -LiteralPath $notes)) { throw "Release notes required: $notes" }

$tagType = (& git cat-file -t "refs/tags/$Tag" 2>$null)
if ($LASTEXITCODE -ne 0 -or $tagType.Trim() -ne 'tag') {
    throw "Annotated tag required: $Tag"
}
$tagCommit = (& git rev-list -n 1 $Tag).Trim()
if ($LASTEXITCODE -ne 0 -or $tagCommit -ne (& git rev-parse HEAD).Trim()) {
    throw 'Checkout must be the release tag commit'
}

if (-not $SkipRemoteChecks) {
    if (-not $DefaultBranch -or -not $Repository -or -not $Token) {
        throw 'Default branch, repository and GITHUB_TOKEN are required'
    }
    & git fetch origin $DefaultBranch --no-tags
    if ($LASTEXITCODE -ne 0) { throw "Cannot fetch origin/$DefaultBranch" }
    & git merge-base --is-ancestor $tagCommit "origin/$DefaultBranch"
    if ($LASTEXITCODE -ne 0) { throw "Tag commit is not on origin/$DefaultBranch" }

    $uri = "https://api.github.com/repos/$Repository/actions/workflows/ci.yml/runs?head_sha=$tagCommit&status=success&per_page=100"
    $headers = @{
        Authorization = "Bearer $Token"
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
    }
    $runs = Invoke-RestMethod -Uri $uri -Headers $headers
    $passed = @($runs.workflow_runs | Where-Object {
        $_.head_sha -eq $tagCommit -and $_.event -eq 'push' -and $_.conclusion -eq 'success'
    })
    if ($passed.Count -eq 0) {
        throw "No successful push CI run for $tagCommit. Wait for CI on $DefaultBranch before tagging."
    }
}
Write-Host "Release gate passed for $Tag ($tagCommit)"
