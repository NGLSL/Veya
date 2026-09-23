param([string]$Nsis = '')

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$artifacts = Join-Path $root 'artifacts'
$buildDir = Join-Path $root 'target\release-package'
New-Item -ItemType Directory -Force -Path $artifacts | Out-Null

& cargo build --locked --release -p veya-desktop --target-dir $buildDir --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw "cargo build failed: $LASTEXITCODE" }

if (-not $Nsis) {
    $Nsis = @(
        (Get-Command makensis.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
        'C:\Program Files (x86)\NSIS\makensis.exe',
        'C:\Program Files\NSIS\makensis.exe'
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
}
if (-not $Nsis -or -not (Test-Path -LiteralPath $Nsis)) {
    throw 'NSIS makensis.exe not found.'
}

$metadata = & cargo metadata --no-deps --format-version 1 --manifest-path (Join-Path $root 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$version = (($metadata | ConvertFrom-Json).packages | Where-Object name -eq 'veya-desktop').version
if (-not $version) { throw 'veya-desktop version not found' }
$binary = Join-Path $buildDir 'release\veya.exe'
if (-not (Test-Path -LiteralPath $binary)) { throw "Binary missing: $binary" }
& $Nsis "/DAPP_VERSION=$version" "/DAPP_BINARY=$binary" (Join-Path $root 'installer\veya.nsi')
if ($LASTEXITCODE -ne 0) { throw "makensis failed: $LASTEXITCODE" }

$installer = Join-Path $artifacts 'veya-setup.exe'
if (-not (Test-Path -LiteralPath $installer)) { throw "Installer missing: $installer" }
Write-Host "Built $installer"
