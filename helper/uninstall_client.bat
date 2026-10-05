@echo off
setlocal EnableDelayedExpansion
title Simple-Iscsi Diskless Client Helper Uninstaller
cd /d "%~dp0"

:: Validasi hak akses Administrator
net session >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [!] Meminta hak akses Administrator via UAC...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

echo ===================================================
echo   Simple-Iscsi Client Helper Uninstaller
echo ===================================================
echo.

:: 1. Hentikan dan Hapus Service SimpleIscsiHelper
echo [1/5] Menghentikan dan menghapus Windows Service...
sc stop SimpleIscsiHelper >nul 2>&1
sc delete SimpleIscsiHelper >nul 2>&1
taskkill /F /IM helper-svc.exe >nul 2>&1
taskkill /F /IM helper.exe >nul 2>&1

:: 2. Hapus Biner dari System32
echo [2/5] Menghapus biner dari %SystemRoot%\System32...
if exist "%SystemRoot%\System32\helper.exe" del /F /Q "%SystemRoot%\System32\helper.exe" >nul 2>&1
if exist "%SystemRoot%\System32\helper-svc.exe" del /F /Q "%SystemRoot%\System32\helper-svc.exe" >nul 2>&1

:: 3. Kembalikan BootExecute ke default Windows
echo [3/5] Mengembalikan BootExecute ke default...
powershell -NoProfile -Command "Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -Name 'BootExecute' -Value @('autocheck autochk *')" >nul 2>&1
if %ERRORLEVEL% neq 0 (
    reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager" /v BootExecute /t REG_MULTI_SZ /d "autocheck autochk *" /f >nul
)

:: 4. Hapus Startup Run Key
echo [4/5] Menghapus Startup Run Key...
reg delete "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v "SimpleIscsiHelper" /f >nul 2>&1

:: 5. Hapus Penanda SimpleIscsiBoot & Parameter Tambahan
echo [5/5] Membersihkan registri helper...
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot" /f >nul 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "WaitForNetworkAtBoot" /f >nul 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "DelayForNetworkAtBoot" /f >nul 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "LinkDownTime" /f >nul 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "MaxRequestHoldTime" /f >nul 2>&1
reg delete "HKLM\SYSTEM\CurrentControlSet\Services\msiscsi\Parameters" /v "LinkDownTime" /f >nul 2>&1

echo.
echo ===================================================
echo   [+] Helper Berhasil Di-Uninstall dari Sistem!
echo ===================================================
echo.

pause
