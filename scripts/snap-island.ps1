param(
  [string]$Out = "$env:TEMP\island.png"
)

Add-Type -AssemblyName System.Drawing

$code = @'
using System;
using System.Runtime.InteropServices;
public class IslandWin {
  public delegate bool Proc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Proc p, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public struct RECT { public int L, T, R, B; }
  // Island host is the small always-on-top window pinned near the top of the screen.
  public static IntPtr Host(uint target) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == target && IsWindowVisible(h)) {
        RECT r; GetWindowRect(h, out r);
        int w = r.R - r.L, ht = r.B - r.T;
        if (r.T < 60 && w > 100 && w < 900 && ht > 100) found = h;
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static RECT Rect(IntPtr h) { RECT r; GetWindowRect(h, out r); return r; }
  public static void Shot(IntPtr h, System.Drawing.Graphics g) {
    IntPtr hdc = g.GetHdc();
    PrintWindow(h, hdc, 2); // PW_RENDERFULLCONTENT
    g.ReleaseHdc(hdc);
  }
}
'@
Add-Type -TypeDefinition $code -Language CSharp -ReferencedAssemblies System.Drawing

$proc = Get-Process aether -ErrorAction Stop | Select-Object -First 1
$hwnd = [IslandWin]::Host([uint32]$proc.Id)
if ($hwnd -eq [IntPtr]::Zero) { throw "Island host window not found" }

$r = [IslandWin]::Rect($hwnd)
$w = $r.R - $r.L
$h = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::FromArgb(255, 40, 44, 52))
[IslandWin]::Shot($hwnd, $g)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "$Out (${w}x${h} at $($r.L),$($r.T))"
