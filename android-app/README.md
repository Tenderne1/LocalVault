# LocalVault Mobile（安卓端）v1.9.4 局域网同步版

LocalVault 桌面版（v1.9.4 起）的安卓手机客户端，通过**局域网**与桌面端双向同步密码数据。

## 功能（当前阶段）

- 🔐 主密码保护：Argon2id(128MB, 3 iter, 2 lanes) + XChaCha20-Poly1305，与桌面端同一套算法
- 📷 扫码配对：扫描桌面端「手机同步」弹窗的二维码（`localvault://sync?ip=..&port=..&code=..&key=..`），6 位一次性配对码换 token
- 🔄 增量同步：拉取桌面端密码条目与分类，`updated_at` 新者胜合并，本地缓存加密存储
- 🗂️ 密码库：查看 / 复制 / 删除条目（密码仅单条下发）
- ⚙️ 修改主密码 / 立即锁定

## 安全设计

- 本地缓存 `vault.mobile` = base64( salt16 || nonce24 || XChaCha20 密文)，密钥由主密码 Argon2id 派生，**不落盘**
- 配对 token、传输密钥随缓存一起加密存储；锁定即清空内存会话密钥
- 同步协议：所有请求体/响应体 XChaCha20 加密（AAD = 接口路径），密钥来自二维码，服务仅监听局域网 38528
- 密码明文：列表接口只返回掩码；查看密码按 id 单条下发

## 构建（Windows 宿主）

前置：JDK 17、Android SDK（platform-tools / android-34 / build-tools 34 / NDK 26.3）、rustup target aarch64-linux-android 等、cargo-ndk。

```powershell
$env:JAVA_HOME="C:\AndroidDev\jdk17"
$env:ANDROID_HOME="C:\AndroidDev\android-sdk"
$env:Path="$env:JAVA_HOME\bin;$env:ANDROID_HOME\platform-tools;" + $env:Path

cd android-app
npm install
npm run tauri -- android init   # 首次
npm run tauri -- android build --apk
```

产物：`android-app/src-tauri/gen/android/app/build/outputs/apk/universal/release/*.apk`

## 开发调试（桌面浏览器预览 UI）

```powershell
cd android-app
npm run tauri -- dev   # 需要先 npm run build 产物或 devUrl 模式
```

## 结构

```
android-app/
├── src/                  # React UI（设置/解锁/同步/密码库/设置）
├── src-tauri/
│   ├── src/
│   │   ├── crypto.rs     # Argon2id + XChaCha20（与桌面端一致）
│   │   ├── store.rs      # 本地加密缓存（主密码/解锁/持久化）
│   │   ├── sync_client.rs# 局域网同步客户端（pair/pull/unpair）
│   │   └── lib.rs        # Tauri 命令
│   └── gen/android/      # tauri android init 生成的原生工程
```

## 与桌面端约定

- 桌面端同步服务：`0.0.0.0:38528`（仅解锁时运行，锁定即停服销毁 token）
- 配对二维码：`localvault://sync?ip=<局域网IP>&port=38528&code=<6位码>&key=<base64传输密钥>`
- 接口：`GET /api/sync/health`、`POST /api/sync/pair`、`POST /api/sync/pull`、`POST /api/sync/unpair`
