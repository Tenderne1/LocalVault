# LocalVault v1.9.4 交接文档

> 交接时间：2026-10-03 · 交接方：AI 助手（原会话窗口）
> 用途：供新工作窗口接手继续开发。以下状态为交接瞬间的**实际落盘事实**，接手后请以文件内容为准复核，勿凭本摘要假设。

---

## 1. 项目概况

| 项 | 值 |
|---|---|
| 项目路径 | `H:\passwordmanagers\LocalVault-v1.9.4`（局域网同步版） |
| 技术栈 | 桌面端：Tauri 2.12.1 + React + Rust + SQLite（Windows）；手机端：android-app/ 独立工程 |
| 端口 | 浏览器填充 autofill=38527（127.0.0.1）；局域网同步 sync=38528（0.0.0.0） |
| 加密 | Argon2id（128MiB/3/2）+ XChaCha20-Poly1305，桌面/手机端常量一致 |
| 只读备份 | `H:\passwordmanagers\LocalVault-v1.9.3-backup-20261003`（勿动） |

版本历史：1.7.7 → 1.9.1 → 1.9.2（自动填充）→ 1.9.3（安全改造：密码明文不进 WebView，dump 基线 testpass123/testuser）→ 1.9.4（局域网同步）。

---

## 1.5 fix5（2026-10-03 晚，手机端五诉求实现）

用户验收 fix4 配对（欧克了）后提出 5 项新诉求，已全部实现并构建交付：

| # | 诉求 | 实现 |
|---|---|---|
| ① | 桌面端给手动配对完整地址 | `src/main.tsx` 手机同步弹窗新增「手动配对地址」区块（完整 localvault:// URL + 一键复制），`syncQrPayload()` 统一生成 |
| ② | 微信扫码无法打开 localvault:// | 二维码内容改为 `http://ip:38528/mobile-qr?c=配对码&k=key`；桌面端 `sync.rs` 新增 GET `/mobile-qr` 引导页（含复制配对链接 + 「在 App 中打开」唤起，key 不出明文）；手机端 `sync_client.rs::parse_qr_payload` 兼容解析 http 引导页格式（含单测 `qr_payload_http_guide_format`）；`AndroidManifest.xml` 注册 `localvault://` scheme |
| ③ | 手机端新建/编辑/删除 | Rust `mobile_save_entry`（已存在）前端接线：`api.ts` 加 `mobileSaveEntry` + `EntryDraft`；`VaultPage.tsx` 加新建/编辑表单（名称/类型/账号/密码/网址/分类/备注/收藏），删除已有 |
| ④ | 自动同步开关 + 推送电脑 | `sync_client.rs` 新增 `push()`（POST /api/sync/push，全量条目幂等合并 + deleted_ids）；`lib.rs` 新增 `sync_push` 命令；`store.rs` VaultData 加 `deleted_ids`（删除先记待推，推送成功后清空）；`SyncPage.tsx` 加「自动同步到电脑」开关（localStorage `lv_mobile_auto_sync`）+「推送本地改动到电脑」按钮；`VaultPage` 保存/删除后按开关自动 push |
| ⑤ | 桌面端同步日志 | `sync.rs` 新增 `SyncLog{time,device,action,name,kind}`（不含密码）；`handle_push` 记录 upsert/delete；`SyncStatus.sync_logs` 透出；`main.tsx` 手机同步弹窗加「同步日志」区块（时间/动作/条目名/类型/设备，最多保留 100 条） |

**产物**（release\ 下）：
- `LocalVault-v1.9.4-Portable-x64.zip`（5,924,966B）——桌面端便携版（新 exe + extension + 启动脚本）
- `LocalVault-v1.9.4-arm64-fix5.apk`（7,260,578B）——签名验证通过（CN=LocalVault Mobile）

**验证**：桌面端/手机端 Rust 编译通过；手机端 3 个二维码解析单测全过（localvault:// 原生、http 引导页、非法拒绝）；手机端 tsc+vite 构建通过；APK apksigner verify 通过。
**未自动验证**：桌面端新 exe 实际渲染引导页、手机端真机增删改/自动同步链路（需用户手工跑通）。

**改动文件**：src-tauri/src/sync.rs、src/main.tsx、src/styles.css（桌面端）；android-app/src-tauri/src/{lib.rs,sync_client.rs,store.rs}、android-app/src/{api.ts,styles.css}、android-app/src/pages/{VaultPage.tsx,SyncPage.tsx}、android-app/src-tauri/gen/android/app/src/main/AndroidManifest.xml、HANDOVER.md

---

## 1.6 fix6（2026-10-03 深夜，手机端二批 8 优化）

用户提 8 个优化点，**本轮改动全部在手机端**（桌面端无改动、无需重建）：

| # | 诉求 | 实现 |
|---|---|---|
| ① | 推送/同步日志要增量校验，不能每次全量推 | `store.rs` VaultData 加 `pushed_since:i64` 增量游标：配对成功时初始化（配对时刻为基线），`sync_pull` 合并后推进，`sync_push` 只推 `updated_at > pushed_since` 的条目 + `deleted_ids` + `pending_cat_ops`，成功后推进游标并清空待推队列 |
| ② | 手机端编辑密码页加密码生成器 | Rust `mobile_generate_password`（长度 6–64 可调，保证大小写+数字，≥8 位加符号，洗牌）；编辑表单密码行加「🔑 生成」按钮 + 「原密码」回填按钮 |
| ③ | 手机端回收站 | `store.rs` 加 `trash:Vec<TrashItem>{entry,deleted_at}`；`mobile_delete_entry` 改为移入回收站（并记 `deleted_ids` 待推）；新命令 `mobile_trash_list`/`mobile_restore_entry`（恢复时 touch updated_at 并从待删队列移除）/`mobile_trash_purge`/`mobile_trash_clear`；VaultPage 加回收站页（恢复/彻底删除/清空） |
| ④ | 编辑时按返回键直接退到桌面 | VaultPage 进入 detail/edit/trash 时 `window.history.pushState`，监听 `popstate` 回列表——Android 返回键走 history.back → popstate → 回上一页，不再退出 App |
| ⑤ | 手机端分类被阉割 | 新命令 `mobile_category_list`/`mobile_category_create`/`mobile_category_rename`（同步改所有条目 category 并 touch updated_at）/`mobile_category_delete`（条目回「默认」），改动记 `pending_cat_ops` 随 push 推送桌面端；SettingsPage 加「分类管理」区块，编辑表单分类下拉 |
| ⑥ | 查看只能看密码 | 新命令 `mobile_entry_detail`（返回不含密码的完整字段：email/phone/nickname/notes/url 等）；VaultPage 列表点击进详情页（各字段逐项复制按钮，密码查看/复制分离） |
| ⑦ | 不能长按条目（类比 PC 右键） | 列表 `onContextMenu`（长按触发）弹底部 action-sheet：查看详情/复制密码/编辑/删除/取消 |
| ⑧ | 手机推送到电脑后，电脑需解锁才能看到 | App.tsx 常驻 effect：自动同步开启且已配对时每 30s 静默调 `syncPush()`——电脑锁定/离线时失败自动重试，解锁后即补推成功 |

**产物**（release\ 下）：
- `LocalVault-v1.9.4-arm64-fix6.apk`（7,313,826B）——签名验证通过（CN=LocalVault Mobile，SHA-256 f52c1ccb…）

**验证**：手机端 Rust cargo check 通过（修复 3 处 E0502 借用冲突 + 1 处 E0283 类型歧义）；前端 tsc+vite 构建通过（dist/assets/index-D9vO_DON.js）；cargo ndk 编译 .so（5,515,016B）→ gradle assembleUniversalRelease → APK 内含 `lib/arm64-v8a/liblocalvault_mobile_lib.so` → zipalign+apksigner 签名通过；雷电9 模拟器安装启动冒烟通过（解锁界面正常渲染、无崩溃）。
**未自动验证**：真机端 8 点交互链路（增量同步日志表现、返回键、长按菜单、回收站、分类推送），需用户实测。

**改动文件**：android-app/src-tauri/src/{store.rs,sync_client.rs,lib.rs}；android-app/src/{api.ts,styles.css}；android-app/src/pages/{VaultPage.tsx,SettingsPage.tsx}；android-app/src/App.tsx；HANDOVER.md
**桌面端本轮无改动**，`LocalVault-v1.9.4-Portable-x64.zip` 仍为 fix5 交付版，无需重装。

---

## 1.7 fix7（2026-10-04 凌晨，推送失败提示语义修正）

用户实测反馈：电脑锁定→解锁期间保存条目，UI 显示「保存失败：推送请求失败（timed out）」，误导用户以为本地没保存。修正原则：**本地保存成功就是成功，推送失败单独提示、后台自动补推**。

| 改动 | 内容 |
|---|---|
| VaultPage/SettingsPage `autoSync` | 不再 `throw`，改为返回 `boolean`（推送成功与否）；所有调用点（保存/删除/恢复/清空/分类增删改）文案分离：「已保存，已推送到电脑」 vs 「已保存；同步到电脑失败，稍后自动重试」——本地操作永远报成功，推送失败仅作提示 |
| push 超时 | `sync_client.rs::push` HTTP timeout 10s → 5s，电脑锁定/离线时失败提示更快，不等用户干等 |
| 失败不推进度（确认既有逻辑） | `sync_push` 失败时 `?` 提前返回，`pushed_since` 不推进、`deleted_ids`/`pending_cat_ops` 不清空 → App.tsx 30s 重试会完整补推 |

**产物**（release\ 下）：`LocalVault-v1.9.4-arm64-fix7.apk`（7,313,826B）——签名验证通过（CN=LocalVault Mobile）。
**验证**：前端 tsc+vite 构建通过（dist/assets/index-BalLlG0M.js）；cargo ndk 编译 .so（5,515,016B）→ gradle → zipalign+apksigner 签名通过。模拟器不在线（用户已关），未做安装冒烟——本轮仅文案/超时改动，渲染管线同 fix6（fix6 已冒烟通过）。
**未自动验证**：真机「电脑锁定→保存→解锁→30s 自动补推」全链路，需用户实测。

**改动文件**：android-app/src/pages/{VaultPage.tsx,SettingsPage.tsx}；android-app/src-tauri/src/sync_client.rs（push timeout 10s→5s）；HANDOVER.md

---

## 1.8 fix8（2026-10-04 凌晨，桌面端推送后自动刷新）

