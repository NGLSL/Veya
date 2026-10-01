param([string]$Name = 'screen', [switch]$VeyaOnly, [long]$WindowHwnd = 0)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
if ($VeyaOnly) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public class WindowRectForValidation {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls,string title);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
}
'@
    $r = New-Object WindowRectForValidation+Rect
    $handle = if ($WindowHwnd) { [IntPtr]$WindowHwnd } else { [WindowRectForValidation]::FindWindow($null,'Veya') }
    [WindowRectForValidation]::GetWindowRect($handle,[ref]$r) | Out-Null
    $bounds = New-Object System.Drawing.Rectangle($r.Left,$r.Top,($r.Right-$r.Left),($r.Bottom-$r.Top))
}
$image = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($image)
try {
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $image.Save((Join-Path $PSScriptRoot "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
    $graphics.Dispose()
    $image.Dispose()
}
