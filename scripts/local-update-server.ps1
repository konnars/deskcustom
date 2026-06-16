# Local update server on Windows (same as Python script)
# Usage: .\scripts\local-update-server.ps1 dist

$ErrorActionPreference = "Stop"
$Dist = if ($args.Count -gt 0) { $args[0] } else { "dist" }
$Port = if ($env:DESKCUSTOM_UPDATE_PORT) { $env:DESKCUSTOM_UPDATE_PORT } else { 8765 }
$Version = if ($env:DESKCUSTOM_VERSION) { $env:DESKCUSTOM_VERSION } else { "0.1.1" }

New-Item -ItemType Directory -Force -Path $Dist | Out-Null

$hostIp = (Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike "127.*" -and $_.PrefixOrigin -ne "WellKnown" } |
    Select-Object -First 1).IPAddress
if (-not $hostIp) { $hostIp = "127.0.0.1" }

$installer = Get-ChildItem "$Dist/*-setup.exe", "$Dist/*.msi" -ErrorAction SilentlyContinue | Select-Object -First 1
$manifest = @{
    version = $Version
    notes = "Local LAN update"
    platforms = @{}
}

if ($installer) {
    $manifest.platforms["windows-x86_64"] = @{
        url = "http://${hostIp}:${Port}/$($installer.Name)"
    }
}

$json = $manifest | ConvertTo-Json -Depth 4
Set-Content -Path "$Dist/latest.json" -Value $json -Encoding UTF8
Write-Host "Wrote latest.json:"
Write-Host $json

if (-not $installer) {
    Write-Warning "No *-setup.exe in $Dist — copy the installer from GitHub Actions first."
}

Write-Host ""
Write-Host "Manifest URL for Deskcustom: http://${hostIp}:${Port}/latest.json"
Write-Host "Starting server on port $Port ..."

Set-Location $Dist
python -m http.server $Port --bind 0.0.0.0
