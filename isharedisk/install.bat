@echo off
title iShareDisk Driver Installer
cd /d "%~dp0"

:: Must run as Admin
net session >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [!] Harus dijalankan sebagai Administrator!
    echo     Klik kanan - Run as Administrator
    pause
    exit /b 1
)

echo ========================================
echo   iShareDisk Driver Installer
echo ========================================
echo.

:: 1️⃣ Import Registry
echo [1/3] Importing registry...
regedit /s "reg\HKLM_ISharePnp.reg"
if %ERRORLEVEL% equ 0 ( echo   [+] HKLM_ISharePnp.reg imported ) else ( echo   [!] Gagal import HKLM_ISharePnp.reg )

regedit /s "reg\SERVICE_ISHAREPNP.reg"
if %ERRORLEVEL% equ 0 ( echo   [+] SERVICE_ISHAREPNP.reg imported ) else ( echo   [!] Gagal import SERVICE_ISHAREPNP.reg )
echo.

:: 2️⃣ Copy Driver
echo [2/3] Copying driver...
if not exist "driver\iSharePp.sys" (
    echo   [!] File driver\iSharePp.sys tidak ditemukan!
    pause
    exit /b 1
)
copy /Y "driver\iSharePp.sys" "C:\Windows\System32\drivers\iSharePp.sys"
if %ERRORLEVEL% equ 0 ( echo   [+] iSharePp.sys copied to C:\Windows\System32\drivers\ ) else ( echo   [!] Gagal copy driver! )
echo.

:: 3️⃣ Add UpperFilters
echo [3/3] Adding UpperFilters = ISharePnp...
reg add "HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}" /v UpperFilters /t REG_MULTI_SZ /d ISharePnp /f
if %ERRORLEVEL% equ 0 ( echo   [+] UpperFilters = ISharePnp added ) else ( echo   [!] Gagal add UpperFilters )
echo.

echo ========================================
echo   ✅ Installasi selesai!
echo   Silakan restart komputer.
echo ========================================
pause
