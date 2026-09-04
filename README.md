# 🔐 LocalVault

**本地优先（Local-first）密码管理器** —— 基于 Tauri 2 + React 19 + TypeScript + Rust + SQLite。

所有密码数据加密后仅保存在你自己的电脑上，不上传云端、无需账号，离线可用。

## ✨ 核心特性



* **强加密**：Argon2id（128 MiB）密钥派生 + XChaCha20-Poly1305 认证加密，密钥在内存中即时清零

* **找回体系**：主密码 + Recovery Code + 本地密保三合一恢复

* **多级分类**：父子层级、展开 / 收起、拖拽排序

* **风险扫描**：密码强度评分、弱密码 / 重复密码检测、7 天内过期提醒

* **密码生成器**：一键生成强密码，复制后 30 秒自动清空剪贴板

* **回收站**：删除自动保留 7–30 天，可恢复 / 彻底清除

* **完整备份**：`.vault` 加密备份 + 独立目录自动版本备份（变化检测 + 保留策略）

* **跨电脑迁移**：账号密码 `.lvx` 加密导入 / 导出

* **批量导入**：CSV 模板，支持 UTF-8 / UTF-16 / GBK/GB18030 编码

* **自动锁定**：空闲 1/5/10/30 分钟自动上锁；Windows 下 Vault 文件隐藏 + 只读保护

## 🔒 安全设计



```
主密码 ──Argon2id──▶ KEK ──XChaCha20-Poly1305 包裹──▶ DEK ──加密每条记录──▶ SQLite Vault
```



* 主密码永不落盘，仅用于内存派生解密密钥

* 每条记录独立随机 nonce + AAD 绑定，密文防篡改

* 仅在用户主动点击「软件更新」时联网，更新包须通过签名校验

* 不收集任何遥测，密码数据永不上传

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



* [项目介绍](项目介绍.md)

* [功能介绍](功能介绍.md)

* [使用手册](使用手册.md)

## 🛡 数据与安全



* Vault 存放于用户本地数据目录（便携版为程序目录 `data\`）

* 完整备份与自动备份必须位于 LocalVault 数据目录**之外**

* CSV 为明文中转文件，导入完成后请及时删除

* 版本变更见 [CHANGELOG.md](CHANGELOG.md)

## 📄 License

[MIT](LICENSE) © 2026 [Tenderne1](https://github.com/Tenderne1)
