# LocalVault Windows 打包说明

## 一键打包

在 Windows PowerShell 中进入项目根目录，执行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\build-windows.ps1
```

脚本会依次：

1. 检查 Node.js / npm / Rust / Cargo
2. 安装 npm 依赖
3. 编译前端
4. 使用 Tauri 生成 NSIS `.exe` 安装包和 MSI
5. 从 release executable 生成 `LocalVault-Portable-x64` 绿色便携版
6. 同时生成 `release\LocalVault-Portable-x64.zip`

## 输出位置

```text
src-tauri\target\release\bundle\nsis\   # EXE 安装程序
src-tauri\target\release\bundle\msi\    # MSI 安装程序
release\LocalVault-Portable-x64\          # 绿色便携版目录
release\LocalVault-Portable-x64.zip        # 绿色便携版压缩包
```

## 绿色便携版的数据位置

便携版目录包含 `portable.flag`，LocalVault 检测到该文件后，会把 Vault 放在：

```text
LocalVault-Portable-x64\data\vault.db
```

这样整个目录可以复制到 U 盘或其他 Windows 电脑。

### 重要

- 不要只复制 `LocalVault.exe`，请复制整个便携版目录。
- 便携版只是“不需要安装”，不是“无需 Windows WebView2 Runtime”。Windows 如果缺少 WebView2 Runtime，需要先安装 Microsoft Edge WebView2 Runtime。
- 建议在首次正式发布前测试：创建 Vault、解锁、锁定、完整备份、找回主密码、密保及密码修改、导入导出，以及把整个便携目录复制到另一台 Windows 电脑后的数据可用性。
