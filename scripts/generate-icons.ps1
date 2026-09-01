$ErrorActionPreference="Stop"
npm.cmd run tauri -- icon src-tauri/icons/brand.svg
if($LASTEXITCODE -ne 0){throw "Tauri icon generation failed"}
