$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$releaseExeCandidates = @(
  (Join-Path (Get-Location) "src-tauri\target\release\LocalVault.exe"),
  (Join-Path (Get-Location) "src-tauri\target\release\localvault.exe")
)
$exe = $releaseExeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $exe) {
  throw "找不到已编译的 LocalVault.exe，请先运行 npm.cmd run tauri:build。"
}

$portableRoot = Join-Path (Get-Location) "release\LocalVault-Portable-x64"
if (Test-Path $portableRoot) { Remove-Item -Recurse -Force $portableRoot }
New-Item -ItemType Directory -Force -Path $portableRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $portableRoot "data") | Out-Null

Copy-Item $exe (Join-Path $portableRoot "LocalVault.exe") -Force

@'
@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$wv=(Get-ItemProperty -Path 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00D3B7A3A8F8}' -ErrorAction SilentlyContinue).pv; if(-not $wv){$wv=(Get-ItemProperty -Path 'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00D3B7A3A8F8}' -ErrorAction SilentlyContinue).pv}; if(-not $wv){Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('LocalVault requires Microsoft Edge WebView2 Runtime. The official download page will now open.','LocalVault'); Start-Process 'https://developer.microsoft.com/en-us/microsoft-edge/webview2/'; exit 1}; Start-Process -FilePath '%~dp0LocalVault.exe'"
if %ERRORLEVEL% NEQ 0 exit /b %ERRORLEVEL%
endlocal
'@ | Set-Content -Path (Join-Path $portableRoot "Launch-LocalVault.cmd") -Encoding ASCII

# Tauri/WebView2 loader 等运行时 DLL（如果目标目录存在则一并带走）。
$releaseDir = Split-Path $exe -Parent
Get-ChildItem $releaseDir -Filter "*.dll" -File -ErrorAction SilentlyContinue | ForEach-Object {
  Copy-Item $_.FullName (Join-Path $portableRoot $_.Name) -Force
}

# 该标记让程序把加密 Vault 放到便携版目录的 data\ 下，而不是 AppData。
Set-Content -Path (Join-Path $portableRoot "portable.flag") -Value "LocalVault Portable" -Encoding UTF8

@"
LocalVault 便携版

使用方法：
1. 整个文件夹一起保存，不要只移动 LocalVault.exe。
2. 推荐双击 Launch-LocalVault.cmd 启动；它会先检查 WebView2 Runtime。也可以直接运行 LocalVault.exe。
3. Vault 数据保存在本目录 data\vault.db。
4. 请不要把 portable.flag 删除，否则程序会恢复使用 Windows 用户数据目录。
5. 建议把整个 LocalVault-Portable-x64 文件夹放在 U 盘/移动硬盘上时，再额外做好加密备份。

注意：便携版不内置 WebView2 Runtime。推荐使用 Launch-LocalVault.cmd；如果缺少 Microsoft Edge WebView2 Runtime，会弹出友好提示并打开微软官方下载页面。
"@ | Set-Content -Path (Join-Path $portableRoot "README-便携版.txt") -Encoding UTF8

# 浏览器填充扩展（保持 icons/ 目录结构，供「加载解压缩的扩展」直接选择）
$extSrc = Join-Path (Get-Location) "extension"
if (Test-Path $extSrc) {
  Copy-Item $extSrc (Join-Path $portableRoot "extension") -Recurse -Force
}
# 扩展安装向导（放在便携版根目录，提示选择本目录 extension 文件夹）
$installCmd = Join-Path (Get-Location) "scripts\安装浏览器扩展.cmd"
if (Test-Path $installCmd) {
  Copy-Item $installCmd (Join-Path $portableRoot "安装浏览器扩展.cmd") -Force
}

$zip = Join-Path (Get-Location) "release\LocalVault-Portable-x64-v1.9.4.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $portableRoot "*") -DestinationPath $zip -CompressionLevel Optimal

# 打包浏览器填充插件（解压后含 manifest.json 与 icons/，可直接加载）
$pluginZip = Join-Path (Get-Location) "release\LocalVault-Fill-v1.9.3.zip"
if (Test-Path $pluginZip) { Remove-Item -Force $pluginZip }
if (Test-Path $extSrc) {
  Compress-Archive -Path (Join-Path $extSrc "*") -DestinationPath $pluginZip -CompressionLevel Optimal
}

Write-Host "Portable folder: $portableRoot" -ForegroundColor Green
Write-Host "Portable ZIP:    $zip" -ForegroundColor Green
Write-Host "Plugin ZIP:      $pluginZip" -ForegroundColor Green
