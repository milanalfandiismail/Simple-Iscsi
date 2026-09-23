@echo off
cd /d "%~dp0"
echo Memulai Kompilasi helper.cpp ke helper.exe (Native Subsystem)...

:: Mencari path Developer Command Prompt/MSBuild dari Visual Studio
set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
call "%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat" x64

if %ERRORLEVEL% neq 0 (
    echo [!] Gagal inisialisasi compiler MSVC!
    pause
    exit /b 1
)

:: Kompilasi menggunakan cl
:: /GS- : menonaktifkan buffer security check agar tidak butuh __security_cookie
:: /GR- : menonaktifkan RTTI (Run-Time Type Information)
:: /EHs-c- : menonaktifkan Exception Handling C++
:: /NODEFAULTLIB : jangan hubungkan library C/C++ standar (karena murni native)
cl.exe /O2 /GS- /GR- /EHs-c- helper.cpp /link /subsystem:native /entry:NtProcessStartup /NODEFAULTLIB ntdll.lib

if %ERRORLEVEL% equ 0 (
    echo.
    echo ===========================================
    echo  [+] Sukses! File helper.exe telah dibuat.
    echo ===========================================
) else (
    echo [!] Gagal mengompilasi helper.cpp!
)
pause