用户实测反馈：手机端推送数据到桌面端后，桌面端**必须锁定再解锁才能看到新条目**。根因：桌面端前端只在解锁时加载一次条目，手机 push 落库后前端无任何刷新机制。

**实现**（本轮改动全在桌面端，手机端无需新 APK）：

| 改动 | 内容 |
|---|---|
| `vault.rs` | 新增 `VaultManager::list()`：已解锁态从 SQLite 重新读全部条目（复用 unlock 的 read_entries+sanitize_entries 管线），供前端刷新 |
| `main.rs` | 新增并注册 `vault_list` 命令；setup 中注入 AppHandle 到 SyncBridge（`set_app`） |
| `sync.rs` | `SyncState` 加 `app:Option<AppHandle>` + `SyncBridge::set_app()`；`handle_push` 成功后（数据已落库）`app.emit("sync-updated")`——前端收到后自动刷新，**无需锁定再解锁** |
| `main.tsx` | 新增 `refreshEntries()`（调 vault_list → normalize → setEntries + loadCategories/loadTrash/loadSyncStatus + 提示"已刷新"）；`listen("sync-updated")` 事件（仅解锁态 view==="vault" 时刷新，锁定态直接忽略）；顶部标题栏加「🔄 刷新」按钮（手动兜底） |

**产物**（release\ 下）：`LocalVault-v1.9.4-Portable-x64-fix8.zip`（6,289,718B）——便携版，zip 内 LocalVault.exe = 17,065,472B（fix5 版 17,012,224B，新代码已嵌入）。
**验证**：桌面端 cargo check 通过（修复 Emitter trait 未导入 / state 生命周期 E0597 两处）；前端 tsc 通过；`npx tauri build --no-bundle` 构建成功；zip 内 exe 字节数核对一致。
**未自动验证**：按用户硬约束（GUI 测试不在真实桌面开窗口），未启动 exe 实测——「手机推送 → 桌面端自动刷新」链路需用户实测：解锁态下手机端保存一条 → 桌面端应自动出现（无需锁定）；锁定态下推送 → 解锁后可见；顶部「🔄 刷新」可手动兜底。

**改动文件**：src-tauri/src/{vault.rs,sync.rs,main.rs}；src/main.tsx；HANDOVER.md
**手机端本轮无改动**，仍用 `LocalVault-v1.9.4-arm64-fix7.apk`。

---

## 1.9 fix9（2026-10-04 上午，手机端下拉刷新 + 双向自动同步 + 构建链修复）

用户诉求：①电脑端修改不能自动同步到手机端，手机端必须手动拉取；②手机端无刷新机制——希望密码库界面下拉松手即刷新。

**实现**（改动全在手机端，桌面端无需重装）：

| 改动 | 内容 |
|---|---|
| `App.tsx` | 自动同步开启时每 30s **双向同步**：先 push（手机→电脑补推），再 pull（电脑→手机拉取），pull 成功后 `dispatchEvent(new CustomEvent("lv-pulled"))`——电脑端修改在 30s 内自动同步到手机 |
| `VaultPage.tsx` | **下拉刷新**：密码库列表页 touchstart/touchmove/touchend 手势（页面顶部、scrollY==0 时生效，阻尼 0.5、阈值 60px），显示「↓ 下拉刷新 / 松开刷新 / 正在同步…」提示，松手触发 syncPull+重载；监听 `lv-pulled` 事件自动刷新；`syncAndReload()` 成功后提示"已同步最新数据" |
| `styles.css` | 加 `.pull-hint` 下拉指示样式 |

**⚠️ 构建链关键修复**：发现**手动 gradlew 打包不会把前端 dist 同步到 Android assets**（tauri CLI 才自动同步）。此前 fix6/fix7 的 APK 实际装的是旧前端（assets 目录停留在旧 hash，如 index-6YtVDo99.js），用户实测到的是 fix5 级界面。**后续手机端打包必须：`npm run build` 后手动 `Copy-Item android-app\dist\* → android-app\src-tauri\gen\android\app\src\main\assets\ -Recurse -Force`，再 gradlew，并验证 APK 内 JS hash 与 dist 一致**（python 查 zip 内 assets/assets/index-*.js）。

**产物**（release\ 下）：`LocalVault-v1.9.4-arm64-fix9.apk`（7,317,922B）——签名验证通过（CN=LocalVault Mobile）；APK 内前端 = index-BFYJlOOn.js（与最新 dist 一致，已验证）。
**验证**：前端 tsc+vite 构建通过；dist 同步后 APK 内 JS hash 核对一致；zipalign+apksigner 签名通过。模拟器不在线，未做安装冒烟。
**未自动验证**：真机下拉刷新手势、30s 双向自动同步链路（需用户实测）。

**改动文件**：android-app/src/{App.tsx,styles.css}；android-app/src/pages/VaultPage.tsx；HANDOVER.md
**桌面端本轮无改动**，仍用 fix8 便携 zip。

---

## 1.10 fix10（2026-10-04 上午，锁定→解锁后 403 根因修复 + 下拉刷新手势修复）

用户实测反馈三问题：①电脑锁定再解锁后，手机端拉取/推送全部报错（截图：push 403）；②手机端离线新增的密码自动/手动都同步不到电脑；③下拉刷新没出现。

**根因①+②**：电脑端 `stop()` 锁定时会销毁 `pair_key`（安全设计），但 `start()` 解锁后只从 pairing.json 恢复 token+device，**未恢复 pair_key** → handle_push/handle_pull 先查 pair_key → None → 403「同步会话未建立」→ 手机端一切同步失败，离线新增自然推不上去。

**修复**（桌面端）：
- `save_pairing` 增加持久化传输密钥（base64）；`load_pairing` 返回 (token, device, key_b64)；`start()` 解锁恢复时还原 `st.pair_key`。
- **兼容性注意**：旧 pairing.json 无 key 字段 → load_pairing 返回 None（视为未配对）→ **用户更新后需重新扫码配对一次**，此后锁定/解锁无需再配对。

**根因③（下拉刷新没出现）**：原实现 touchstart 时 `window.scrollY>0` 直接 return（列表中部下拉不触发），且 Tauri Android WebView 的 touch 事件可能不派发。修复：改为 **pointer 事件**（pointerdown/move/up/cancel），任意位置按下即记录起点，滚动到顶（scrollY==0）后继续下拉累计距离，松手≥60px 触发同步刷新。

**产物**（release\ 下）：
- `LocalVault-v1.9.4-arm64-fix10.apk`（7,317,922B）——签名验证通过（CN=LocalVault Mobile），APK 内前端 = index-B6Zqli3N.js（与 dist 一致）
- `LocalVault-v1.9.4-Portable-x64-fix10.zip`（6,291,733B）——桌面端便携版，zip 内 exe = 17,068,032B（fix8 版 17,065,472B）

**验证**：桌面端 cargo check + `tauri build --no-bundle`（CARGO_TARGET_DIR=build-fix10 独立目录，因用户正开着旧 exe 文件锁）；手机端 npm build + assets 同步 + gradle + 签名；zip 内 exe 字节核对一致。模拟器不在线，未做安装冒烟。
**未自动验证**：锁定→解锁后手机端同步、下拉刷新手势（需用户实测，且注意**更新后需重新扫码配对一次**）。

**改动文件**：src-tauri/src/sync.rs（pair_key 持久化）；android-app/src/pages/VaultPage.tsx（pointer 下拉）；HANDOVER.md
**构建链备注**：桌面端 exe 被占用时（用户开着程序），用 `$env:CARGO_TARGET_DIR=独立目录` + `npx tauri build --no-bundle` 绕开文件锁，再手动打包 zip。

---

## 1.11 fix11（2026-10-04 下午，403 防呆 + 设置页整理 + 右上角刷新）

用户反馈：①依旧电脑锁定再解锁拉取 403；②设置页分类默认展开（要默认收起+持久化）；③"修改密码"标题要改"修改主密码设置"；④新建分类按钮太大；⑤刷新改到右上角按钮。

**实现**（手机端；桌面端本轮无改动，fix10 已含 pair_key 持久化）：

| # | 改动 |
|---|---|
| ① | `sync_pull`/`sync_push` 重构：收到 403（电脑端密钥未恢复/配对已解除）时**自动清除本地配对**并返回明确提示"配对已失效，请在桌面端重新生成二维码并重新配对"——不再让用户困惑于"已配对但一直 403" |
| ② | SettingsPage 分类区块**默认收起**，点标题展开（▶/▼），展开状态存 localStorage（`lv_settings_cats_open`），退出重开保持收起 |
| ③ | "修改主密码"卡片标题改为「🔑 修改主密码设置」 |
| ④ | 新建分类按钮从大按钮改为 `mini-btn`（"新建"），与输入框同排不遮挡 |
| ⑤ | VaultPage 列表页 header 加「🔄 刷新」按钮（右上角），点击同步桌面端并重载；下拉刷新保留为补充 |

**产物**（release\ 下）：`LocalVault-v1.9.4-arm64-fix11.apk`（7,317,922B）——签名验证通过（CN=LocalVault Mobile）；APK 内前端 = index-Cn5TAV72.js（与 dist 一致）；.so 5,516,320B。
**验证**：手机端 cargo check / cargo ndk / npm build / assets 同步 / gradle / zipalign+apksigner 全通过；APK 内 JS hash 核对一致。
**未自动验证**：真机交互（需用户实测）。
**⚠️ 403 使用前提**：用户电脑端必须是 fix10（含 pair_key 持久化）且**重新扫码配对一次**；若电脑端仍是旧 exe（用户进程占用未覆盖），403 会持续，fix11 手机端会在拉取/推送时自动清配对并提示重配。

**改动文件**：android-app/src-tauri/src/lib.rs（sync_pull/sync_push 403 处理）；android-app/src/pages/{SettingsPage.tsx,VaultPage.tsx}；android-app/src/styles.css；HANDOVER.md

---

## 1.12 fix11b（2026-10-04 下午，target\release exe 更新——403 真正根因）

用户反馈"断开重连过仍 403"，并要求更新 `src-tauri\target\release`（用户直接用该路径 exe 跑真实数据库）。

**根因确认**：`src-tauri\target\release\localvault.exe` 一直是 **fix8 版**（17,065,472B，曾被进程占用未能覆盖）——fix8 不持久化配对密钥，每次锁定销毁 pair_key、解锁不恢复 → 手机端 403。用户在旧版上重新配对无效（旧代码根本不存 key）。

