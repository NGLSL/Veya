param([Parameter(Mandatory = $true)][string]$Binary)

$ErrorActionPreference = 'Stop'
$path = (Resolve-Path -LiteralPath $Binary).Path
$stream = [IO.File]::OpenRead($path)
try {
    $reader = [IO.BinaryReader]::new($stream)
    if ($stream.Length -lt 0x40) { throw "Not a PE executable: $path" }
    $stream.Position = 0x3c
    $peOffset = $reader.ReadInt32()
    if ($peOffset -lt 0x40 -or $peOffset + 94 -gt $stream.Length) {
        throw "Invalid PE header: $path"
    }
    $stream.Position = $peOffset
    if ($reader.ReadUInt32() -ne 0x00004550) { throw "Invalid PE signature: $path" }
    $stream.Position = $peOffset + 24
    if ($reader.ReadUInt16() -notin @(0x10b, 0x20b)) {
        throw "Unsupported PE optional header: $path"
    }
    $stream.Position = $peOffset + 24 + 68
    $subsystem = $reader.ReadUInt16()
    if ($subsystem -ne 2) {
        throw "Veya must use the Windows GUI subsystem (2); found $subsystem in $path"
    }
}
finally {
    $stream.Dispose()
}
Write-Host "Windows GUI subsystem verified: $path"
