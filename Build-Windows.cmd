@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\build-windows.ps1"
if errorlevel 1 (
  echo.
  echo Build failed. Press any key to exit.
  pause >nul
  exit /b 1
)
echo.
echo Build completed successfully.
pause
endlocal
