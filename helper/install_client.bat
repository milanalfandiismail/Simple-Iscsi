@echo off
setlocal EnableDelayedExpansion
title Simple-Iscsi Diskless Client Helper Installer
cd /d "%~dp0"

:: Validasi hak akses Administrator
net session >nul 2>&1
if %ERRORLEVEL% neq 0 (
    echo [!] Meminta hak akses Administrator via UAC...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "Start-Process cmd.exe -ArgumentList '/k cd /d \"%~dp0\" && \"%~nx0\"' -Verb RunAs"
    exit /b
)

echo ===================================================
echo   Simple-Iscsi Dual-Stage Client Helper Installer
echo ===================================================
echo.

:: Hentikan proses lama jika sedang berjalan agar file tidak terkunci
taskkill /F /IM helper-svc.exe >nul 2>&1
sc stop SimpleIscsiHelper >nul 2>&1

:: 1. Copy Stage 1 (helper.exe) ke System32
echo [1/5] Menyalin helper.exe ke %SystemRoot%\System32...
if exist "helper.exe" (
    copy /Y "helper.exe" "%SystemRoot%\System32\helper.exe" >nul
    if !ERRORLEVEL! equ 0 (
        echo       [+] helper.exe disalin ke %SystemRoot%\System32.
    ) else (
        echo       [!] GAGAL menyalin helper.exe! Pastikan file tidak sedang terkunci.
        pause
        exit /b 1
    )
) else (
    echo       [!] ERROR: helper.exe tidak ditemukan di folder ini!
    echo           Silakan jalankan compile.bat terlebih dahulu.
    pause
    exit /b 1
)

:: 2. Copy Stage 2 (helper-svc.exe) ke System32
echo [2/5] Menyalin helper-svc.exe ke %SystemRoot%\System32...
if exist "helper-svc.exe" (
    copy /Y "helper-svc.exe" "%SystemRoot%\System32\helper-svc.exe" >nul
    if !ERRORLEVEL! equ 0 (
        echo       [+] helper-svc.exe disalin ke %SystemRoot%\System32.
    ) else (
        echo       [!] GAGAL menyalin helper-svc.exe! Pastikan file tidak sedang terkunci.
        pause
        exit /b 1
    )
) else (
    echo       [!] ERROR: helper-svc.exe tidak ditemukan di folder ini!
    echo           Silakan jalankan compile.bat terlebih dahulu.
    pause
    exit /b 1
)

:: 3. Daftarkan helper.exe ke BootExecute (Stage 1) secara aman via PowerShell
echo [3/5] Mendaftarkan helper.exe ke BootExecute...
powershell -NoProfile -Command "Set-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager' -Name 'BootExecute' -Value @('autocheck autochk *', 'helper.exe')" >nul 2>&1
if %ERRORLEVEL% equ 0 (
    echo       [+] helper.exe berhasil didaftarkan di BootExecute.
) else (
    reg add "HKLM\SYSTEM\CurrentControlSet\Control\Session Manager" /v BootExecute /t REG_MULTI_SZ /s "," /d "autocheck autochk *,helper.exe" /f >nul
    echo       [+] helper.exe didaftarkan di BootExecute (via reg fallback).
)

:: 4. Daftarkan helper-svc.exe sebagai Windows Service dan Startup Run Key (Stage 2)
echo [4/5] Mendaftarkan helper-svc.exe sebagai Service dan Startup...
sc query SimpleIscsiHelper >nul 2>&1
if %ERRORLEVEL% equ 0 (
    sc config SimpleIscsiHelper binPath= "%SystemRoot%\System32\helper-svc.exe" start= auto DisplayName= "Simple-Iscsi Network Alignment Helper" >nul
    echo       [+] Windows Service SimpleIscsiHelper dikonfigurasi ulang - Start Auto.
) else (
    sc create SimpleIscsiHelper binPath= "%SystemRoot%\System32\helper-svc.exe" start= auto DisplayName= "Simple-Iscsi Network Alignment Helper" >nul
    if !ERRORLEVEL! equ 0 (
        echo       [+] Windows Service SimpleIscsiHelper berhasil dibuat - Start Auto.
    ) else (
        echo       [-] Pembuatan Windows Service dilewati - menggunakan Run Key fallback.
    )
)

reg add "HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run" /v "SimpleIscsiHelper" /t REG_SZ /d "%SystemRoot%\System32\helper-svc.exe /run" /f >nul
if %ERRORLEVEL% equ 0 (
    echo       [+] Startup Run Key SimpleIscsiHelper berhasil didaftarkan.
)

:: 5. Konfigurasi Parameter iScsiPrt & msiscsi (WaitForNetworkAtBoot & Fast-Boot Delay)
echo [5/5] Mengonfigurasi parameter iScsiPrt (Fast Boot Tuning)...
reg add "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "WaitForNetworkAtBoot" /t REG_DWORD /d 1 /f >nul
reg add "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "DelayForNetworkAtBoot" /t REG_DWORD /d 5 /f >nul
reg add "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "LinkDownTime" /t REG_DWORD /d 60 /f >nul
reg add "HKLM\SYSTEM\CurrentControlSet\Services\iScsiPrt\Parameters" /v "MaxRequestHoldTime" /t REG_DWORD /d 60 /f >nul
reg add "HKLM\SYSTEM\CurrentControlSet\Services\msiscsi\Parameters" /v "LinkDownTime" /t REG_DWORD /d 60 /f >nul
if %ERRORLEVEL% equ 0 (
    echo       [+] Parameter iScsiPrt WaitForNetworkAtBoot=1 dan DelayForNetworkAtBoot=5 berhasil dipasang.
)

echo ===================================================
echo   [+] Instalasi Selesai!
echo   Client diskless telah dikonfigurasi dengan:
echo     1. Stage 1: BootExecute Native Helper
echo     2. Stage 2: User-Mode IP Purge Companion
echo     3. Fast-Boot: iScsiPrt WaitForNetworkAtBoot Tuning
echo.
echo   Saat client boot, hostname dan IP akan disinkronkan
echo   mengikuti DHCP/iBFT secara mulus dan aman!
echo ===================================================

if "%1" neq "nopause" (
    pause
)
