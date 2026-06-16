# Build Deskcustom Windows installer (run on Windows PC)
# Requires: Rust, Node.js, Visual Studio Build Tools

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\apps\desktop"

Write-Host "==> npm install"
npm install

Write-Host "==> Building NSIS installer..."
npm run build

$bundleDirs = @(
    "..\..\target\release\bundle\nsis",
    "src-tauri\target\release\bundle\nsis"
)
$dist = "..\..\dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$bundle = $null
foreach ($dir in $bundleDirs) {
    if (Test-Path $dir) {
        $bundle = $dir
        break
    }
}

if ($bundle) {
    Copy-Item "$bundle\*.exe" $dist -Force
    Write-Host "Installer copied to dist\"
    Get-ChildItem $dist
} else {
    Write-Error "Bundle not found at $bundle"
}

Write-Host ""
Write-Host "Done! Install Deskcustom:"
Write-Host "  dist\Deskcustom_*-setup.exe"
Write-Host ""
Write-Host "LAN updates:"
Write-Host "  python scripts\local-update-server.py dist"
