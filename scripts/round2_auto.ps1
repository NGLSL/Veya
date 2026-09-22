# Veya Round 2 — scriptable scenarios
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\round2_auto.ps1 -Scenario R2-02
#   powershell -ExecutionPolicy Bypass -File scripts\round2_auto.ps1 -Scenario R2-03
#   powershell -ExecutionPolicy Bypass -File scripts\round2_auto.ps1 -Scenario R2-07
# Requires: target\release\veya.exe (build first)

param(
  [ValidateSet("R2-02", "R2-03", "R2-07")]
  [string]$Scenario = "R2-02",
  [int]$PasteCount = 10,
  [int]$SettleMs = 400
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$veya = Join-Path $root "target\release\veya.exe"
if (-not (Test-Path $veya)) {
  throw "veya.exe not found. Run: cargo build --release"
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public class KeyInject {
  [DllImport("user32.dll")]
  public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
  public const uint KEYEVENTF_KEYUP = 0x2;
  public static void Press(byte vk) { keybd_event(vk, 0, 0, UIntPtr.Zero); Thread.Sleep(40); }
  public static void Release(byte vk) { keybd_event(vk, 0, KEYEVENTF_KEYUP, UIntPtr.Zero); Thread.Sleep(40); }
  public static void CtrlV() { Press(0x11); Press(0x56); Release(0x56); Release(0x11); }
  public static void ShiftInsert() { Press(0xA0); Press(0x2D); Release(0x2D); Release(0xA0); }
}
'@

$log = Join-Path $root "round2_auto.log"
$err = Join-Path $root "round2_auto.err"
Remove-Item $log, $err -ErrorAction SilentlyContinue

Write-Host "Starting Veya ($Scenario)..."
$p = Start-Process -FilePath $veya -RedirectStandardOutput $log -RedirectStandardError $err -PassThru
Start-Sleep -Seconds 2

function Stop-Veya {
  if ($p -and -not $p.HasExited) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 300
  }
}

try {
  switch ($Scenario) {
    "R2-02" {
      Write-Host "R2-02: Copy A -> Copy B -> Paste (expect paste only on B)"
      Set-Clipboard -Value "veya-r2-02-A"
      Start-Sleep -Milliseconds $SettleMs
      Set-Clipboard -Value "veya-r2-02-B"
      Start-Sleep -Milliseconds $SettleMs
      [KeyInject]::CtrlV()
      Start-Sleep -Milliseconds 600
    }
    "R2-03" {
      Write-Host "R2-03: one copy, Ctrl+V x$PasteCount on same record"
      Set-Clipboard -Value "veya-r2-03"
      Start-Sleep -Milliseconds $SettleMs
      for ($i = 1; $i -le $PasteCount; $i++) {
        [KeyInject]::CtrlV()
        Start-Sleep -Milliseconds 250
      }
      Start-Sleep -Milliseconds 400
    }
    "R2-07" {
      Write-Host "R2-07: CJK + multiline + emoji content"
      # Build sample without relying on .ps1 file encoding (PS 5.1 + UTF-8 no BOM is lossy).
      $sample = [System.Text.Encoding]::UTF8.GetString([byte[]](
        0xE4,0xBD,0xA0,0xE5,0xA5,0xBD,0x20,0x56,0x65,0x79,0x61,
        0x0A,
        0xE7,0xAC,0xAC,0xE4,0xBA,0x8C,0xE8,0xA1,0x8C,0x20,
        0xF0,0x9F,0x9A,0x80
      ))
      Write-Host "R2-07 sample: [$sample]"
      Set-Clipboard -Value $sample
      Start-Sleep -Milliseconds $SettleMs
      [KeyInject]::CtrlV()
      Start-Sleep -Milliseconds 400
      [KeyInject]::ShiftInsert()
      Start-Sleep -Milliseconds 400
    }
  }
}
finally {
  try { Stop-Veya } catch { Write-Host "stop: $($_.Exception.Message)" }
}

Write-Host ""
Write-Host "===== LOG (UTF-8) ====="
if (Test-Path $log) {
  [System.IO.File]::ReadAllText($log, [System.Text.Encoding]::UTF8)
} else {
  Write-Host "(no log)"
}
Write-Host "===== ERR ====="
if (Test-Path $err) {
  [System.IO.File]::ReadAllText($err, [System.Text.Encoding]::UTF8)
}
Write-Host ""
Write-Host "Compare against index.html expected patterns, then mark Pass/Fail in the app."
Write-Host "Log file: $log"
