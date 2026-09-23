@echo off
cd /d "%~dp0"
echo Memulai Kompilasi dan Eksekusi Unit Test IP Cleaner...

set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
call "%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1

if %ERRORLEVEL% neq 0 (
    echo [!] Gagal inisialisasi compiler MSVC!
    exit /b 1
)

cl.exe /O2 /EHsc test_ip_cleaner.cpp /Fe:test_ip_cleaner.exe >nul 2>&1

if %ERRORLEVEL% equ 0 (
    test_ip_cleaner.exe
) else (
    echo [!] Gagal mengompilasi test_ip_cleaner.cpp!
    exit /b 1
)
