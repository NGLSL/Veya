param(
    [string]$Tag = $env:GITHUB_REF_NAME,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$RunId = $env:GITHUB_RUN_ID
)

$ErrorActionPreference = 'Stop'
if ($Tag -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw "Invalid release tag: $Tag" }
$root = Split-Path $PSScriptRoot -Parent
$notesPath = Join-Path $root "docs\releases\$Tag.md"
$installer = Join-Path $root 'artifacts\veya-setup.exe'
if (-not (Test-Path -LiteralPath $notesPath)) { throw "Release notes missing: $notesPath" }
if (-not (Test-Path -LiteralPath $installer)) { throw "Installer missing: $installer" }

$sha256 = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
$checksumPath = Join-Path $root 'artifacts\veya-setup.exe.sha256'
Set-Content -LiteralPath $checksumPath -Value "$sha256 *veya-setup.exe" -Encoding utf8
$sizeMiB = [math]::Round((Get-Item -LiteralPath $installer).Length / 1MB, 2)
$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve HEAD' }
$curated = Get-Content -Raw -Encoding UTF8 -LiteralPath $notesPath
if ([string]::IsNullOrWhiteSpace($curated)) { throw "Release notes are empty: $notesPath" }
$runLink = if ($Repository -and $RunId) { "https://github.com/$Repository/actions/runs/$RunId" } else { 'local build' }
$body = @(
    $curated.TrimEnd()
    ''
    '## 构建与下载'
    ''
    "- 版本：``$Tag``"
    "- 提交：``$commit``"
    "- Windows 构建：$runLink"
    '- 验证：格式检查、workspace 测试、release 编译、NSIS 安装包构建'
    "- 安装包：``veya-setup.exe``，$sizeMiB MiB"
    "- SHA-256：``$sha256``"
    ''
    '安装器覆盖旧版程序并保留 `%APPDATA%\Veya` 中的用户数据。'
) -join "`n"
Set-Content -LiteralPath (Join-Path $root 'release-body.md') -Value $body -Encoding utf8
Write-Host "Release body and checksum ready for $Tag"
