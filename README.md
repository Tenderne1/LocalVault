# LocalVault

Tauri 2 + React + TypeScript + Rust + SQLite 的本地优先密码管理器。

## 核心功能

- Argon2id + XChaCha20-Poly1305 加密 Vault
- 主密码、Recovery Code 与本地密保恢复
- 多级分类、展开/收起、双击展开/收起与拖拽排序
- 密码收藏、回收站、历史记录
- 完整 `.vault` 备份与账号密码 `.lvx` 导入/导出
- 独立目录自动版本备份、变化检测与保留策略
- CSV 批量导入及输入限制；模板含演示数据，导入时自动忽略；支持 UTF-8、UTF-16、GBK/GB18030
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

清理发布产物：

```powershell
npm.cmd run clean:release
```

## 数据与安全

- Vault 数据默认存放在当前用户本地数据目录；便携版使用自身 `data\\` 目录。
- 完整备份和账号密码导出均为加密文件，不保存明文主密码。
- 自动版本备份必须位于 LocalVault 数据目录之外；只有 Vault 实际内容发生变化时才创建新的完整快照。
- CSV 是明文中转文件，导入完成后应及时删除或妥善保护。
- 软件更新只在用户主动检查时联网；更新包必须通过 Tauri Updater 签名验证。

## 更新记录

版本新增、修复和移除内容统一记录在 `CHANGELOG.md`。
