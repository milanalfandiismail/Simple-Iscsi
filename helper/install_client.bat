@echo off
title Diskless Client Helper Installer
cd /d "%~dp0"

net session >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [!] Harus dijalankan sebagai Administrator!
    pause
    exit /b 1
)

echo ===================================================
echo   Memasang iSharePnp Driver + BootExecute Helper
echo ===================================================
echo.

:: 1. Copy Files
echo [1/3] Menyalin file driver dan helper...
if exist "..\isharedisk\driver\iSharePp.sys" (
    copy /Y "..\isharedisk\driver\iSharePp.sys" "C:\Windows\System32\drivers\iSharePp.sys" >nul
) else if exist "iSharePp.sys" (
    copy /Y "iSharePp.sys" "C:\Windows\System32\drivers\iSharePp.sys" >nul
)

if exist "helper.exe" (
    copy /Y "helper.exe" "C:\Windows\System32\helper.exe" >nul
)
echo       [+] File iSharePp.sys dan helper.exe disalin.

:: 2. Daftarkan Service iSharePnp
echo [2/3] Mendaftarkan service iSharePnp...
if exist "..\isharedisk\reg\SERVICE_ISHAREPNP.reg" (
    regedit /s "..\isharedisk\reg\SERVICE_ISHAREPNP.reg"
)
echo       [+] Service iSharePnp terdaftar.

:: 3. Daftarkan helper.exe ke BootExecute
echo [3/3] Mendaftarkan helper.exe ke BootExecute...
reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager" /v BootExecute /t REG_MULTI_SZ /d "autocheck autochk *\0helper.exe" /f >nul
echo       [+] helper.exe aktif di BootExecute.

echo.
echo ===================================================
echo   [+] Instalasi Selesai! Client siap di-booting.
echo ===================================================
pause
