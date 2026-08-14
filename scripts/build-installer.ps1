param(
  [switch]$SkipAssets
)

$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)

if (-not $SkipAssets) {
  python -m pip install --quiet pillow
  python scripts/generate-installer-assets.py
}

npm run tauri build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$nsis = Get-ChildItem "src-tauri\target\release\bundle\nsis\*-setup.exe" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $nsis) {
  Write-Error "NSIS installer was not produced. Check the Tauri build log."
}

New-Item -ItemType Directory -Force -Path "release" | Out-Null
$dest = Join-Path "release" "Aether-Setup-$($nsis.Name -replace '.*?(\d+\.\d+\.\d+).*','$1').exe"
if ($nsis.Name -match "(\d+\.\d+\.\d+)") {
  $dest = Join-Path "release" "Aether-Setup-$($Matches[1]).exe"
} else {
  $dest = Join-Path "release" "Aether-Setup.exe"
}
Copy-Item $nsis.FullName $dest -Force
Write-Host "Installer ready: $dest"
Write-Host "Upload that file to itch.io (Windows) and/or GitHub Releases."
