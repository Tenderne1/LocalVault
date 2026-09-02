$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "release"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue "src-tauri\target\release\bundle"
Write-Host "Release artifacts cleaned."
