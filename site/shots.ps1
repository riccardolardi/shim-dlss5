param([string]$Out = "C:\Users\ricca\Code\shim-dlss5\site\img")
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
}
"@
[W]::SetProcessDPIAware() | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null

$env:SHIM_DATA_DIR = Join-Path $env:TEMP "shim-demo-data"; $p = Start-Process "C:\Users\ricca\Code\shim-dlss5\target\release\shim.exe" -PassThru
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 40 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 500; $p.Refresh(); $h = $p.MainWindowHandle }
if ($h -eq [IntPtr]::Zero) { throw "no window" }
[W]::MoveWindow($h, 40, 20, 1500, 1360, $true) | Out-Null
[W]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Seconds 4

function Shot([string]$name) {
  [W]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 1200
  $r = New-Object W+RECT
  # Extended frame bounds = the visible window without the invisible resize border.
  [W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null
  $top = 46; $w = $r.R - $r.L; $hh = $r.B - $r.T - $top
  $bmp = New-Object System.Drawing.Bitmap $w, $hh
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T + $top, 0, 0, $bmp.Size)
  $g.Dispose()
  $path = Join-Path $Out "$name.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  "saved $path ($w x $hh)"
}

$root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
function Click([string]$name) {
  $byName = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::NameProperty), $name; $isButton = New-Object System.Windows.Automation.PropertyCondition ([System.Windows.Automation.AutomationElement]::ControlTypeProperty), ([System.Windows.Automation.ControlType]::Button); $cond = New-Object System.Windows.Automation.AndCondition $byName, $isButton
  $el = $null
  for ($i = 0; $i -lt 20 -and -not $el; $i++) { $el = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond); if (-not $el) { Start-Sleep -Milliseconds 300 } }
  if (-not $el) { throw "element not found: $name" }
  $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
  Start-Sleep -Milliseconds 1500
}

Shot "library"
Click "S.T.A.L.K.E.R. 2: Heart of Chornobyl"; Shot "game"
Stop-Process -Id $p.Id


