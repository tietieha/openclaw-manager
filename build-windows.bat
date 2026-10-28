@echo off
setlocal EnableExtensions
set "BUILD_EXIT_CODE=0"
set "HOLD_WINDOW=1"

rem Use --no-pause when invoking this script from CI or another script.
if /i "%~1"=="--no-pause" set "HOLD_WINDOW=0"

rem Build the Windows NSIS installer and MSI from this repository.
cd /d "%~dp0"
set "ARTIFACT_DIR=%CD%\artifacts\windows"

echo.
echo ========================================
echo   OpenClaw Manager - Windows Build
echo ========================================
echo.

rem Load MSVC even when this is not a Visual Studio developer terminal.
where cl >nul 2>&1
if errorlevel 1 (
    for /d %%V in ("%ProgramFiles%\Microsoft Visual Studio\*") do (
        for /d %%E in ("%%~fV\*") do (
            if exist "%%~fE\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS64=%%~fE\VC\Auxiliary\Build\vcvars64.bat"
        )
    )
    for /d %%V in ("%ProgramFiles(x86)%\Microsoft Visual Studio\*") do (
        for /d %%E in ("%%~fV\*") do (
            if exist "%%~fE\VC\Auxiliary\Build\vcvars64.bat" set "VCVARS64=%%~fE\VC\Auxiliary\Build\vcvars64.bat"
        )
    )
)
if defined VCVARS64 (
    call "%VCVARS64%"
    if errorlevel 1 goto :failed
)

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

if not exist "%ARTIFACT_DIR%\nsis\" mkdir "%ARTIFACT_DIR%\nsis"
if errorlevel 1 goto :failed
if not exist "%ARTIFACT_DIR%\msi\" mkdir "%ARTIFACT_DIR%\msi"
if errorlevel 1 goto :failed
copy /y "src-tauri\target\release\openclaw-manager.exe" "%ARTIFACT_DIR%\openclaw-manager.exe" >nul
if errorlevel 1 goto :failed
copy /y "src-tauri\target\release\bundle\nsis\*.exe" "%ARTIFACT_DIR%\nsis\" >nul
if errorlevel 1 goto :failed
copy /y "src-tauri\target\release\bundle\msi\*.msi" "%ARTIFACT_DIR%\msi\" >nul
if errorlevel 1 goto :failed

echo.
echo Build succeeded. Output files:
dir /b "%ARTIFACT_DIR%\nsis\*.exe" 2>nul
dir /b "%ARTIFACT_DIR%\msi\*.msi" 2>nul
echo.
echo Full output directory:
echo %ARTIFACT_DIR%
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
