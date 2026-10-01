param([Parameter(Mandatory)][string]$Binary, [string]$Data = 'appdata-current')
$isolatedData = Join-Path $PSScriptRoot $Data
$oldAppData = $env:APPDATA
try {
    $env:APPDATA = $isolatedData
    $process = Start-Process -FilePath $Binary -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $PSScriptRoot "$Data.stdout.log") -RedirectStandardError (Join-Path $PSScriptRoot "$Data.stderr.log")
    $process | Select-Object Id,Path | ConvertTo-Json
} finally {
    $env:APPDATA = $oldAppData
}