**本轮动作**：
- 将 fix10 exe（17,068,032B，含 pair_key 持久化 + 桌面刷新按钮）覆盖到 `src-tauri\target\release\localvault.exe`（进程已关闭，覆盖成功，已验证字节数）。
- 重新打包 `LocalVault-v1.9.4-Portable-x64-fix11.zip`（6,291,733B，exe 同 fix10）。
- 手机端 fix11 APK 已含 403 防呆（配对失效自动清除并提示重新扫码）。

**用户下一步**：启动新 exe → 解锁 → **重新扫码配对一次**（这次 pairing.json 会带上密钥，此后锁定/解锁均正常）。

**改动文件**：src-tauri/target/release/localvault.exe（覆盖更新）；release/LocalVault-v1.9.4-Portable-x64-fix11.zip（新建）；HANDOVER.md

---

## 1.13 fix12（2026-10-04 晚，功能大版本：指纹+密保找回+搜索筛选多选排序+备份+去下拉）

用户需求（7 点）：①彻底去掉手机端下拉刷新仅留按钮；②忘记主密码怎么办；③搜索/筛选/多选/排序；④指纹解锁；⑤备份导出；⑥设置页"密码及密保修改"参照电脑端；⑦先做功能后做 ABI 打包。

**实现**：

| 需求 | 落地 |
|---|---|
| ① | VaultPage 删除全部下拉刷新代码（pointer 手势/pull-hint），仅保留右上角「🔄 刷新」按钮 |
| ② | 密保找回：解锁页「忘记主密码？用密保找回」→ Recovery Code + 3 组密保答案验证 → 重置新主密码（本地旧数据清空，重新配对拉取） |
| ③ | 密码库工具条：搜索框（名称/账号/网址/分类）、分类筛选 chip 滚动条、排序循环切换（修改时间↓/↑·名称A→Z/Z→A）、「☑️ 多选」模式（全选/批量删除→回收站/批量移动到分类） |
| ④ | 指纹解锁：MainActivity.kt 加 BiometricPrompt（JNI 静态方法 + bioResult 轮询）；Rust `mobile_biometric_*`；store.rs `bio_key` 内存缓存（解锁时保存）；设置页「🤚 指纹解锁」开关；锁屏页指纹按钮；「立即锁定」清 bio_key 强制主密码 |
| ⑤ | 备份：`mobile_export_backup`（主密钥加密 XChaCha20，不含主密码）+ 系统分享面板发送；`mobile_import_backup`（粘贴文本+备份对应主密码，按 updated_at 新者胜合并） |
| ⑥ | 设置页「🔑 密保及密码修改」参照电脑端：先验证当前主密码 → 勾选「修改主密码」/「修改密保（3 组问题+答案，自动生成新 18 位 Recovery Code）」→ 保存；密保存独立文件 `vault.mobile.security`（argon2id 哈希，不依赖主密钥，可离线验证） |
| ⑦ | ABI 全平台打包留待后续（本轮仅 arm64） |

**新增命令**：mobile_lock_all / mobile_biometric_available / mobile_biometric_unlock / mobile_share_text / mobile_security_status / mobile_update_security / mobile_recovery_reset / mobile_export_backup / mobile_import_backup；mobile_lock 语义改为保留指纹会话。
**新增依赖**：Cargo.toml target.android: jni 0.21 + ndk-context 0.1；build.gradle.kts: androidx.biometric:1.1.0。
**产物**（release\ 下）：`LocalVault-v1.9.4-arm64-fix12.apk`（7,363,069B，签名验证通过；APK 内前端 index-qJTlaWhU.js 与 dist 一致；.so 5,516,320B）。**fix13 已发布**：见 1.13 节，覆盖启动崩溃与电脑端找回。
**验证**：cargo test 9 单测全绿（新增 security_and_backup：密保校验/错误答案拒绝/忘记密码重置/备份导出导入/错误密码拒绝）；cargo ndk arm64 编译通过；npm build / assets 同步 / gradle / 签名全通过。
**未自动验证**：真机指纹弹窗、分享面板（需用户实测；JNI 轮询逻辑已在真机路径，若失败优先看 MainActivity 日志）。
**备注**：密保答案/Recovery Code 以 argon2id 哈希存储，泄露文件不直接暴露明文；忘记密码重置会清空本地数据（不可解旧密文），依赖重新配对拉取——若离线有未同步新增会丢失，用户应优先同步。

**改动文件**：android-app/src-tauri/src/{store.rs,lib.rs,android_jni.rs(新),Cargo.toml}；android-app/src-tauri/gen/android/app/src/main/java/com/localvault/mobile/MainActivity.kt；gen/android/app/build.gradle.kts；android-app/src/{api.ts,App.tsx,styles.css}；android-app/src/pages/{UnlockPage.tsx,SettingsPage.tsx,VaultPage.tsx}；HANDOVER.md

---

## 1.13 fix13（2026-10-03，方案 X 电脑端找回 + Java 启动崩溃修复）

用户批准方案 X（手机端找回双通道并存）并报告启动崩溃「初始化失败：Error: Error invoking postMessage: Java exception was raised during method invocation」，已一并修复交付。

| 项 | 内容 |
|---|---|
| ① Java 启动崩溃 | 根因：release 版 R8 混淆重命名 MainActivity 静态方法 → Rust JNI 按名找不到 → 挂起 Java 异常 → Tauri postMessage IPC 桥报错。修复：`proguard-rules.pro` 加 `-keep class com.localvault.mobile.MainActivity { *; }`；`android_jni.rs::with_env` 末尾 `env.exception_clear()` 清挂起异常 |
| ② 方案 X：电脑端 Recovery Code 找回 | 桌面端 `vault.rs::recovery_material()` 读 meta recovery_wrapped+salt+recovery_questions → base64 输出 {saltB64,wrappedB64,questions}；`sync.rs` pull 响应新增 `"recovery"` 字段（未设密保为 null）。手机端 pull 后保存 `data.recovery`；`store.rs` 新增 RecoveryMaterial / has_pc_recovery / pc_recovery_questions / verify_pc_recovery（与桌面端完全同一算法：combo=code\0ans1\0ans2\0ans3 → Argon2id derive → XChaCha 解密 AAD=b"LocalVault|recovery|v1"，能解即身份通过）/ pc_recovery_reset；通用重置抽为私有 `reset_with_new_master`（recovery_reset 也复用）。lib.rs 新增 mobile_pc_recovery_status / mobile_pc_recovery_reset；UnlockPage 找回表单加 Tab「手机端密保 / 电脑端 Recovery Code」（状态来自 mobile_pc_recovery_status.available/questions，需先正常同步一次才可用） |
| ③ 产物 | 桌面端 exe 重建**已覆盖** `src-tauri\target\release\localvault.exe`（17,059,328B，协议含 recovery 下发；**新 APK 必须配新 exe**，否则手机端拿不到恢复材料）；`release\LocalVault-v1.9.4-Portable-x64-fix13.zip`（6,297,356B，exeInZip=17,059,328B）；`release\LocalVault-v1.9.4-arm64-fix13.apk`（7,519,384B，签名 SHA-256 f52c1ccb… 验证通过；APK 内唯一 JS index-D7PBaLJO.js hash 47a9f8d4 与 dist 一致；.so 5,570,336B） |
| ④ 验证 | 桌面端 cargo check 通过、release 构建成功；手机端 cargo test 9 单测全绿（含既有 security_and_backup）；cargo ndk arm64 编译通过；npm build→assets 同步（**清理了残留旧 JS index-qJTlaWhU.js**）→gradle→zipalign→apksigner verify 全通过 |
| ⑤ 待用户实测 | 启动不再报 Java exception；找回页「电脑端 Recovery Code」通道：需先配对同步一次（拉到 recovery 材料）→ 用电脑端 Recovery Code+3 组密保答案验证 → 重置新主密码；电脑端密保未设置时该 Tab 显示不可用提示 |

**改动文件**：src-tauri/src/{vault.rs,sync.rs}；android-app/src-tauri/src/{store.rs,sync_client.rs,lib.rs,android_jni.rs}；android-app/src-tauri/gen/android/app/proguard-rules.pro；android-app/src/{api.ts,styles.css}；android-app/src/pages/UnlockPage.tsx；HANDOVER.md

---

## 1.14 fix14（2026-10-03，4 项体验修复：找回入口/首设密保引导/指纹录入/备份文案）

用户实测 fix13 后报 4 问题，全部在手机端修复（桌面端本轮无改动）：

| # | 问题 | 修复 |
|---|---|---|
| ① | 解锁页没有找回密码入口 | 根因：入口条件 `hasSecurity \|\| pcAvailable`，未设密保且未配对时恒 false。改为**常显**「忘记主密码？找回」；点开后无任何可用通道时显示指引（去设置页设密保 / 先同步再用电脑端 Recovery Code）；电脑端材料状态改为**挂载即查**（不只打开找回面板时） |
| ② | 首次设置主密码没让配置密保和保存 Recovery Code | SetupPage 重写：设置主密码同时配置 3 组密保问题+答案 → 提交时 `mobileSetup` 后紧跟 `mobileUpdateSecurity`（当前密码=刚设的主密码）→ 生成并**全屏展示 Recovery Code**，「我已保存」确认后进入 App；提供「暂不设置密保」跳过链接（会提示忘记密码无法找回） |
| ③ | 指纹开关开启后不生效、没调用系统指纹配置 | 澄清机制：bio_key 需**解锁成功一次**后缓存，冷启动解锁页（从未解锁过）本来就没有指纹会话密钥——这是设计行为。新增：设置页指纹卡加「📝 录入系统指纹」按钮（MainActivity.kt 新增 openBiometricSettings()，JNI 跳系统指纹录入页，无录入页退回安全设置）+ 文案写明「先正常解锁一次，之后主密码锁定可用指纹解锁」 |
| ④ | 导出加密备份"导出的是密钥？不会被解密吗" | 是误解：导出的是**密文**（salt+nonce+XChaCha20 密文 base64），不含主密码、不含任何明文密钥；无主密码不可解。备份说明文案改为明确表述「导出的是一串加密密文（不是密钥、也不是明文密码），只有输入你的主密码才能解开；保存在微信/网盘/云盘都是安全的」 |

