# Changelog

## [1.6.3]

### Added

- 增加“软件更新”入口。
- 增加基于 Tauri Updater 的手动更新检查。
- 更新检查默认不自动执行，仅在用户主动点击时联网。
- 支持从 GitHub Releases 检查新版本。
- 支持下载并安装签名更新包。
- 增加更新下载进度显示。
- 增加 GitHub Actions 自动发布 updater artifacts、`latest.json`、Portable ZIP 和 SHA-256 校验文件。

### Security

- 更新包必须通过 Tauri updater 签名验证后才能安装。
- updater 私钥只允许存在于本地安全存储或 GitHub Actions Secrets 中。
- 更新过程不读取、不上传 Vault、账号、密码或 Recovery Code。
- 完整备份导出恢复为“选择保存位置 → 再次验证当前主密码 → 执行导出”的二次验证流程。

### Changed

- 软件版本统一为 `1.6.3`。
- 关于页面、窗口版本标识和 README 更新到 V1.6.3。
- GitHub updater endpoint 固定为 `Tendernel/LocalVault` 的最新 Release。
