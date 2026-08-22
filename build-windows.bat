@echo off
setlocal EnableExtensions
set "BUILD_EXIT_CODE=0"
set "HOLD_WINDOW=1"

rem Use --no-pause when invoking this script from CI or another script.
if /i "%~1"=="--no-pause" set "HOLD_WINDOW=0"

rem Build the Windows NSIS installer and MSI from this repository.
cd /d "%~dp0"

echo.
echo ========================================
echo   OpenClaw Manager - Windows Build
echo ========================================
echo.

where node >nul 2>&1
if errorlevel 1 (
    echo [ERROR] Node.js was not found in PATH.
    echo Install Node.js 24 or newer, then run this script again.
    goto :failed
)

where npm >nul 2>&1
if errorlevel 1 (
    echo [ERROR] npm was not found in PATH.
    goto :failed
)

echo Node:
node --version
echo npm:
rem npm is a .cmd shim on Windows; CALL is required to return to this script.
call npm --version
if errorlevel 1 goto :failed
echo.

if not exist "node_modules\" (
    echo Installing JavaScript dependencies from package-lock.json...
    call npm ci
    if errorlevel 1 goto :failed
    echo.
)

echo Building application and Windows installers...
call npm run tauri:build
if errorlevel 1 goto :failed

echo.
echo Build succeeded. Output files:
dir /b "src-tauri\target\release\bundle\nsis\*.exe" 2>nul
dir /b "src-tauri\target\release\bundle\msi\*.msi" 2>nul
echo.
echo Full output directory:
echo %CD%\src-tauri\target\release\bundle
goto :done

:failed
echo.
echo Build failed. See the error messages above.
set "BUILD_EXIT_CODE=1"

:done
echo.
if "%HOLD_WINDOW%"=="1" (
    echo The build window will stay open for you to review the output.
    echo Type exit and press Enter when you are done.
    cmd /d /k
)
endlocal & exit /b %BUILD_EXIT_CODE%