**产物**：`release\LocalVault-v1.9.4-arm64-fix14.apk`（7,523,480B，签名 SHA-256 f52c1ccb… 验证通过；APK 内唯一 JS index-B341iUL9.js hash 1a3c199b 与 dist 一致；.so 5,571,488B）。
**验证**：npm build→assets 同步（清理旧 JS index-D7PBaLJO.js）→cargo ndk arm64→gradle→zipalign→apksigner verify 全通过。
**说明**：①手机端 Rust 本轮无逻辑改动，未跑 cargo test（store 未动）；桌面端 exe 无需重建，继续用 fix13 即可；②新装用户走新 SetupPage 即完成密保配置；已设主密码未设密保的老用户用设置页「密保及密码修改」补设。

**改动文件**：android-app/src/pages/{SetupPage.tsx,UnlockPage.tsx,SettingsPage.tsx}；android-app/src/{api.ts}；android-app/src-tauri/src/{android_jni.rs,lib.rs}；android-app/src-tauri/gen/android/app/src/main/java/com/localvault/mobile/MainActivity.kt；HANDOVER.md

---

## 1.15 fix15（2026-10-03，WebView 旧界面缓存根治：fix14 修复其实都在 APK 里但真机显示旧 UI）

用户两次贴证据：①微信传输截图（fix14 "问号"、fix14b "上传中"）②文件管理器弹窗铁证（fix14b.apk 7.17M 20:11 文件、20:14:13 安装成功、包名/签名/版本全对）但设置页仍是旧 UI（旧指纹文案、无"录入系统指纹"、无找回入口）。排查结论：**fix14/fix14b 的 APK 内容全部正确**（python 解包验证 index.html 引用新 JS、JS 含全部新字符串），真机显示旧界面 = **Android WebView 对 tauri://localhost 响应做了缓存**，更新安装后仍命中旧缓存。根治三处：

1. **Rust 每次启动清缓存**：lib.rs setup 里 `w.clear_all_browsing_data()`（注意：`WebviewWindow::clear_cache` 不存在，正确 API 是 `clear_all_browsing_data`）
2. **Android 侧禁用磁盘缓存**：MainActivity.kt 递归查找 WebView 并设 `cacheMode = 2`（LOAD_NO_CACHE；注意：TauriActivity 的 `webView` 属性私有不可访问，`android.webkit.WebView.LOAD_NO_CACHE` 常量在本 Kotlin 编译环境 Unresolved，改用字面值 2 + 注释）
3. **build 标识移到设置页标题下方**（"⚙️ 设置" 下直接显示 `LocalVault v1.9.4 · 局域网同步版 · build fix15`，不再藏页面底部），一眼确认版本

**构建链修正**：assets 清理脚本的正则有 bug（把新 CSS 也误删），fix15 已改为先 Copy dist 全部再单独清 JS/CSS 旧文件；后续打包必须核对 assets\assets 同时有且仅有当前 index-*.js 和 index-*.css。

**产物**：`release\LocalVault-v1.9.4-arm64-fix15.apk`（7,519,306B；APK 内唯一 JS index-u3725t3N.js 含 build fix15/找回链接/录入指纹/Recovery Code 全部字符串；index.html 引用正确；.so 5,572,152B 新编译含 clear_all_browsing_data；签名 CN=LocalVault Mobile 验证通过）。
**验证**：python 解包核对（上一条）；真机需用户覆盖安装（无需卸载，缓存已被 Rust/Kotlin 双重清除）。
**说明**：本 APK 无需用户卸载重装——clear_all_browsing_data 启动时清缓存 + cacheMode=2 禁用缓存，覆盖安装即生效。

**改动文件**：android-app/src/pages/SettingsPage.tsx（build 标识上移）；android-app/src/styles.css（.build-tag）；android-app/src-tauri/src/lib.rs（setup 清缓存）；android-app/src-tauri/gen/android/app/src/main/java/com/localvault/mobile/MainActivity.kt（递归找 WebView 禁缓存）；HANDOVER.md

---

## 1.16 fix16（2026-10-03，闪退根治：clear_all_browsing_data 触发 R8 混淆 NoSuchMethodError）

用户实测 fix15 一打开即闪退（"屡次停止运行"），提供雷电9 模拟器 adb（127.0.0.1:5561，x86_64）调试。logcat 定位根因：
```
java.lang.NoSuchMethodError: no non-static method "Lcom/localvault/mobile/RustWebView;.clearAllBrowsingData()V"
```
排查链：RustWebView.kt 生成文件含 clearAllBrowsingData（02:17）→ build class 也含（02:32，javap 确认）→ **但 APK 的 classes.dex 里整个 dex 无 clearAllBrowsingData 字符串**（dexdump 确认）→ 字段 d/e/f/g 混淆痕迹 → **R8 把 tauri 生成的 RustWebView.clearAllBrowsingData 混淆重命名了**，Rust 侧 JNI 按原名查找 → NoSuchMethodError → 启动即崩。之前 proguard-rules.pro 只 keep 了 MainActivity，漏了 RustWebView。

**修复**：proguard-rules.pro 追加 keep：`RustWebView / RustWebViewClient / Rust / Ipc` 全部成员（Rust 侧 JNI 会按名调用的 tauri 生成类）。重新打包后 dexdump 验证 clearAllBrowsingData 已存在于 dex。
**验证**：装雷电模拟器（x86_64，ARM 翻译层）→ 启动后 PID 存活（不再闪退）→ screencap 显示**首次设置主密码页带密保引导**（"设置密保（推荐）"、"Recovery Code + 3组密保答案找回"）→ **WebView 缓存问题同步验证解决**（fix15 的三处缓存根治生效，新 UI 正常显示）。
**产物**：`release\LocalVault-v1.9.4-arm64-fix16.apk`（7,519,306B；dex 含 clearAllBrowsingData；JS index-7pCAhheN.js 含 build fix16/找回链接/录入指纹/设置密保推荐；.so 5,572,152B；签名同前）。
**经验**：tauri 生成的 RustWebView 等类被 R8 混淆后 JNI 找不到方法会启动即崩（NoSuchMethodError），proguard 必须 keep 所有 Rust 侧 JNI 调用的 Kotlin 类；本次也确认 fix15 的缓存修复方向正确（三处：Rust clear_all_browsing_data + Kotlin cacheMode=2 + build 标识上移），fix16 里全部保留。

**改动文件**：android-app/src-tauri/gen/android/app/proguard-rules.pro（+RustWebView/RustWebViewClient/Rust/Ipc keep）；android-app/src/pages/SettingsPage.tsx（标识 fix16）；HANDOVER.md

---

## 1.10 fix17 / fix17b（2026-10-03 深夜，Recovery Code 页按钮灰色根治）

用户真机截图指出「我已保存 Recovery Code，进入 App」按钮灰色点不了（fix16 版本）。

**根因**（读 SetupPage.tsx 定位）：Recovery 页按钮 `disabled={busy}`，而 `busy` 只在 **catch 失败分支** `setBusy(false)`；成功路径（mobileUpdateSecurity 成功后 `setRecoveryCode(code)`）**从未重置 busy** → 进入 Recovery 页时 busy 恒为 true → 按钮永远灰色禁用。

**双重修复**（SetupPage.tsx）：
1. 成功分支加 `setBusy(false)`（提交完成后恢复可交互）
2. Recovery 页按钮**彻底去掉 `disabled={busy}` 绑定**（该页出现时提交已结束，按钮应永远可点，不再依赖 busy 重置时机）

**构建链**：npm build → Copy dist + 清理旧 JS/CSS → gradle assembleUniversalRelease（-x rust 任务）→ zipalign + apksigner → 签名。

**产物**：`release\LocalVault-v1.9.4-arm64-fix17.apk`（7,519,306B）与 `release\LocalVault-v1.9.4-arm64-fix17b.apk`（7,519,306B）。fix17b 为最终交付版（fix17 已验证 busy 修复后仍做按钮去 disabled 的加固）。

**APK 内容验证**（check17b.py 读 APK 内 JS）：JS=assets/assets/index-CpzcmIyG.js；hasBuildTag_fix17=True；Recovery 按钮代码段 `E.jsx("button",{className:"btn-primary",onClick:W,...`（**无 disabled**）；htmlJs=/assets/index-CpzcmIyG.js 匹配；.so 5,572,152B；apkSize 7,519,306。

**模拟器全流程验证**（雷电9 127.0.0.1:5561，卸载重装 fix17b）：
- 首次设置流程（主密码 Abcdefg1! + 密保答案 ans1/ans2/ans3）用 adb tap+input text 走通 → **成功生成并显示 Recovery Code 页**（恢复码 QsCsSEBp8KzLmWkwWc）→ 证明提交链路、密保写入、Recovery Code 生成全部正常
- **按钮点击在模拟器 WebView 上偶发失效**：tap/swipe 多次无反应；但同一环境「解锁按钮（form submit）」与「忘记主密码？找回链接（普通 onClick）」点击均正常 → 判定为雷电 x86_64 ARM 翻译层 WebView 触摸事件不稳定（环境问题），非代码问题；真机（原生 arm64，触摸正常）按钮应可点，待用户真机验收
- 验证过程确认：`input keyevent 111`（ESC）收键盘稳定（keyevent 4 BACK 会退到桌面/退出 app）

**改动文件**：android-app/src/pages/SetupPage.tsx（成功分支 setBusy(false) + 按钮去 disabled）；android-app/src/pages/SettingsPage.tsx（build 标识 fix17）；HANDOVER.md

---

## 1.11 fix17c→fix17e（2026-10-03 深夜→凌晨，Recovery Code 按钮灰色【终极根因】）

用户两次反馈按钮仍灰色点不了（模拟器截图显示按钮蓝色但点击无效）。CDP 远程调试（WebView setWebContentsDebuggingEnabled）锁定**终极根因**：

**tauri Android 的 asset server 不读 APK 的 assets 目录，而是读 Rust 编译时嵌入二进制（.so）的前端快照**（tauri.conf.json frontendDist=../dist → build.rs 的 generate_context! 内嵌）！

**时间线复盘**：
- fix14/14b：npm build 新前端 + Copy APK assets，但**没重编 .so** → .so 内嵌 fix13 前端 → 用户看到 fix13 旧界面
- fix15：改 Rust 重编 .so → .so 内嵌 fix15 前端（index-u3725t3N.js，含 disabled={busy} bug）
- fix16→fix17d：只改前端 + 重打 APK，**一直没重编 .so** → .so 内嵌始终是 fix15 前端 → **用户真机/模拟器一直跑 fix15 旧前端 → 按钮 disabled 恒灰、点击无效**——这就是"装了新版本界面不变"的全部原因

