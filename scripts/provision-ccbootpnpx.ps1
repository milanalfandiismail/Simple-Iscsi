[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][string]$VhdPath,
    [Parameter(Mandatory=$true)][string]$MountDrive,
    [string]$DriverSource = (Join-Path $PSScriptRoot '..\CCBootPNPX.sys')
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $VhdPath)) { throw "VHD tidak ditemukan: $VhdPath" }
if (-not (Test-Path -LiteralPath $DriverSource)) { throw "Driver tidak ditemukan: $DriverSource" }

$drive = $MountDrive.TrimEnd(':', '\') + ':'
$sysHive = Join-Path $drive '\Windows\System32\config\SYSTEM'
if (-not (Test-Path -LiteralPath $sysHive)) { throw "Hive SYSTEM tidak ditemukan: $sysHive (VHD belum di-mount / letter salah?)" }

$driversDir = Join-Path $drive '\Windows\System32\drivers'
$dst = Join-Path $driversDir 'CCBootPNPX.sys'
Copy-Item -LiteralPath $DriverSource -Destination $dst -Force
Write-Host "[OK] Driver -> $dst"

$hive = 'HKLM\CCBOOT_SYS'
& reg load $hive $sysHive | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'reg load gagal' }

try {
    $sel = Get-ItemProperty 'HKLM:\CCBOOT_SYS\Select' -ErrorAction Stop
    $csName = 'ControlSet{0:D3}' -f [int]$sel.Current
    $svc = "HKLM:\CCBOOT_SYS\$csName\Services\CCBootPNPX"
    New-Item -Path $svc -Force | Out-Null
    New-ItemProperty -Path $svc -Name 'Type'         -PropertyType DWord        -Value 1 -Force | Out-Null
    New-ItemProperty -Path $svc -Name 'Start'        -PropertyType DWord        -Value 0 -Force | Out-Null
    New-ItemProperty -Path $svc -Name 'ErrorControl' -PropertyType DWord        -Value 1 -Force | Out-Null
    New-ItemProperty -Path $svc -Name 'ImagePath'    -PropertyType ExpandString -Value '\SystemRoot\System32\drivers\CCBootPNPX.sys' -Force | Out-Null
    New-ItemProperty -Path $svc -Name 'Group'        -PropertyType String       -Value 'Boot Bus Extender' -Force | Out-Null

    Write-Host "[OK] Service key: $csName\Services\CCBootPNPX"
    Get-ItemProperty $svc | Select-Object Type, Start, ErrorControl, ImagePath, Group | Format-List

    $ovr = Join-Path $svc 'StartOverride'
    if (Test-Path $ovr) { Write-Warning "StartOverride ada: $((Get-ItemProperty $ovr).Start)" }
}
finally {
    [gc]::Collect()
    [gc]::WaitForPendingFinalizers()
    & reg unload $hive | Out-Null
}

Write-Host '[DONE] Driver + service terpasang. Unmount VHD via diskpart sebelum boot.'
Write-Host '[NOTE] Script ini TIDAK menulis CCBootPnP dan TIDAK install helper/control-plane.'

