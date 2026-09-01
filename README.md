# LocalVault V1.6.3

Tauri 2 + React + TypeScript + Rust + SQLite 的本地优先密码管理器。

## 当前修复
- 右键菜单跟随鼠标
- 新建分类真正可用
- 删除只删除当前条目，并增加二次确认
- 创建 Vault 后进入恢复设置
- Recovery Code + 三组本地问题
- 完整备份 / 账号密码导入导出分离
- 登录页只保留“找回主密码”，不再提供备份恢复入口
- 找回主密码改为“验证恢复凭证 → 设置新主密码 → 必须使用新主密码重新验证”
- “密保及密码修改”先验证当前主密码，再可选择只修改密保、只修改主密码或同时修改
- 修改密保自动生成新的 Recovery Code，并尝试自动复制到剪贴板
- Vault 备份支持“合并”或“替换”两种恢复方式，合并不会清掉当前新增条目
- Recovery 密保答案取消 12 字符最低限制，仅要求非空，支持中文答案
- 保存状态
- Windows 品牌图标
- Tauri updater 配置
- SQLite 使用 DELETE journal，减少直接拷贝数据库时的 WAL/SHM 一致性问题

## 启动
```powershell
npm install
npm.cmd run tauri:dev
```

## Windows 构建

推荐直接使用一键脚本：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\scripts\build-windows.ps1
```

脚本会生成 NSIS `.exe` 安装程序、MSI，以及 `release\LocalVault-Portable-x64.zip` 绿色便携版。

也可以直接执行：

```powershell
npm.cmd run tauri:build
```

然后运行 `scripts\build-portable.ps1` 生成便携版。

详细说明见 `BUILD_WINDOWS.md`。

### 重要
这是可运行开发版，不代表已经通过独立安全审计。生产密码请先在测试 Vault 中完成完整回归。

## 数据迁移与备份说明

LocalVault 的“完整备份”和“仅导出账号密码”用途不同：

- **完整备份（`.vault`）**：完整 Vault 快照。适合灾备、重装系统或整机迁移。恢复前会先使用你输入的 Vault 主密码验证备份；验证失败不会覆盖当前 Vault。
- **仅导出账号密码（`.lvx`）**：只导出密码条目，适合 A 电脑导出、B 电脑导入。导出时需要再次验证 A 电脑的 Vault 主密码；导入 B 电脑时必须再次输入 A 电脑的主密码。
- `.lvx` 不保存明文主密码。文件包含随机 salt、由源 Vault 主密码派生的 KEK 加密包裹的 DEK，以及由 DEK 加密并认证的密码数据。
- **不要把 `.lvx` / `.vault` 当成普通文本文件或 CSV 使用，也不要通过聊天软件、邮件明文转发主密码。**


### 主密码规则（V1.6.3）

新设置的主密码必须至少 8 位，并同时包含数字、大写字母、小写字母和特殊符号；修改主密码时不得与旧主密码相同。已有 Vault 的主密码只要能够正确验证即可继续使用，不会因为旧密码历史规则而被强制重新设置。


## 软件更新（V1.6.3）

LocalVault V1.6.3 增加了基于 Tauri Updater 的手动更新机制：

- 默认不会自动检查更新。
- 只有用户主动点击“软件更新”时才访问 GitHub Releases。
- 更新检查不会上传 Vault、账号、密码或 Recovery Code。
- 更新包必须通过 Tauri updater 签名验证后才会安装。
- Windows 更新采用 Tauri 官方 updater 流程，下载并安装成功后程序会退出，由安装程序完成更新。
- 官方更新源为 GitHub Releases：`Tendernel/LocalVault`。
- 蓝奏云不作为程序内置自动更新源。

自动更新签名私钥不得提交到 GitHub；发布构建通过 GitHub Actions Secret `TAURI_SIGNING_PRIVATE_KEY` 和 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 提供。
