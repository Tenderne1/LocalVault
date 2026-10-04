# 🔐 LocalVault

**本地优先（Local-first）密码管理器** —— 基于 Tauri 2 + React 19 + TypeScript + Rust + SQLite。

所有密码数据加密后仅保存在你自己的电脑上，不上传云端、无需账号，离线可用。

## ✨ 核心特性



* **强加密**：Argon2id（128 MiB）密钥派生 + XChaCha20-Poly1305 认证加密，密钥在内存中即时清零

* **找回体系**：主密码 + Recovery Code + 本地密保三合一恢复

* **多级分类**：父子层级、展开 / 收起、拖拽排序

* **风险扫描**：密码强度评分、弱密码 / 重复密码检测、7 天内过期提醒

* **密码生成器**：一键生成强密码，字符类型、长度、数量可配置

* **回收站**：删除自动保留 7–30 天，可恢复 / 彻底清除

* **完整备份**：`.vault` 加密备份 + 独立目录自动版本备份（变化检测 + 保留策略）

* **跨电脑迁移**：账号密码 `.lvx` 加密导入 / 导出

* **批量导入**：CSV 模板，支持 UTF-8 / UTF-16 / GBK/GB18030 编码

* **自动锁定**：空闲 1/5/10/30 分钟自动上锁；Windows 下 Vault 文件隐藏 + 只读保护

* **🌐 浏览器自动填充（v1.9.4 局域网同步版）**：本地回环填充服务（仅 127.0.0.1）+ Chromium MV3 扩展；密码框旁 🔑 一键填充，域名子域双向匹配，密码仅在下发时刻经本机回环传输；一次配对后 Vault 解锁自动连接、锁定自动断开，无需重复配对

* **📱 安卓手机端（v1.9.4 局域网同步版）**：`android-app/` 手机客户端，通过局域网与桌面端双向同步密码；扫码 / 配对码配对（6 位一次性），传输全程 XChaCha20 加密，仅限局域网不暴露公网；手机端主密码与桌面端同一套规则与算法（详见 [android-app/README.md](android-app/README.md)）

* **全局唤出快捷键**：默认 `Ctrl+Alt+L` 随时把窗口唤到前台（可录制更换 / 关闭）

* **开机自启 / 点 × 最小化托盘**：常规设置集中管理；点击 × 可选择最小化到托盘（可记住选择），托盘右键退出才真正结束程序

* **浅色 / 暗色主题**：右上角一键切换，首次运行默认跟随系统

* **多版本软件更新**：检测更新后按版本查看说明，支持软件内自动更新，也可复制直链用浏览器 / IDM 下载或手动下载

## 🔒 安全设计



```
主密码 ──Argon2id──▶ KEK ──XChaCha20-Poly1305 包裹──▶ DEK ──加密每条记录──▶ SQLite Vault
```



* 主密码永不落盘，仅用于内存派生解密密钥

* 每条记录独立随机 nonce + AAD 绑定，密文防篡改

* 仅在用户主动点击「软件更新 → 检测更新」时联网，更新包须通过签名校验

* 解锁后前端列表不持任何密码明文：Rust 内存会话密钥单条加密（仅下发 passwordEncrypted 密文），查看 / 复制单条时才经后端解密、用完即清零，锁定即销毁会话密钥

* 不收集任何遥测，密码数据永不上传

* 浏览器填充服务仅监听 127.0.0.1 且需 Bearer 令牌鉴权；Vault 锁定即强制停止服务并销毁令牌；扩展 content script 不持有令牌

* 配对码为 6 位一次性（5 分钟有效）；match 接口永不返回密码

## 🚀 快速开始



* **安装版**：前往 [Releases](https://github.com/Tenderne1/LocalVault/releases) 下载 NSIS / MSI 安装包

* **便携版**：下载 `LocalVault-Portable-x64.zip`，解压即用（需 WebView2 Runtime）

## 🛠 开发与构建



```
npm install

npm.cmd run tauri:dev        # 开发模式

npm.cmd run tauri:build      # 构建安装包

npm.cmd run build:portable   # 构建绿色便携版
```

## 📚 文档




* [使用手册](使用手册.md)

* [浏览器填充使用说明](extension/README.md)

* [自动填充架构说明](AUTOFILL-PLAN.md)

## 🛡 数据与安全



* Vault 存放于用户本地数据目录（便携版为程序目录 `data\`）

* 完整备份与自动备份必须位于 LocalVault 数据目录**之外**

* CSV 为明文中转文件，导入完成后请及时删除

* 版本变更见 [CHANGELOG.md](CHANGELOG.md)

## 📄 License

[MIT](LICENSE) © 2026 [Tenderne1](https://github.com/Tenderne1)


## v1.8.0

- 默认关闭的本地故障日志，不记录密码或密钥材料。
- Windows 安装版启用 WebView2 bootstrapper；便携版提供 WebView2 检测启动器。
- 改进 Rust 临时密钥材料清零，并明确本机高权限内存 Dump 的安全边界。
