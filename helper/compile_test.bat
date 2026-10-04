@echo off
cd /d "%~dp0"
set "VS_PATH=C:\Program Files\Microsoft Visual Studio\18\Community"
call "%VS_PATH%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul 2>&1
cl.exe /EHsc test_ip_cleaner.cpp /Fe:test_ip_cleaner.exe /nologo
if %ERRORLEVEL% equ 0 (
    test_ip_cleaner.exe
)
if "%1" neq "nopause" (
    pause
)
