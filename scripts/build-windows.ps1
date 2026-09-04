$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

Write-Host "=== LocalVault Windows Build ===" -ForegroundColor Cyan

node --version
if ($LASTEXITCODE -ne 0) { throw "未检测到 Node.js，请先安装 Node.js LTS。" }
npm --version
if ($LASTEXITCODE -ne 0) { throw "未检测到 npm。" }
rustc --version
if ($LASTEXITCODE -ne 0) { throw "未检测到 Rust，请先安装 Rust。" }
cargo --version
if ($LASTEXITCODE -ne 0) { throw "未检测到 Cargo。" }

Write-Host "[1/4] 安装/更新前端依赖..." -ForegroundColor Yellow
npm.cmd install
if ($LASTEXITCODE -ne 0) { throw "npm install 失败。" }

Write-Host "[2/3] 构建 Windows 安装包..." -ForegroundColor Yellow
npm.cmd run tauri:build
if ($LASTEXITCODE -ne 0) { throw "Tauri Windows 打包失败。" }

Write-Host "[3/3] 生成绿色便携版..." -ForegroundColor Yellow
& (Join-Path $PSScriptRoot "build-portable.ps1")
if ($LASTEXITCODE -ne 0) { throw "绿色便携版生成失败。" }

$bundle = Join-Path (Get-Location) "src-tauri\target\release\bundle"
Write-Host ""
Write-Host "=== 构建完成 ===" -ForegroundColor Green
Write-Host "安装包目录: $bundle"
Write-Host "便携版目录: $(Join-Path (Get-Location) 'release\LocalVault-Portable-x64')"
