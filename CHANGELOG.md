# Changelog

## 1.7.9 — 2026-09-04

- 修复防暴力破解验证码 SVG 原始字符串导致的 Rust 编译错误。
- 修复 Tauri 命令无法访问验证码安全状态方法的可见性错误。
- 清理验证码生成中的无效括号警告。
- 统一应用、Cargo、Tauri、npm 与文档版本号为 1.7.9。
- 发布包不再包含 `node_modules` 等本地依赖缓存。


## 1.7.8 — 2026-09-04

### Fixed
- Fixed auto-lock setting persistence; selected 1/5/10/30 minute timeout now survives restart.
- Improved activity detection for mouse, pointer, keyboard, wheel, and touch input.
- Revalidated auto-lock on window focus/visibility changes so switching apps and waking from sleep cannot bypass the timeout.
- Reset the inactivity timer after successful unlock to prevent immediate re-locking.
- Only clear the frontend vault state after a successful backend lock.
- Made trash restore atomic to avoid partial restore/delete states.
- Fixed “backup now” so it does not silently enable automatic backups.
- Corrected the delete dialog wording to match the actual move-to-trash behavior.
- Fixed password-expiry option display ordering.


## 1.7.7 — 2026-09-02

### 修复
- 修复导入 1.6.3 完整 `.vault` 备份时，旧版 `categories` 表没有 `parent_name` 导致验证失败的问题；旧版分类按一级分类兼容读取。
- 完整备份预览与合并兼容旧版分类表结构，读取原始备份时不会修改用户选择的备份文件。
- 完整 Vault 恢复增加 Windows 文件替换失败保护：验证完成后才替换当前 Vault，替换失败时尝试恢复原 Vault。

### 优化
- 自动版本备份的变化检测同时纳入密码条目和分类数据，分类新增、修改、删除、排序变化也能正确触发新快照。
- 清理未使用的 RecoveryDialog、CSV 常量、对话框状态、样式规则及重复 Rust 辅助函数。
- 移除未使用的 `thiserror` 依赖和回收站移动接口中未实际使用的保留期参数。
- README 收敛为当前项目说明，历史修改不再堆积在文档中。
