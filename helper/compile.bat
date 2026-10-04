@echo off
cd /d "%~dp0"
echo ===================================================
echo  Simple-Iscsi Dual-Stage Helper Compiler (MSVC x64)
echo ===================================================
echo.

:: Mencari path Developer Command Prompt/MSBuild dari Visual Studio
set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
call "%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1

if %ERRORLEVEL% neq 0 (
    echo [!] Gagal inisialisasi compiler MSVC!
    pause
    exit /b 1
)

:: 1. Kompilasi Stage 1: helper.cpp (Native Subsystem untuk BootExecute)
echo [1/2] Mengompilasi helper.cpp ke helper.exe [Native Subsystem]...
cl.exe /O2 /GS- /GR- /EHs-c- helper.cpp /link /subsystem:native /entry:NtProcessStartup /NODEFAULTLIB ntdll.lib /out:helper.exe /nologo
if %ERRORLEVEL% equ 0 (
    echo       [+] helper.exe berhasil dibuat.
) else (
    echo       [!] Gagal mengompilasi helper.cpp!
    pause
    exit /b 1
)

:: 2. Kompilasi Stage 2: helper-svc.cpp (Win32 Console/Service Subsystem untuk User-Mode)
echo [2/2] Mengompilasi helper-svc.cpp ke helper-svc.exe [Win32 Service/Startup]...
cl.exe /O2 /EHsc /MT helper-svc.cpp /link /subsystem:console ws2_32.lib iphlpapi.lib advapi32.lib /out:helper-svc.exe /nologo
if %ERRORLEVEL% equ 0 (
    echo       [+] helper-svc.exe berhasil dibuat.
) else (
    echo       [!] Gagal mengompilasi helper-svc.cpp!
    pause
    exit /b 1
)

echo.
echo ===================================================
echo  [+] Sukses! Kedua binary helper berhasil dibuat:
echo      1. helper.exe     [BootExecute Native]
echo      2. helper-svc.exe [User-Mode IP Alignment]
echo ===================================================
if "%1" neq "nopause" (
    pause
)