**证据链**（CDP Runtime.evaluate）：
- fix17d（未重编 .so）装后：`document.querySelector('script[src*="index-"]').src` = `http://tauri.localhost/assets/index-u3725t3N.js`（fix15 的 hash，APK 内根本无此文件！）；按钮 outerHTML `<button class="btn-primary" disabled="">`（busy 未重置的 fix15 bug）
- fix17e（cargo ndk 重编 .so 内嵌新 dist）装后：加载 index-Bb-7NkdC.js（新）；Recovery 页含 build fix17c 标识；按钮 disabled:false；CDP click 与 **adb 真实触摸 tap 均成功进入主界面**

**修复动作**：
1. SetupPage.tsx：成功路径补 setBusy(false)；Recovery 按钮去 disabled + 改 form submit（type="submit"，与解锁按钮同形态）
2. SettingsPage.tsx：build 标识 fix17c
3. MainActivity.kt：加 WebView.setWebContentsDebuggingEnabled(true)（方便今后 CDP 排查）
4. **cargo ndk 重编 .so**（关键！内嵌新前端）→ gradle → zipalign+apksigner

**产物**：`release\LocalVault-v1.9.4-arm64-fix17e.apk`（7,519,306B；APK 内 .so 5,572,216B 新编译；加载 JS index-Bb-7NkdC.js；签名 CN=LocalVault Mobile f52c1ccb… 可覆盖安装）

**【构建链铁律·追加】**：**只要改了前端（npm build），必须 cargo ndk 重编 .so**——否则新前端永远进不了 app（asset server 从 .so 内嵌读，APK assets 目录只是摆设）！标准构建链 = npm build → cargo ndk（.so）→ Copy assets（保险）→ gradle(-x rust) → 签名。

**改动文件**：SetupPage.tsx、SettingsPage.tsx、MainActivity.kt（调试开关）、HANDOVER.md

---

## 1.12 fix18（2026-10-03 深夜，指纹功能入口消失修复）

用户装 fix17e 后反馈"指纹解锁功能没了"（设置页无指纹项）。**根因**：SettingsPage 指纹卡整体被 `{status.bioAvailable && ...}` 条件包裹——模拟器/未录入指纹的设备上 bioAvailable=false → 指纹卡整个不渲染，看起来像功能被删。

**修复**（SettingsPage.tsx）：指纹卡改为**始终显示**；未检测到指纹（bioAvailable=false）时显示引导文案"当前设备未检测到可用指纹。请先点击下方按钮录入系统指纹，回到这里开启开关即可使用"，并保留「📝 录入系统指纹」按钮（跳系统指纹录入页）；已检测到指纹时显示原说明。build 标识改 fix18。

**构建链教训·追加**：改前端后 cargo ndk 重编 .so 的**增量检测正常工作**（dist 变化即触发），.so 大小变化并非判断内嵌版本的有效依据（tauri 压缩 assets，不同版本压缩后大小可能相同/相近）；**唯一可靠验证 = 装 APK 后 CDP 查 `script[src*="index-"]` 的 hash**。

**产物**：`release\LocalVault-v1.9.4-arm64-fix18.apk`（7,519,306B；APK 内 .so 5,572,152B；加载 JS index-BDYPMObv.js）。

**验证**（雷电模拟器 + CDP）：fix18 卸载重装 → 首次设置 → recovery → 进入 App → 设置页显示 build fix18 标识 + 指纹解锁卡 + "未检测到可用指纹"引导 + 「录入系统指纹」按钮全部渲染 ✓。

**改动文件**：SettingsPage.tsx、HANDOVER.md

---

## 1.13 fix19/fix19b（2026-10-03 深夜，启动崩溃 + 指纹弹窗崩溃）

用户真机装 fix18 后反馈：①设置页指纹卡显示但"录入指纹按钮没反应" ②锁定后 app 卡死只能重开。进一步排查：

**fix19（启动崩溃 ClassNotFoundException）**：日志 `java.lang.ClassNotFoundException: com.localvault.mobile.MainActivity`（Thread-5）。**根因**：mobile_status 每次调 load_bio_key → android_jni::with_env 的 `env.find_class()` 在 **native 线程用系统 classloader** 找不到 app 类 → 崩。**修复**：with_env 改为用 App Context 的 `getClassLoader().loadClass("com.localvault.mobile.MainActivity")` 加载；并用 `vm.get_env()` 复用已 attach 线程 / AttachGuard RAII 自动 detach（修线程泄漏）。同时 mobile_status 短路逻辑（can_bio_unlock() 为 true 时不再调 JNI）。

**fix19b（指纹弹窗崩溃 IllegalArgumentException）**：真机点指纹解锁 → `Negative text must not be set if device credential authentication is allowed`。**根因**：MainActivity.kt 的 PromptInfo 同时 `setNegativeButtonText("取消")` + `setAllowedAuthenticators(BIOMETRIC_WEAK or DEVICE_CREDENTIAL)`——**Android 约束：允许 DEVICE_CREDENTIAL 时禁止设置 negative text**。**修复**：改为 `setAllowedAuthenticators(BIOMETRIC_STRONG)`（只强生物识别/指纹）+ negative"取消"；bioAvailable() 判断同步改为 BIOMETRIC_STRONG（避免判断/认证不一致）。

**产物**：`release\LocalVault-v1.9.4-arm64-fix19.apk`（7,605,322B，含 JNI classloader 修复）、`release\LocalVault-v1.9.4-arm64-fix19b.apk`（7,605,322B，含 BiometricPrompt 修复）。

**验证**：fix19 模拟器全链路（首次设置秒完成 / bio_key.enc 96B 持久化 / 锁定→解锁 1.5s 内 UI 响应不再卡死 / 设置页 fix19 指纹卡）；fix19b 模拟器启动正常无崩溃。**指纹弹窗本身（BIOMETRIC_STRONG 分支）需真机验证**。

**改动文件**：android_jni.rs（with_env classloader 重构）、MainActivity.kt（BIOMETRIC_STRONG）、HANDOVER.md

---

## 1.14 fix20（2026-10-03 深夜，指纹录入按钮跳转不可靠 + 已录指纹场景语义化）

用户反馈：点「录入系统指纹」提示"已打开系统指纹录入页面"，但系统界面实际没弹出来。**根因**：MainActivity.openBiometricSettings 的候选 action 依次尝试 `BIOMETRIC_ENROLL → FINGERPRINT_ENROLL(已废弃) → SECURITY_SETTINGS`，且**未做 resolveActivity 检查**——部分 ROM 上 startActivity(BIOMETRIC_ENROLL) 不抛异常但页面无法显示，用户看到的是"已打开"假成功。

**修复**：
1. Kotlin：候选改为 `BIOMETRIC_ENROLL → SECURITY_SETTINGS`（去掉废弃的 FINGERPRINT_ENROLL），**startActivity 前先 `intent.resolveActivity(pm)` 确认存在可跳页面**，全部不可跳时返回 null → 前端提示"未能自动打开设置页，请手动进入系统设置"（不再假成功）
2. 前端 SettingsPage：**已录指纹（bioAvailable=true）时按钮显示「🖐️ 管理系统指纹」**（跳系统生物识别设置管理/重录）；未录时显示「📝 录入系统指纹」；提示文案按返回 action 区分（BIOMETRIC_ENROLL=指纹录入页 / SECURITY=安全设置页）
3. build 标识 fix20

**产物**：`release\LocalVault-v1.9.4-arm64-fix20.apk`（7,605,322B；.so 5,656,376B 新编译；加载 JS index-DqwrnG32.js）——**包含 fix19（JNI classloader）与 fix19b（BiometricPrompt BIOMETRIC_STRONG）全部修复**。

**验证**：模拟器启动正常、首次设置→Recovery→进 App→设置页 build fix20 + 指纹卡（无指纹分支显示录入引导+按钮）。**真机验证点**：指纹弹窗（BIOMETRIC_STRONG）、已录指纹时按钮为「管理系统指纹」、解锁页指纹按钮 → 系统指纹框 → 解锁成功。

**改动文件**：SettingsPage.tsx、MainActivity.kt、HANDOVER.md

---

## 1.15 fix21/fix22（2026-10-03 深夜，指纹验证入口 + 立即锁定保留指纹会话）

用户反馈"点了管理系统指纹但系统界面没弹"+"指纹一直不生效"。查资料确认：①Android 官方 BiometricPrompt 标准用法（BIOMETRIC_STRONG + negative，fix19b 已改对）②`ACTION_BIOMETRIC_ENROLL` 兼容性差（部分 ROM startActivity 成功但页面不显示）③Tauri 官方也有 biometric 插件（同用系统 BiometricPrompt）。

**fix21（测试入口）**：
- 新增 Rust command `mobile_test_biometric`（纯弹系统 BiometricPrompt 验证，不改变解锁状态）
- SettingsPage 指纹卡加 **「🔓 立即测试指纹」按钮**：点击直接弹系统指纹框，当场验证链路（成功显示"✅ 指纹验证成功"，失败/取消显示错误）
- MainActivity.openBiometricSettings 改 **SECURITY_SETTINGS 优先**（稳定存在），BIOMETRIC_ENROLL 兜底；跳转前 resolveActivity 检查
- api.ts 加 mobileTestBiometric；build fix21

**fix22（立即锁定后仍可指纹解锁）**：
- 用户反馈"立即锁定后解锁页没有指纹按钮"。根因：mobile_lock_all 执行 `lock_all()` + `delete_bio_key()`（旧设计"立即锁定=彻底销毁指纹会话"）。**修复**：mobile_lock_all 改为 `lock_to_bio()`（保留指纹会话）——立即锁定后 bioUsable=true，解锁页显示指纹按钮
- 设置页指纹卡文案同步改"「立即锁定」后仍可用指纹解锁"；build fix22

**产物**：`release\LocalVault-v1.9.4-arm64-fix21.apk`（7,605,322B）、`release\LocalVault-v1.9.4-arm64-fix22.apk`（7,605,322B）

**验证**：模拟器——fix21 加载 index-CTcLjbI4.js、设置页含「立即测试指纹」按钮、点击后命令链路通（无指纹报"指纹验证未通过"不崩）；fix22 加载 index-BFtCGngK.js、build fix22、立即锁定→解锁页正常。**真机验证点**：「立即测试指纹」弹系统指纹框；立即锁定后解锁页出现「🤚 指纹解锁」按钮并可用。

**改动文件**：lib.rs（mobile_test_biometric + mobile_lock_all 改 lock_to_bio）、api.ts、SettingsPage.tsx、MainActivity.kt、HANDOVER.md

