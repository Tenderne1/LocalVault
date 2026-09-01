# LocalVault V1.6.3 更新系统配置说明

## 设计目标

LocalVault V1.6.3 采用“本地优先 + 用户主动检查更新”的模式：

- 默认不自动联网检查更新。
- 用户点击“软件更新”后才访问 GitHub Releases。
- 更新请求只获取版本信息、更新说明和签名更新包，不上传 Vault、账号、密码或 Recovery Code。
- 更新包必须通过 Tauri Updater 签名验证后才允许安装。
- 官方自动更新源仅使用 GitHub Releases；蓝奏云不参与程序内置自动更新。

Tauri 官方文档要求 updater 使用签名验证，且私钥不能公开；公钥写入 `src-tauri/tauri.conf.json`，私钥只在构建环境中使用。详见 Tauri Updater 官方文档。

## 第一次配置更新签名密钥

在项目根目录执行：

```powershell
npm.cmd run tauri signer generate -- -w "$env:USERPROFILE\.tauri\localvault.key"
```

如果 CLI 要求设置密码，建议设置一个专门用于更新签名私钥的强密码，并安全保存。

生成后会得到私钥文件和对应公钥文件。**私钥绝对不能提交到 GitHub。**

把公钥文件内容复制到：

```text
src-tauri/tauri.conf.json
```

替换：

```text
REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY
```

注意：`pubkey` 必须填写公钥内容，而不是公钥文件路径。

## 本地构建签名更新包

PowerShell：

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY="$env:USERPROFILE\.tauri\localvault.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD="你的私钥密码"
npm.cmd run tauri:build
```

构建成功后，Windows 更新相关签名文件会出现在：

```text
src-tauri/target/release/bundle/nsis/
src-tauri/target/release/bundle/msi/
```

例如：

```text
LocalVault_1.6.3_x64-setup.exe
LocalVault_1.6.3_x64-setup.exe.sig
LocalVault_1.6.3_x64_en-US.msi
LocalVault_1.6.3_x64_en-US.msi.sig
```

## GitHub Actions Secrets

进入 GitHub：

```text
Settings
→ Secrets and variables
→ Actions
→ New repository secret
```

建立：

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
```

其中：

- `TAURI_SIGNING_PRIVATE_KEY`：私钥文件内容，或 Tauri 支持的私钥值。
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`：生成私钥时设置的密码；如果没有密码则留空。

**不要把这两个值写进仓库、README、`.env`、ZIP 或 Release。**

## GitHub Release 自动生成

`.github/workflows/release.yml` 已配置为：

1. 推送 `v*` 标签。
2. GitHub Actions 在 Windows runner 上构建 Tauri。
3. 使用 GitHub Secrets 对 updater artifacts 签名。
4. 创建 GitHub Release。
5. 上传 EXE、MSI 及对应 `.sig` 文件。
6. 生成 Tauri updater 使用的 `latest.json`。
7. 生成并上传 Portable ZIP。
8. 生成并上传 `SHA256SUMS.txt`。

## 发布 V1.6.3

确认以下文件中的版本均为 `1.6.3`：

```text
package.json
src-tauri/Cargo.toml
src-tauri/tauri.conf.json
```

然后：

```powershell
git add .
git commit -m "feat: add signed GitHub updater for v1.6.3"
git push origin main
```

创建并推送标签：

```powershell
git tag v1.6.3
git push origin v1.6.3
```

之后 GitHub Actions 会自动构建并创建 `LocalVault v1.6.3` Release。

## 更新地址

`src-tauri/tauri.conf.json` 使用：

```text
https://github.com/Tendernel/LocalVault/releases/latest/download/latest.json
```

因此后续发布 `v1.6.4`、`v1.6.5` 等新版本时，已经安装的旧版本无需修改更新地址。

## 安全注意事项

1. 不要把 updater 私钥提交到 GitHub。
2. 不要把 `.vault`、`.lvx`、Recovery Code 或真实密码提交到仓库。
3. 不要用未经签名的 EXE 替代 updater artifact。
4. 如果 updater 私钥泄露，应立即停止继续发布并重新规划密钥迁移；已经安装旧公钥的客户端不会自动信任新的公钥。
5. 第一次正式启用 updater 后，应保留私钥的离线备份。
6. 更新功能只负责软件版本更新，不参与 Vault 数据同步。
