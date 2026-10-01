param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$Binary,
    [string]$Version = ''
)

# This destructive integration check belongs only on a fresh hosted CI runner.
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    throw 'Installer smoke testing is allowed only on an isolated GitHub Actions runner.'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The disposable runner must have administrator rights.'
}
$runnerTemp = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\')
if (-not (Test-Path -LiteralPath $runnerTemp -PathType Container)) { throw 'RUNNER_TEMP does not exist.' }
$testRoot = Join-Path $runnerTemp ('veya-installer-' + [Guid]::NewGuid().ToString('N'))
function Assert-OwnedPath([string]$Path) {
    $absolute = [IO.Path]::GetFullPath($Path)
    if (-not $absolute.StartsWith($testRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escapes this test's unique root: $absolute"
    }
    return $absolute
}
function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Read-Registry([string]$Name, [Microsoft.Win32.RegistryView]$View) {
    $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $View)
    try {
        $key = $hive.OpenSubKey($Name)
        if (-not $key) { return $null }
        try {
            $values = @{}
            foreach ($valueName in $key.GetValueNames()) { $values[$valueName] = $key.GetValue($valueName) }
            return $values
        }
        finally { $key.Dispose() }
    }
    finally { $hive.Dispose() }
}
$registryNames = @('Software\Veya', 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya')
$views = @([Microsoft.Win32.RegistryView]::Registry32, [Microsoft.Win32.RegistryView]::Registry64)
$desktop = [Environment]::GetFolderPath('DesktopDirectory')
$programs = [Environment]::GetFolderPath('Programs')
$desktopLink = Join-Path $desktop 'Veya.lnk'
$menuFolder = Join-Path $programs 'Veya'
$menuLink = Join-Path $menuFolder 'Veya.lnk'
# Guard both per-user and common shortcuts before invoking an installer that kills
# every process named veya.exe. No existing installation is safe to touch here.
$existingPaths = @($desktopLink, $menuFolder,
    (Join-Path ([Environment]::GetFolderPath('CommonDesktopDirectory')) 'Veya.lnk'),
    (Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'Veya'))
foreach ($path in $existingPaths) {
    Assert-True (-not (Test-Path -LiteralPath $path)) "Existing Veya shortcut: $path"
}
foreach ($view in $views) {
    foreach ($name in $registryNames) {
        Assert-True ($null -eq (Read-Registry $name $view)) "Existing Veya registration: $view/$name"
    }
}
Assert-True (-not (Get-Process -Name 'veya' -ErrorAction SilentlyContinue)) 'An existing Veya process is running.'
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$Binary = (Resolve-Path -LiteralPath $Binary).Path
if (-not $Version) {
    $binaryVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($Binary)
    $Version = $binaryVersion.ProductVersion
    if (-not $Version) { $Version = $binaryVersion.FileVersion }
    Assert-True ($Version -match '^(\d+\.\d+\.\d+)(?:\.0)?$') "Unsupported binary version: $Version"
    $Version = $Matches[1]
}
$expectedHash = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash
$expectedIconHash = (Get-FileHash -LiteralPath (Join-Path (Split-Path $PSScriptRoot -Parent) 'icons\icon.ico') -Algorithm SHA256).Hash
$originalAppData = $env:APPDATA
$evidence = [ordered]@{
    version = $Version; root = $testRoot; binary_sha256 = $expectedHash
    success = $false; stages = @(); limitation = 'Database bytes are verified unchanged by packaging; application migration and reading are not exercised.'
}
$installAttempted = $false
$installDir = Assert-OwnedPath (Join-Path $testRoot 'installed')
$installedExe = Assert-OwnedPath (Join-Path $installDir 'veya.exe')
$uninstaller = Assert-OwnedPath (Join-Path $installDir 'uninstall.exe')
$evidencePath = Assert-OwnedPath (Join-Path $testRoot 'evidence.json')

function Invoke-Setup([string]$Setup) {
    Assert-True (-not (Get-Process -Name 'veya' -ErrorAction SilentlyContinue)) 'Unexpected Veya process before installation.'
    Assert-OwnedPath $installDir | Out-Null
    # NSIS requires /D to be the final, unquoted argument, including for spaces.
    $process = Start-Process -FilePath $Setup -ArgumentList "/S /D=$installDir" -WindowStyle Hidden -Wait -PassThru
    Assert-True ($process.ExitCode -eq 0) "Installer exited with $($process.ExitCode)."
}
function Check-Installation([string]$ExpectedVersion, [string]$Hash = '') {
    Assert-True (Test-Path -LiteralPath $installedExe -PathType Leaf) 'Installed executable missing.'
    Assert-True (Test-Path -LiteralPath $uninstaller -PathType Leaf) 'Uninstaller missing.'
    $fileVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($installedExe).FileVersion
    Assert-True ($fileVersion -eq $ExpectedVersion -or $fileVersion -eq "$ExpectedVersion.0") "Unexpected PE version: $fileVersion"
    if ($Hash) {
        Assert-True ((Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash -eq $Hash) 'Installed executable hash differs from release binary.'
    }
    $registration = Read-Registry $registryNames[0] ([Microsoft.Win32.RegistryView]::Registry32)
    $uninstallRegistration = Read-Registry $registryNames[1] ([Microsoft.Win32.RegistryView]::Registry32)
    Assert-True ($null -ne $registration -and $null -ne $uninstallRegistration) '32-bit HKLM installer registration missing.'
    Assert-True ($registration.InstallLocation -eq $installDir) 'Installation path registration mismatch.'
    Assert-True ($uninstallRegistration.InstallLocation -eq $installDir) 'Uninstall path registration mismatch.'
    Assert-True ($uninstallRegistration.DisplayVersion -eq $ExpectedVersion) 'Registered version mismatch.'
    Assert-True ($uninstallRegistration.UninstallString -eq ('"' + $uninstaller + '"')) 'Uninstall command mismatch.'
    $shell = New-Object -ComObject WScript.Shell
    $icons = @()
    foreach ($linkPath in @($desktopLink, $menuLink)) {
        Assert-True (Test-Path -LiteralPath $linkPath -PathType Leaf) "Shortcut missing: $linkPath"
        $link = $shell.CreateShortcut($linkPath)
        Assert-True ($link.TargetPath -eq $installedExe) "Shortcut target mismatch: $linkPath"
        $icon = ($link.IconLocation -replace ',\s*\d+$', '').Trim('"')
        Assert-OwnedPath $icon | Out-Null
        Assert-True (Test-Path -LiteralPath $icon -PathType Leaf) "Shortcut icon missing: $icon"
        $icons += $icon
    }
    Assert-True ($icons[0] -eq $icons[1]) 'Shortcut icons differ.'
    return [ordered]@{ version = $fileVersion; installed_sha256 = (Get-FileHash -LiteralPath $installedExe).Hash; icon = $icons[0]; icon_sha256 = (Get-FileHash -LiteralPath $icons[0]).Hash }
}
function Uninstall-Owned {
    Assert-OwnedPath $uninstaller | Out-Null
    if (-not (Test-Path -LiteralPath $uninstaller)) { throw 'Owned uninstaller missing; cannot safely clean up.' }
    foreach ($name in $registryNames) {
        $registration = Read-Registry $name ([Microsoft.Win32.RegistryView]::Registry32)
        if ($registration -and $registration.InstallLocation -ne $installDir) { throw 'Registration ownership changed; cleanup refused.' }
    }
    Assert-True (-not (Get-Process -Name 'veya' -ErrorAction SilentlyContinue)) 'Unexpected Veya process before uninstall.'
    # _?= prevents the NSIS uninstaller from copying itself to an untracked path.
    $process = Start-Process -FilePath $uninstaller -ArgumentList "/S _?=$installDir" -WindowStyle Hidden -Wait -PassThru
    Assert-True ($process.ExitCode -eq 0) "Uninstaller exited with $($process.ExitCode)."
    # NSIS can leave its running executable behind with _?=. Remove ONLY this
    # owned file after the process has ended, as the normal bootstrap would.
    if (Test-Path -LiteralPath $uninstaller) { Remove-Item -LiteralPath (Assert-OwnedPath $uninstaller) -Force }
    if ((Test-Path -LiteralPath $installDir) -and -not (Get-ChildItem -LiteralPath $installDir -Force)) {
        Remove-Item -LiteralPath (Assert-OwnedPath $installDir) -Force
    }
}

try {
    New-Item -ItemType Directory -Path $testRoot | Out-Null
    # Some fresh runners have no physical desktop directory yet. Only create
    # these known per-user shell directories inside this disposable profile.
    foreach ($shellDirectory in @($desktop, $programs)) {
        if (-not (Test-Path -LiteralPath $shellDirectory)) {
            $profileRoot = [IO.Path]::GetFullPath($env:USERPROFILE).TrimEnd('\') + '\'
            Assert-True ([IO.Path]::GetFullPath($shellDirectory).StartsWith($profileRoot, [StringComparison]::OrdinalIgnoreCase)) 'Shell directory is outside the disposable user profile.'
            New-Item -ItemType Directory -Path $shellDirectory -Force | Out-Null
        }
    }
    $env:APPDATA = Assert-OwnedPath (Join-Path $testRoot 'appdata')
    $dataDir = Assert-OwnedPath (Join-Path $env:APPDATA 'Veya')
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    $database = Assert-OwnedPath (Join-Path $dataDir 'veya.db')
    $fixture = Assert-OwnedPath (Join-Path $testRoot 'fixture.py')
    @'
import base64, hashlib, json, pathlib, sqlite3, sys
db = pathlib.Path(sys.argv[1])
local_file = db.parent / 'retained-file.txt'
local_file.write_text('installer retention fixture', encoding='utf-8')
con = sqlite3.connect(db)
con.executescript('''
CREATE TABLE application (exe TEXT PRIMARY KEY, display_name TEXT NOT NULL, path TEXT NOT NULL DEFAULT '', icon BLOB, excluded INTEGER NOT NULL DEFAULT 0);
CREATE TABLE clipboard_record (sequence INTEGER PRIMARY KEY, content_type TEXT NOT NULL, content TEXT NOT NULL, content_hash TEXT NOT NULL, source_app TEXT NOT NULL, source_pid INTEGER NOT NULL, source_window TEXT NOT NULL DEFAULT '', source_confidence TEXT NOT NULL, created_at_ms INTEGER NOT NULL, pinned INTEGER NOT NULL DEFAULT 0, payload BLOB, image_width INTEGER, image_height INTEGER);
CREATE TABLE paste_trigger (id INTEGER PRIMARY KEY AUTOINCREMENT, clipboard_record_id INTEGER NOT NULL REFERENCES clipboard_record(sequence) ON DELETE CASCADE, target_app TEXT NOT NULL, target_pid INTEGER NOT NULL, target_window TEXT NOT NULL DEFAULT '', method TEXT NOT NULL, confidence TEXT NOT NULL DEFAULT 'hotkey-observed', triggered_at_ms INTEGER NOT NULL);
CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
''')
png = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a5xkAAAAASUVORK5CYII=')
for seq, kind, content, payload, width, height in [(1,'text','retained pinned text',None,None,None),(2,'files',str(local_file),json.dumps([str(local_file)]).encode(),None,None),(3,'image','[image]',png,1,1)]:
    con.execute('INSERT INTO clipboard_record VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)',(seq,kind,content,hashlib.sha256(content.encode()).hexdigest(),'fixture.exe',0,'fixture','exact',seq,1,payload,width,height))
con.execute('INSERT INTO app_settings VALUES (?,?)',('retention_days','30'))
con.commit()
assert con.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
assert con.execute('SELECT content_type, pinned FROM clipboard_record ORDER BY sequence').fetchall() == [('text',1),('files',1),('image',1)]
assert con.execute('SELECT value FROM app_settings WHERE key=?',('retention_days',)).fetchone()[0] == '30'
con.close()
'@ | Set-Content -LiteralPath $fixture -Encoding UTF8
    & python $fixture $database
    if ($LASTEXITCODE -ne 0) { throw 'SQLite retention fixture creation failed.' }
    $dataHashes = @{}
    foreach ($file in Get-ChildItem -LiteralPath $dataDir -File) { $dataHashes[$file.FullName] = (Get-FileHash -LiteralPath $file.FullName).Hash }
    function Check-Data {
        foreach ($path in $dataHashes.Keys) {
            Assert-True (Test-Path -LiteralPath $path) "Retained data missing: $path"
            Assert-True ((Get-FileHash -LiteralPath $path).Hash -eq $dataHashes[$path]) "Retained data changed: $path"
        }
    }
    $oldInstaller = Assert-OwnedPath (Join-Path $testRoot 'v0.2.1-setup.exe')
    $oldChecksum = Assert-OwnedPath (Join-Path $testRoot 'v0.2.1-setup.exe.sha256')
    $url = 'https://github.com/NGLSL/Veya/releases/download/v0.2.1/veya-setup.exe'
    Invoke-WebRequest -Uri $url -OutFile $oldInstaller
    Invoke-WebRequest -Uri "$url.sha256" -OutFile $oldChecksum
    $checksumText = Get-Content -LiteralPath $oldChecksum -Raw
    Assert-True ($checksumText -match '^\s*([a-fA-F0-9]{64})(?:\s|$)') 'Malformed official v0.2.1 checksum.'
    $oldExpectedHash = $Matches[1].ToUpperInvariant()
    $oldActualHash = (Get-FileHash -LiteralPath $oldInstaller -Algorithm SHA256).Hash
    Assert-True ($oldActualHash -eq $oldExpectedHash) 'Official v0.2.1 checksum mismatch.'
    $evidence.old_installer_sha256 = $oldActualHash
    $installAttempted = $true
    Invoke-Setup $oldInstaller
    $oldState = Check-Installation '0.2.1'
    Check-Data
    $evidence.stages += @{ name = 'install-v0.2.1'; state = $oldState; data_preserved = $true }
    # An obsolete hashed icon is a deterministic upgrade cleanup fixture even
    # if the two release icons happen to have the same content hash.
    $obsoleteIcon = Assert-OwnedPath (Join-Path $installDir 'veya-icon-obsolete-fixture.ico')
    Copy-Item -LiteralPath $oldState.icon -Destination $obsoleteIcon
    Invoke-Setup $Installer
    $newState = Check-Installation $Version $expectedHash
    Assert-True ($newState.icon_sha256 -eq $expectedIconHash) 'Installed shortcut icon differs from the current release icon.'
    Assert-True (-not (Test-Path -LiteralPath $obsoleteIcon)) 'Upgrade left the obsolete icon fixture behind.'
    $remainingIcons = @(Get-ChildItem -LiteralPath $installDir -Filter 'veya-icon-*.ico' -File)
    Assert-True ($remainingIcons.Count -eq 1 -and $remainingIcons[0].FullName -eq $newState.icon) 'Upgrade left stale hashed icons behind.'
    Check-Data
    $evidence.stages += @{ name = 'upgrade-current'; state = $newState; data_preserved = $true }
    Uninstall-Owned
    foreach ($path in @($installedExe, $uninstaller, $desktopLink, $menuFolder)) {
        Assert-True (-not (Test-Path -LiteralPath $path)) "Uninstall left a file or shortcut: $path"
    }
    Assert-True (-not (Test-Path -LiteralPath $installDir)) 'Uninstall left installation directory or icons.'
    foreach ($view in $views) {
        foreach ($name in $registryNames) { Assert-True ($null -eq (Read-Registry $name $view)) "Uninstall left registration: $view/$name" }
    }
    Check-Data
    $evidence.stages += @{ name = 'uninstall-current'; data_preserved = $true; registry_shortcuts_binary_icons_removed = $true; running_uninstaller_cleanup = 'owned bootstrap file removed after process exit' }
    $installAttempted = $false
    $evidence.success = $true
}
catch {
    $evidence.error = $_.Exception.Message
    throw
}
finally {
    if ($installAttempted -and (Test-Path -LiteralPath $uninstaller)) {
        try { Uninstall-Owned }
        catch { $evidence.cleanup_error = $_.Exception.Message; Write-Warning "Owned installer cleanup failed: $_" }
    }
    $env:APPDATA = $originalAppData
    if (Test-Path -LiteralPath $testRoot) {
        $evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $evidencePath -Encoding UTF8
        Write-Host ($evidence | ConvertTo-Json -Depth 8)
        Write-Host "INSTALLER_EVIDENCE=$evidencePath"
        if ($env:GITHUB_OUTPUT) { "evidence=$evidencePath" | Add-Content -LiteralPath $env:GITHUB_OUTPUT -Encoding UTF8 }
    }
}