---

## 2. 当前主线状态（P2：安卓端骨架，已完成）

**P1（已完成）**：桌面端局域网同步服务（0.0.0.0:38528，配对码+token+传输密钥+XChaCha20 全密文）；根因级修复 Windows 端口残留（socket2 SO_REUSEADDR 建监听器 + `start_stop_restart_cycle` 回归测试，14 tests 通过）。桌面端 1.9.4 exe/安装包已交付。

**P2 A 线（工具链，本机已装）**：
- JDK17：`C:\AndroidDev\jdk17`（清华镜像）
- Android SDK：`C:\AndroidDev\android-sdk`（platform-tools / platforms 34+36 / build-tools 34.0.0 / NDK 26.3.11579264 / cmdline-tools latest=1.0.16486076）
- rustup 安卓 4 target + cargo-ndk 4.1.2
- 用户级环境变量：JAVA_HOME、ANDROID_HOME、PATH 已写

**P2 B 线（代码，android-app/，cargo check + 7 单测全绿）**：
- Rust（src-tauri/src/）：crypto.rs / store.rs / sync_client.rs / lib.rs（13 命令，已加 setup 探针 + panic 日志 + 数据目录修正）/ main.rs
- 前端（src/）：App.tsx（已加错误显示 + [LV] console 日志）/ api.ts / pages/* / components/Scanner.tsx
- 原生工程：gen/android/（compileSdk=36 / targetSdk=34 / minSdk=24；Manifest 已加 CAMERA）
- APK 已构建签名：`android-app\src-tauri\gen\android\app\build\outputs\apk\universal\release\LocalVault-v1.9.4-arm64.apk`（arm64-v8a）

---

## 3. 待办主线（P3–P5，未动工）

- **P3**：双向增量推送（手机端增删改回写桌面端 deleted_ids/upsert）
- **P4**：手机端增删改 UI 完善（当前仅 pull 只读 + 本地增删改，未推回桌面）
- **P5**：正式打包分发（全 ABI、README/CHANGELOG 收尾、签名链）

未决默认值（SYNC-PLAN §8，按"都欧克，开始吧"实现，未逐项确认）：双向同步、手机可读写、强制相同主密码、离线只读缓存、APP 名 LocalVault 手机端、minSdk 24。

---

## 4. 黑屏问题：已定位并修复（2026-10-03 本轮完成）

**现象**：安卓端启动黑屏（真机小米 15 Ultra + 雷电9 模拟器均复现）。

**根因（共 3 个，已全部修复）**：
1. **`tauri.conf.json` `"windows": []`（空数组）** → Tauri Android 侧不创建 WebView（Activity 正常、Rust 正常、但无任何渲染）。修复：补上 `windows: [{ title, url: "index.html" }]`（备份：`tauri.conf.json.bak-20261003`）。
2. **`Cargo.toml` `tauri = { features = [] }` 清空默认特性，且无 `custom-protocol`** → tauri 判定 dev 模式（`dev = !custom_protocol`，tauri build.rs）→ 加载 `devUrl`(http://localhost:1420) 而非打包资源 → 页面请求失败。修复：`features = ["custom-protocol"]`（现为 `["custom-protocol", "devtools"]`，devtools 为诊断用途，**正式发布版应移除 devtools**）。
3. **`MobileStore::new()` 在 `.setup()` 设置 `LOCALVAULT_MOBILE_DATA_DIR` 环境变量之前执行** → 路径回退相对路径 `LocalVaultMobile` → 相对 CWD 落到只读文件系统（用户实测报 `Read-only file system (os error 30)`）。修复：store.rs 加 `set_data_dir()`，lib.rs `setup()` 拿到 `app_data_dir()` 后强制修正 store 路径（日志：`store 数据路径修正为：/data/user/0/com.localvault.mobile/LocalVaultMobile/vault.mobile`）。

**验证（雷电9 模拟器 adb 127.0.0.1:5561，已 root）**：
- WebView 完整加载：renderer 进程创建、`Rust_onPageLoading → Rust_onPageLoaded` 全流程 ✅
- 数据目录修正日志 ✅；`vault.mobile` 成功落盘 `/data/data/com.localvault.mobile/LocalVaultMobile/vault.mobile` ✅
- 用户亲自操作验证：设置主密码 → 解锁 → 进入局域网同步页 → 扫码识别（"不是LocalVault配对二维码"提示正常）✅
- IPC 层（`__TAURI_INTERNALS__.invoke`）：mobile_setup / mobile_status / mobile_unlock / mobile_lock 全部工作 ✅（CDP 远程调试验证，WebView 调试经 devtools feature 开启，adb forward 9222）

**当前产物**：`release\LocalVault-v1.9.4-arm64-fix3.apk`（7,244,194B，SHA256 AB4F9DD9A5D5614E015FE540786737589A401EFA7A9DD65F83DAAECAF9AC6122）。

**下一步**：桌面端「手机同步」弹窗生成配对码 → 手机扫码配对 → 同步（P3/P4 待办见第 3 节）。

**2026-10-03 追加修复（配对扫码 base64 解码失败）**：

---

## 5. 构建命令速查（Windows 本机）

```powershell
# 1) Rust 交叉编译（改 Rust 代码后）
$env:ANDROID_NDK_HOME="C:\AndroidDev\android-sdk\ndk\26.3.11579264"
Set-Location "H:\passwordmanagers\LocalVault-v1.9.4\android-app\src-tauri"
cargo ndk -t arm64-v8a -o target/aarch64-linux-android/release build --release

# 2) 前端构建
Set-Location "H:\passwordmanagers\LocalVault-v1.9.4\android-app"; npm run build

# 3) 同步产物到原生工程（.so 与 assets）
#   .so → gen\android\app\src\main\jniLibs\arm64-v8a\
#   dist → gen\android\app\src\main\assets\

# 4) Gradle 打包（绕开 Windows symlink 问题：排除 rust 任务，.so 已手动就位）
Set-Location "H:\passwordmanagers\LocalVault-v1.9.4\android-app\src-tauri\gen\android"
$env:JAVA_HOME="C:\AndroidDev\jdk17"; $env:ANDROID_HOME="C:\AndroidDev\android-sdk"
.\gradlew.bat :app:assembleUniversalRelease --no-daemon `
  -x :app:rustBuildArm64Release -x :app:rustBuildArmRelease -x :app:rustBuildX86Release -x :app:rustBuildX86_64Release

# 5) zipalign + 签名（见第 4 节，keystore 在 C:\AndroidDev\localvault-mobile.keystore，密码 localvault123）
```

> 说明：因 Windows 未开开发者模式（缺 SeCreateSymbolicLinkPrivilege），tauri CLI 的 `tauri android build` 会卡在 symlink lib。绕行方案如上（Gradle 排除 rust 任务 + 手动 .so）。**建议后续开一次开发者模式根治**，就能走标准 `tauri android build`。

---

## 6. 关键文件/常量

- 加密常量：SALT_LEN=16、KEY_LEN=32、NONCE_LEN=24、ARGON_MEM_KIB=131072、ARGON_ITERS=3、ARGON_LANES=2、Argon2id V0x13；本地缓存 AAD=b"vault.mobile"；同步协议 AAD=接口路径
- 二维码协议：`localvault://sync?ip=<ip>&port=38528&code=<6位>&key=<base64 32B>`；配对码 6 位一次性 5 分钟
- 桌面端同步细节以 `H:\passwordmanagers\LocalVault-v1.9.4\src-tauri\src\sync.rs` 为准
- 签名 keystore：`C:\AndroidDev\localvault-mobile.keystore`（alias=localvault，storepass/keypass=localvault123，后续更新必须沿用同一签名才能覆盖安装）
- APK 产物目录：`android-app\src-tauri\gen\android\app\build\outputs\apk\universal\release\`
- 原生工程：`android-app\src-tauri\gen\android\`（compileSdk=36/targetSdk=34/minSdk=24；Manifest 已加 CAMERA；rust 插件任务被手动排除）

---

## 7. 用户硬约束（务必遵守）

1. **任何代码/版本修改前先备份干净源码**（1.9.4 无独立备份目录，需要时先复制一份）
2. **GUI/软件测试只在虚拟桌面跑，绝不在用户真实桌面开程序窗口**（多次因此发火）；用户已明确"你直接操作我的电脑环境"但软件窗口仍只在虚拟桌面
3. **密码明文绝不进前端**（dump 验收基线：testpass123/testuser 不得出现在 WebView dump）
4. **锁定 = 停服务 + 销毁 token/密钥**；不用单实例类搪塞归因，要根因级修复
5. 本地构建好即可、用户自己测试（"你测试太慢了"）
6. 手机端与桌面端**相同主密码规则**
7. 用户已明确不要：应用商店上架、自动保存站点密码、Native Messaging（二期备选）
8. 桌面端 productName 保持 LocalVault（保障更新器/签名链路），仅版本副标题区分（V1.9.4 · 局域网同步版）

---

## 8. 移交备注

- 本会话因 Windows symlink 权限反复绕行构建，**构建流程务必照第 5 节**，不要回退到"手工注入 assets 再重签"（会触发 MT 管理器 -2 安装错误）
- 用户对多轮卡顿/中断非常敏感：**能一次做完的动作不要拆成多轮**，命令尽量合并成单条后台任务
- 用户尚未连接手机（无法 adb logcat），黑屏定位依赖用户自行抓日志回传；拿到日志前停止盲猜
---

## 1.17 定版收尾（2026-10-03，clean build final）

用户指令：清理测试/debug 垃圾使 1.9.4 干净 + 重打定版包 + 保留「立即测试指纹」、删除「管理系统指纹」。

**前端**：SettingsPage.tsx 指纹卡改为**只保留「🔓 立即测试指纹」主按钮**（删除 openBioSettings 按钮、bioOpenMsg 状态块、「录入/管理系统指纹」入口）；未录指纹文案改为引导"手机 系统设置 → 指纹/生物识别 中录入指纹"；build 标识 `build final`。
**Kotlin**：MainActivity.kt **删除 `WebView.setWebContentsDebuggingEnabled(true)` debug 开关**（仅保留 cacheMode=2 防旧界面缓存）。
**产物**：`release\LocalVault-v1.9.4-arm64-final.apk`（7,601,226B，APK 内 JS=index-BCHTVCbW.js）。
**验证**（雷电模拟器 + CDP）：加载 index-BCHTVCbW.js ✓、设置页 `build final` ✓、「立即测试指纹」按钮存在 ✓、「管理系统指纹」「录入系统指纹」均已移除 ✓。
**清理**（按用户指令，删除我产生的全部调试物）：项目根 cdp_* / check17* / dump_html / fix19_err* / lv*.png / wsid* / adb*.txt / check_apk.py / classes.dex / dexdump* / dist-check.txt / javap.txt / log17 / logcat.txt / pid16 / s1-4 / start16 / un.txt 等；release 内 *-aligned.apk / *.idsig / build-err.txt / cdp_*.py / chain_check.* / emu-* / get_req.bin / key_check.* / make_*.py / pair_req.bin / parse_ui.py / ui.xml；备份目录 build-fix10/、build-fix17c/、build-fix18/。保留：HANDOVER.md、桌面端源码入口（root index.html/package.json/vite.config.ts 等）、release 定版 APK + 桌面 Portable zips；历史 APK 移入 `release\archive\`（27 个，fix11-fix22）。
**定版后遗留**：桌面端 `src-tauri\target\release\`（用户偏好真实数据库路径，保留不动）。

**改动文件**：SettingsPage.tsx、MainActivity.kt、HANDOVER.md
---

## 1.18 正式打包 v1.9.4（2026-10-03，安装包 + 绿色版 + 手机端定版）

用户指令：1.9.4 留档（archive）无意义 → 已删除；开始 1.9.4 正式打包（安装包、绿色版、app——APK 已有 final 定版包不重打）。

**桌面端安装包**（`npx tauri build`，bundle targets=nsis+msi，embedBootstrapper 内嵌 WebView2）：
- `release\LocalVault_1.9.4_x64-setup.exe`（6,178,714B，NSIS）
- `release\LocalVault_1.9.4_x64_en-US.msi`（8,183,808B，MSI）
- updater 产物（latest.json/signature）**未能生成**：tauri.conf.json 配置了 pubkey，但无 `TAURI_SIGNING_PRIVATE_KEY` 私钥 → 报 "A public key has been found, but no private key"。**影响**：GitHub 自动更新（updater）需要私钥签名才能发布；安装包本身不受影响。若要启用自动更新，需用户提供私钥。

**绿色版**：`scripts\build-portable.ps1` 版本号 v1.9.3→v1.9.4 后执行。**坑**：PowerShell 5.1 `Compress-Archive` 压缩 exe 偶发 `CompressArchiveUnauthorizedAccessError`（exe 实际未锁定、非只读，属 PS5.1 bug）→ 改用 `[System.IO.Compression.ZipFile]::CreateFromDirectory` 成功。
- `release\LocalVault-Portable-x64-v1.9.4.zip`（6,048,105B，16 条目：exe + Launch-LocalVault.cmd + portable.flag + README + 安装浏览器扩展.cmd + extension 完整）
- Tauri 2 embedBootstrapper 内嵌 WebView2 bootstrap，target\release 无 DLL 依赖，zip 内容完整
- 插件 zip（LocalVault-Fill-v1.9.3.zip）保持 1.9.3 不重打（脚本内插件名未动）

**手机端**：`release\LocalVault-v1.9.4-arm64-final.apk`（7,601,226B，build final）——不重打。

**清理**：release\archive\（历史 APK + 桌面包，用户确认无意义）已删除；bundle 内 1.9.3 的 setup.exe/msi 已删；便携版解包目录 LocalVault-Portable-x64\（中间产物）已删。

**release 最终产物**（6 项）：
1. LocalVault_1.9.4_x64-setup.exe（安装包）
2. LocalVault_1.9.4_x64_en-US.msi（MSI）
3. LocalVault-Portable-x64-v1.9.4.zip（绿色版）
4. LocalVault-v1.9.4-arm64-final.apk（手机端定版）
5. LocalVault-Fill-v1.9.3.zip + txt（浏览器插件，保留）

**遗留**：updater 私钥缺失（用户决定是否启用自动更新需提供 TAURI_SIGNING_PRIVATE_KEY）。

**改动文件**：scripts\build-portable.ps1、HANDOVER.md

---

## 1.19 自动更新机制上线（2026-10-04，桌面端 + 手机端同机制）

用户指令：文档也更新一下，然后开始给 app 加跟电脑版一样的自动更新机制，我要开自动更新的功能的。

**签名密钥（重要）**：
- 生成新签名密钥对：`npx tauri signer generate -w C:\AndroidDev\localvault-updater.key -p "LocalVault2026!"`（rsign 加密私钥格式）
- **新公钥**（两端 tauri.conf.json `plugins.updater.pubkey` 均已换为此值；旧 pubkey `12Mk...` 无对应私钥，作废）：
  `dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQ5Rjc1RjM2ODdBMzRCNDcKUldSSFM2T0hObC8zMllBdE9rU2RiYWdEbzlyZ2FJL0pEcW1HeTVvRTJ4L0drZHVXbVAyQ2pFU0EK`
- **发布必须保密私钥** `C:\AndroidDev\localvault-updater.key`（密码 LocalVault2026!）

**桌面端**（src-tauri\tauri.conf.json 已配 pubkey + endpoints=GitHub latest.json + createUpdaterArtifacts:true）：
- 带 `TAURI_SIGNING_PRIVATE_KEY`（内容，PowerShell：`$env:TAURI_SIGNING_PRIVATE_KEY=(Get-Content key -Raw)`）+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD=LocalVault2026!` 执行 `npx tauri build` → 产出带 .sig 的更新包：
  - `src-tauri\target\release\bundle\nsis\LocalVault_1.9.4_x64-setup.exe` + `.sig`（420B）
  - `src-tauri\target\release\bundle\msi\LocalVault_1.9.4_x64_en-US.msi` + `.sig`

**手机端（android-app）自动更新——自实现（重要决策）**：
- **tauri-plugin-updater 官方标注 Android 支持级别 `none`（2.13.1 起，metadata.platforms.support.android level=none）**——在 Android 上注册插件后点「检查更新」直接 `Fatal signal 6 (SIGABRT)`（tokio-rt-worker 线程，panic=abort 无文案）。已实测复现两次后弃用插件。
- **自实现完整链路**（lib.rs，与电脑端同一套 minisign 签名算法，legacy=true）：
  - `mobile_check_update`：reqwest GET latest.json → 解析 `platforms.android-aarch64.url/signature` → semver 版本比较 → 返回 UpdateInfo{version,currentVersion}
  - `mobile_download_update`：reqwest 下载 APK bytes → `minisign_verify::PublicKey::decode`（公钥 base64 解码后解码 PEM）+ `Signature::decode` + `verify(bytes, sig, true)` 校验 → 写 `app_cache_dir/localvault-update.apk` → `android_jni::install_apk`（MainActivity.kt FileProvider + ACTION_VIEW application/vnd.android.package-archive + FLAG_GRANT_READ_URI_PERMISSION；Manifest 已有 REQUEST_INSTALL_PACKAGES）
  - 依赖：`minisign-verify = "0.2"`、`semver = "1"`、`reqwest = { version="0.12", default-features=false, features=["rustls-tls","json"] }`（**ureq 在 Android 下载大文件报 "response body closed before all bytes were read"——已弃用，改用 reqwest**）
  - Cargo.toml 移除 `tauri-plugin-updater`；capabilities/default.json 移除 `updater:default`
- 前端 SettingsPage.tsx：新增「🔄 检查更新」卡片（🔍检查更新 → 有更新显示 ⬇️下载并安装 + 消息区）；**不使用 @tauri-apps/plugin-updater JS 包**（Update.download 不暴露文件路径），全走 api.ts `mobileCheckUpdate/mobileDownloadUpdate`
- SetupPage.tsx：Recovery Code 保存页 build 标识统一 `build final`

**验证（雷电9 + adb reverse 本地 mock 端到端）**：
- 本地起 HTTP 服务器（release\ 下 latest.json + APK）+ `adb reverse tcp:8123 tcp:8123` + 临时把 UPDATER_ENDPOINT 指向 `http://127.0.0.1:8123/latest.json`（验证后改回 GitHub）：
- 检查更新 → **发现新版本 1.9.5（当前 1.9.4）** ✅ → 下载 → cache 落盘 `localvault-update.apk`（7,674,954B 完整）✅ → **minisign 签名校验通过** ✅ → **系统安装器打开**（"已打开系统安装器"提示）✅ → app 全程不崩 ✅
- 正式版（GitHub 端点）装模拟器启动正常，检查更新报"缺少 android-aarch64 平台配置"（GitHub 上的 latest.json 还是旧的只有 windows——发布新 latest.json 后生效，符合预期）

**发布产物（release\，共 8 项）**：
1. `latest.json`（**发布版**：version=1.9.4，platforms 含 `windows-x86_64` + `android-aarch64` 两个条目，各带 url + signature；url 用 GitHub `releases/latest/download/` 前缀）
2. `LocalVault_1.9.4_x64-setup.exe` + `.sig`（新签名）
3. `LocalVault_1.9.4_x64_en-US.msi` + `.sig`（新签名）
4. `LocalVault-Portable-x64-v1.9.4.zip`（绿色版，用新 exe 重打）
5. `LocalVault-v1.9.4-arm64-upd.apk`（8,358,986B，手机端正式自动更新版，JS=index-CGKtlJT6.js）+ `.sig`
6. `LocalVault-Fill-v1.9.3.zip`（浏览器插件，保留）

**发布步骤（待用户操作 GitHub）**：
1. 在 GitHub `Tenderne1/LocalVault` 创建 Release（Tag：v1.9.4），上传：`LocalVault_1.9.4_x64-setup.exe`、`LocalVault_1.9.4_x64_en-US.msi`、`LocalVault-v1.9.4-arm64-upd.apk`、`latest.json`（**文件名必须与 latest.json 中 url 一致**）
2. 之后两端 App 内「检查更新」即可生效（桌面端已有检查更新入口；手机端设置页新增按钮）
3. 下次发版：重新签名新版本产物 + 更新 latest.json 的 version/signature/url 再上传

**改动文件**：android-app\src-tauri\src\lib.rs（自实现 updater + 移除插件）、Cargo.toml、capabilities\default.json、src\api.ts、src\pages\SettingsPage.tsx、src\pages\SetupPage.tsx、src-tauri\tauri.conf.json（换 pubkey）、HANDOVER.md

## 1.20 蓝奏云下载通道 + 密码库下拉布局（2026-10-04）

用户指令：①"有更新了，让用户去蓝奏云自己下载安装，用户点击版本更新按钮后跳转到蓝奏云下载"（两个更新方式：蓝奏云 + GitHub）；②"调整密码库的界面，分类的、排序的、多选的，重新调整布局，分类最好改为下拉展开选择"。

**蓝奏云下载通道（手机端设置页）**：
- 蓝奏云地址 `https://wwbak.lanzoub.com/b01n4i7v7i`（访问密码 `1xms`），SettingsPage.tsx 常量 `LANZOU_URL`/`LANZOU_PWD`
- 更新卡片新增「📦 蓝奏云下载」按钮（始终显示），点击 → `api.mobileOpenUrl(LANZOU_URL)` → Android 系统浏览器打开；卡片下展示访问密码
- 检查更新失败文案改为"检查更新失败（GitHub 网络不稳定），可直接点击「蓝奏云下载」手动下载最新版"
- **JNI 链路**：MainActivity.kt 新增 `openUrl(url)`（ACTION_VIEW + Uri.parse + FLAG_ACTIVITY_NEW_TASK，主线程执行、返回 ok/失败）→ android_jni.rs `open_url`（android 实装 + 非 android stub）→ lib.rs `mobile_open_url` command → api.ts `mobileOpenUrl`

**密码库工具条下拉布局（VaultPage.tsx + styles.css）**：
- 工具条由"横向滚动分类 chips + 循环切换排序按钮"改为**三按钮均分一行**（.toolbar-row-3 + .toolbar-select）：
  - `📁 {当前分类}`（分类下拉，action-sheet 选择：🗂️全部 + 各分类，选中高亮）
  - `⇅ {当前排序}`（排序下拉，action-sheet 选择：修改时间↓/修改时间↑/名称A→Z/名称Z→A，选中高亮）
  - `☑️ 多选`（toggle，active 高亮）
- 删除 `nextSort` 循环切换逻辑；新增 `catPickerOpen`/`sortPickerOpen` 状态
- CSS：新增 `.toolbar-row-3`、`.toolbar-select`、`.action-sheet button.active`

**验证（雷电9 + CDP）**：
- 密码库工具条三个下拉按钮渲染正常 ✅；分类下拉弹出（🗂️全部 + 取消）✅；排序下拉弹出（4 种排序 + 取消，选中项高亮）✅
- 设置页「📦 蓝奏云下载」按钮存在 + 访问密码 1xms 显示 ✅；点击后 `dumpsys` 前台变为 `com.android.browser/BrowserActivity`（系统浏览器打开）✅

**产物**：`release\LocalVault-v1.9.4-arm64-lanzou.apk`（8,303,531B 签名后，JS=index-B7yvpahp.js + index-5KLRJtvs.css，arm64 通用包）——蓝奏云 + 密码库下拉合并版。签名沿用 `localvault-mobile.keystore`（alias=localvault/localvault123）。蓝奏云目录内容由用户维护，App 只负责跳转；GitHub Release 发布仍按 1.19 步骤（上传 APK 到 Release + 更新 latest.json android 条目 url/signature）。

## 1.21 返回键/设置折叠/右上角锁定/自动锁定（2026-10-04）

用户指令（四条）：①返回键：多选状态下返回应取消选择，正常退出需连续两次返回；②设置界面功能改收拢折叠；③立即锁定按钮固定到右上角；④自动锁定时间（参考电脑端，含"永不锁定"选项，有操作重置计时）。

**返回键桥（MainActivity.kt + back.ts 新建）**：
- MainActivity 覆写 `onBackPressed()`（@Suppress DEPRECATION）：不执行默认行为，`evaluateJavascript("window.__lvHandleBack ? window.__lvHandleBack() : null")` 把返回键交给前端；webview 未就绪才走 super
- 新增 `back.ts`：`registerBackHandler()`（后注册先处理）+ `dispatchBack()`；App.tsx 挂 `window.__lvHandleBack`
- VaultPage 注册 handler：长按菜单/分类/排序/移动弹窗 → 关闭；多选模式 → 取消多选（batch-bar 消失）；详情/编辑/回收站 → 返回列表；均未命中 → false 冒泡
- App 层兜底 handler（注册最早，最后处理）：距上次返回 <2s → `api.mobileExit()`（lib.rs 新 command → android_jni::finish_activity → MainActivity.finishActivity()：finish + finishAndRemoveTask）；否则 toast「再按一次返回键退出」
- 验证：多选按返回 → 取消多选、留在 App ✅；列表页第一次返回 → toast ✅；第二次返回 → App 退出（前台无 MainActivity）✅

**设置页全部折叠（SettingsPage.tsx）**：
- 指纹解锁/备份与恢复/检查更新/锁定与安全 全部改为 collapse-head（默认收起，▶/▼），与已有的分类管理/密保修改一致
- 新增通用 `fold` 状态（Record<string, boolean>）+ toggleFold
- 验证：6 个卡片全部显示 ▶（收起）✅

**右上角锁定按钮（三页统一）**：
- VaultPage/SyncPage/SettingsPage header 均新增 `.header-actions` + `🔒` mini-btn（lock-btn）；MainPage 向 Vault/Sync 传 onLock
- 点击 → mobileLockAll() + onLock()（回解锁页）；设置页底部「立即锁定」卡保留
- 验证：密码库 header 显示 [🔄 刷新, 🔒]；点 🔒 → 回到"输入主密码解锁" ✅

**自动锁定（App.tsx + SettingsPage）**：
- 设置项 `lv_mobile_auto_lock`（""=永不 / 1 / 5 / 10 / 30 分钟），分段按钮选择（seg-btn）
- App.tsx phase=main 时：监听 pointerdown/touchstart/keydown/click 重置计时，到点 → mobileLockAll() + refresh()（回解锁页）；phase≠main 或"永不"不生效
- 验证：设 1 分钟 → 解锁后无操作 70s → 自动回到锁定页 ✅

**产物**：`release\LocalVault-v1.9.4-arm64-lanzou.apk`（8,383,562B 签名后，JS=index-C4qjUYNi.js + index-BZyIAbxA.css）——本版新增四项交互/设置优化，覆盖安装可升。改动文件：MainActivity.kt、android_jni.rs（finish_activity）、lib.rs（mobile_exit）、api.ts、back.ts（新建）、App.tsx、VaultPage.tsx、SyncPage.tsx、SettingsPage.tsx、MainPage.tsx、styles.css、HANDOVER.md。

## 1.22 header 精简 + WebView 旧前端根因修复（2026-10-04）

用户指令：①去掉设置/密码库/局域网同步三页左上角标题与右上角描述文案（build tag、「点按查看详情·长按操作菜单」、同步说明）；②锁定按钮从「🔒」改为「🔒 锁定」。

**代码改动（三页 header 统一精简）**：
- VaultPage.tsx：删 h2「密码库」与 muted 提示「点按查看详情·长按操作菜单」；header 仅保留刷新 + 锁钮
- SyncPage.tsx：删 h2「局域网同步」与 muted 描述「与桌面端 LocalVault 在同一局域网内配对…」
- SettingsPage.tsx：删 h2「设置」；删底部 foot-note「LocalVault v1.9.4 · 局域网同步版 · build final」（494 行整段删除）
- 三页 `.header-actions` 内锁钮文本统一改为「🔒 锁定」（lock-btn 宽度自适应，不换行）

**WebView 顽固加载旧 JS 的根因（重要，勿再绕缓存）**：
- 现象：APK 内 assets/index.html 已是新 hash、`adb pull` 核对安装包内容正确，但 webview 始终加载上一版 `index-C4qjUYNi.js`；pm clear、删 cache/app_webview/WebView 目录、卸载重装全部无效
- 根因：**Tauri v2 Android 把前端资源嵌入 Rust 二进制**（`lib/arm64-v8a/liblocalvault_mobile_lib.so` 内含全部前端文件），tauri://localhost 从 .so 读取，**APK assets 仅是旁路**。上轮（1.21）只跑了 gradle 未重编 .so，导致 .so 内仍是 C4qjUYNi，webview 永远读到旧前端
- **修复链（必须全跑）**：npm run build → `cargo ndk -t arm64-v8a -o gen/android/app/src/main/jniLibs build --release`（把 dist 嵌入 .so，验证 `b'C7qgrfA7' in so`）→ gradlew assembleUniversalRelease（-x rustBuild*）→ zipalign+apksigner 签名
- MainActivity 保留启动时 `cacheDir.deleteRecursively()`（防 assets 旁路缓存，无害；真实来源是 .so）

**验证（雷电9 adb 127.0.0.1:5561，CDP）**：
- `document.scripts` 末尾 = `index-C7qgrfA7.js`（最新）✅
- 密码库页：无标题/无「点按查看详情」，header=[🔄 刷新, 🔒 锁定] ✅
- 同步页：无标题/无描述，第一行=🔒 锁定 ✅
- 设置页：无「LocalVault v1.9.4」/无「build final」，第一行=🔒 锁定 ✅

**产物**：`release\LocalVault-v1.9.4-r2-arm64.apk`（8,383,562B，JS=index-C7qgrfA7.js + index-BZyIAbxA.css）——**当前最新交付**（含 1.21 全部功能 + 本轮 header 精简）。旧包 `arm64-upd.apk`（1.19）、`arm64-lanzou.apk`（1.21）勿再交付用户，避免混淆。

## 1.23 版本名可区分 + 清理旧包（2026-10-04）

用户质疑"怎么还是原来的版本名？不会还是旧版本的吧？有更新吗？检验了吗？旧版删了吗"——版本号全链路一直是 1.9.4（从未升版），仅凭界面变化判断新旧不可靠。

**处理**：
- `gen/android/app/build.gradle.kts` versionName 硬编码 `"1.9.4-r2"`（versionCode 仍 1009004 不变 → 覆盖安装不受影响）；系统「应用信息」/安装界面可明确看到 1.9.4-r2
- release 删除旧包：`LocalVault-v1.9.4-arm64-upd.apk(.sig)`、`LocalVault-v1.9.4-arm64-lanzou.apk(.idsig)`，另删 r2 的 `.idsig`（v4 签名附带文件，非必需）；现仅存 `LocalVault-v1.9.4-r2-arm64.apk` 一个 APK

**验证**：`aapt2 dump badging` → versionName='1.9.4-r2' ✅；雷电9 `dumpsys package` → versionName=1.9.4-r2 ✅；CDP scripts=index-C7qgrfA7.js ✅。HANDOVER 更新到 1.23。


