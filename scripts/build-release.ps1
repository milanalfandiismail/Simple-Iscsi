# Build release Simple-Iscsi dan salin UI agar tidak lupa.
# Pemakaian: powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Push-Location $root
try {
    npm run build:css
    if ($LASTEXITCODE -ne 0) { throw "npm run build:css gagal" }

    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release gagal" }

    $src = Join-Path $root "assets\ui"
    $dst = Join-Path $root "target\release\ui"
    if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
    Copy-Item $src $dst -Recurse
    Write-Host "Release siap: target\release\rust-iscsi-server.exe + target\release\ui"
    Write-Host "Copy exe dan folder ui/ ke server, lalu jalankan dari folder yang sama."
} finally {
    Pop-Location
}
