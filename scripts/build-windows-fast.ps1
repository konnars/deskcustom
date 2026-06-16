# Быстрая сборка Deskcustom на Windows БЕЗ NSIS-установщика (~3–8 мин вместо ~15).
# Нужно: Git, Rust, Node.js, Visual Studio Build Tools (C++).

$ErrorActionPreference = "Stop"
Set-Location "$PSScriptRoot\..\apps\desktop"

Write-Host "==> npm install"
npm install

Write-Host "==> npm run icons"
npm run icons

Write-Host "==> Fast build (exe only, no installer)..."
npm run build:fast

$exeCandidates = @(
    "..\..\target\release\deskcustom-app.exe",
    "src-tauri\target\release\deskcustom-app.exe"
)

$exe = $null
foreach ($path in $exeCandidates) {
    if (Test-Path $path) {
        $exe = Resolve-Path $path
        break
    }
}

if (-not $exe) {
    Write-Error "deskcustom-app.exe not found in target/release/"
}

$dist = "..\..\dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item $exe "$dist\Deskcustom.exe" -Force

Write-Host ""
Write-Host "Готово: $dist\Deskcustom.exe"
Write-Host ""
Write-Host "Обновить без установщика:"
Write-Host "  1. Закрой Deskcustom (и в трее, если есть)"
Write-Host "  2. Скопируй dist\Deskcustom.exe поверх старого .exe"
Write-Host "     Обычно: $env:LOCALAPPDATA\Programs\Deskcustom\Deskcustom.exe"
Write-Host "     или:    C:\Program Files\Deskcustom\Deskcustom.exe"
Write-Host "  3. Запусти Deskcustom снова"
Write-Host ""
Write-Host "Или просто запусти dist\Deskcustom.exe для проверки."
