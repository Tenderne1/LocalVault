# LocalVault

Tauri 2 + React + TypeScript + Rust + SQLite 的本地优先密码管理器。

## 当前版本：1.7.7

### 本次更新
- 修复 1.6.3 `.vault` 完整备份导入时旧版 `categories` 表缺少 `parent_name` 导致验证失败的问题。
- 完整备份预览、合并兼容旧版分类表结构，读取原始备份时不修改备份文件。
- 完整 Vault 恢复增加 Windows 文件替换失败保护，验证成功后才替换当前 Vault，失败时保留原 Vault。
- 自动版本备份的变化检测同时覆盖密码条目和分类数据，分类排序等变化也能形成新快照。
- 清理未使用的前端对话框组件、常量、样式及 Rust 辅助代码，移除未使用依赖。

## 核心功能

- Argon2id + XChaCha20-Poly1305 加密 Vault
- 主密码、Recovery Code 与本地密保恢复
- 多级分类、展开/收起、双击展开/收起与拖拽排序
- 密码收藏、回收站、最近 3 次修改记录
- 完整 `.vault` 备份与账号密码 `.lvx` 导入/导出
- 独立目录自动版本备份、变化检测与保留策略
- CSV 批量导入；支持 UTF-8、UTF-16、GBK/GB18030
- 手动软件更新与 Tauri Updater 签名校验
- Windows 本地 Vault 文件隐藏/只读保护

## 开发

```powershell
npm install
npm.cmd run tauri:dev
```

## Windows 构建

```powershell
npm.cmd run tauri:build
```

生成便携版：

```powershell
npm.cmd run build:portable
```

## 数据与安全

- Vault 默认存放在当前用户本地数据目录；便携版使用程序目录下的 `data\`。
- 完整备份和账号密码导出均为加密文件，不保存明文主密码。
- 自动版本备份必须位于 LocalVault 数据目录之外。
- CSV 是明文中转文件，导入完成后应及时删除或妥善保护。
- 软件更新只在用户主动检查时联网；更新包必须通过 Tauri Updater 签名验证。

版本变更仅记录在 `CHANGELOG.md`。
