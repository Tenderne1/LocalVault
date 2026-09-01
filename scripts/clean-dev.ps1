$ErrorActionPreference="Stop"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue node_modules,dist,src-tauri/target
Write-Host "Clean complete."
