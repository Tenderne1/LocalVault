@echo off
chcp 65001 >nul
title LocalVault-Fill 浏览器扩展安装向导
echo ============================================
echo   LocalVault-Fill 浏览器扩展安装向导
echo ============================================
echo.
echo 本目录自带扩展文件夹：extension
echo.
echo 正在打开浏览器扩展管理页...
start chrome://extensions
echo.
echo 请按以下步骤操作：
echo   1. 打开页面右上角的【开发者模式】开关
echo   2. 点击【加载已解压的扩展程序】
echo   3. 选择本目录下的 extension 文件夹
echo.
echo 若上面的页面没有打开，请手动在浏览器地址栏输入：
echo   chrome://extensions （Chrome）
echo   或 edge://extensions （Edge）
echo.
pause
