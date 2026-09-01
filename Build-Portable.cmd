@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-portable.ps1"
if errorlevel 1 (
  echo.
  echo Portable build failed. Press any key to exit.
  pause >nul
  exit /b 1
)
echo.
echo Portable build completed successfully.
pause
endlocal
