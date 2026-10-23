param(
    [switch]$Disable,
    [int]$ExpectedSizeMB = -1
)

$ErrorActionPreference = "Stop"
$paramsPath = "HKLM:\SYSTEM\CurrentControlSet\Services\CCacheX\Parameters"
$pnpPath = "HKLM:\SYSTEM\CCBootPnP"
$computerPath = "HKLM:\SYSTEM\CurrentControlSet\Control\ComputerName\ComputerName"

function Get-Val($path, $name) {
    try {
        $item = Get-ItemProperty -Path $path -Name $name -ErrorAction Stop
        return $item.$name
    } catch {
        return $null
    }
}

$cacheSize = Get-Val $paramsPath "CacheSize"
$clientCache = Get-Val $paramsPath "ClientCache"
$writeBack = Get-Val $paramsPath "WriteBack"
$toLocal = Get-Val $pnpPath "ClientCacheToLocal"

$models = @()
try {
    $models = @(Get-CimInstance Win32_DiskDrive -ErrorAction Stop | Select-Object -ExpandProperty Model)
} catch { }
$modelText = ($models -join " | ")
$vendorOk = $modelText -match "CCBoot|CCDisk|VMware"

Write-Host "== CCacheX check =="
Write-Host ("ComputerName       : " + (Get-Val $computerPath "ComputerName"))
Write-Host ("Disk model(s)      : " + $modelText)
Write-Host ("Vendor match       : " + $(if ($vendorOk) { "YES" } else { "NO (harus CCBoot/CCDisk/VMware)" }))
if ($null -eq $cacheSize) {
    Write-Host "CacheSize (MB)     : (not set; hostname/auto)"
} else {
    Write-Host ("CacheSize (MB)     : " + $cacheSize)
}
Write-Host ("ClientCache        : " + $clientCache)
Write-Host ("WriteBack          : " + $writeBack)
Write-Host ("ClientCacheToLocal : " + $toLocal)

$service = Get-Service -Name CCacheX -ErrorAction SilentlyContinue
if ($service) {
    Write-Host ("Service CCacheX    : " + $service.Status)
} else {
    Write-Host "Service CCacheX    : NOT FOUND"
}

if ($Disable) {
    if (Test-Path $paramsPath) {
        Set-ItemProperty -Path $paramsPath -Name "AutoStart" -Type DWord -Value 0
        Set-ItemProperty -Path $paramsPath -Name "ClientCache" -Type DWord -Value 0
        Write-Host "Cache disabled: AutoStart=0, ClientCache=0. Reboot required."
    } else {
        Write-Warning "Path not found: $paramsPath"
    }
    exit 0
}

$fail = $false
if ($clientCache -ne 1) {
    Write-Warning "ClientCache bukan 1; cache belum aktif."
    $fail = $true
}
if ($writeBack -ne 0) {
    Write-Warning "WriteBack harus 0 untuk mode read-cache."
    $fail = $true
}
if ($toLocal -ne 0) {
    Write-Warning "ClientCacheToLocal harus 0 untuk client diskless."
    $fail = $true
}
if ($clientCache -eq 1 -and -not $vendorOk) {
    Write-Warning "Vendor disk tidak cocok. CCacheX hanya memproses vendor CCBoot/CCDisk/VMware. Set [windows] vendor_id = \"CCBoot\" di config.toml server."
    $fail = $true
}
if ($ExpectedSizeMB -ge 0) {
    if ($null -eq $cacheSize) {
        Write-Warning "CacheSize tidak ada di registry. Ini normal jika ukuran dikirim lewat hostname; bukan berarti cache size 0."
    } elseif ($cacheSize -ne $ExpectedSizeMB) {
        Write-Warning ("CacheSize tidak sesuai: expected " + $ExpectedSizeMB + " MB, actual " + $cacheSize)
        $fail = $true
    }
}

if ($fail) { exit 1 }
Write-Host "OK: cache read-only aktif dan aman."
exit 0
