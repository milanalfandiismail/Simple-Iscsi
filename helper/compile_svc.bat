@echo off
cd /d "%~dp0"
echo Memulai Kompilasi helper-svc.cpp ke helper-svc.exe...

set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
call "%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1

cl.exe /O2 /EHsc /MT helper-svc.cpp /link /subsystem:console ws2_32.lib iphlpapi.lib advapi32.lib /out:helper-svc.exe

if %ERRORLEVEL% equ 0 (
    echo ====================================================
    echo  [+] Sukses! File helper-svc.exe berhasil dibuat.
    echo ====================================================
) else (
    echo [!] Gagal mengompilasi helper-svc.cpp!
)
if "%1" neq "nopause" (
    pause
)
