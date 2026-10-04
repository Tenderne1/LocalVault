// LocalVault 手机端（Android）v1.9.4 局域网同步版
// P2 骨架：主密码设置/解锁/锁定 + 扫码配对 + 基础同步（health/pair/pull）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::sync::Mutex;
use tauri::{Manager, State};
use base64::Engine;

pub mod android_jni;
pub mod crypto;
pub mod store;
pub mod sync_client;

use store::{MobileStore, PairingInfo, SyncEntry};

// ---------- 命令 ----------

/// bio_key ↔ base64（供持久化到 Keystore 加密文件）
fn bio_key_b64(key: &[u8; crypto::KEY_LEN]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key)
}

fn bio_key_from_b64(b64: &str) -> Result<[u8; crypto::KEY_LEN], String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| "指纹会话密钥损坏".to_string())?;
    raw.try_into()
        .map_err(|_| "指纹会话密钥长度错误".to_string())
}

/// 解锁成功后持久化指纹会话密钥（Keystore 加密落盘，冷启动后指纹仍可用）
fn persist_bio_key(s: &store::MobileStore) {
    if let Some(k) = s.current_bio_key() {
        android_jni::save_bio_key(&bio_key_b64(k));
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileStatus {
    has_master: bool,
    unlocked: bool,
    paired: bool,
    pairing: Option<PairingInfo>,
    entry_count: usize,
    /// 设备支持生物识别（指纹/面部）
    bio_available: bool,
    /// 当前锁定状态是否可用指纹解锁（内存会话密钥或持久化 Keystore 会话密钥）
    bio_usable: bool,
    /// 已设置密保（可找回主密码）
    has_security: bool,
}

#[tauri::command]
fn mobile_status(store: State<'_, Mutex<MobileStore>>) -> Result<MobileStatus, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let (unlocked, pairing, entry_count) = match s.data() {
        Some(d) => (true, d.pairing.clone(), d.entries.len()),
        None => (false, None, 0),
    };
    let bio_available = android_jni::biometric_available();
    let bio_usable = bio_available
        && !unlocked
        && (s.can_bio_unlock() || android_jni::load_bio_key().is_some());
    Ok(MobileStatus {
        has_master: s.has_master(),
        unlocked,
        paired: pairing.is_some(),
        pairing,
        entry_count,
        bio_available,
        bio_usable,
        has_security: s.has_security(),
    })
}

#[tauri::command]
async fn mobile_setup(
    password: String,
    confirm: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.setup(&password, &confirm)?;
    persist_bio_key(&s);
    Ok(())
}

#[tauri::command]
async fn mobile_unlock(password: String, store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.unlock(&password)?;
    persist_bio_key(&s);
    Ok(())
}

/// 锁定（保留指纹会话：之后可用指纹解锁）
#[tauri::command]
fn mobile_lock(store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.lock_to_bio();
    Ok(())
}

/// 立即锁定（保留指纹会话：之后仍可用指纹解锁；如需完全清除指纹会话可在后续版本提供）
#[tauri::command]
fn mobile_lock_all(store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.lock_to_bio();
    Ok(())
}

// ---------- 生物识别（指纹/面部） ----------

#[tauri::command]
fn mobile_biometric_available() -> Result<bool, String> {
    Ok(android_jni::biometric_available())
}

/// 指纹解锁：弹出系统生物识别 → 成功后用内存会话密钥直接解锁
/// （无内存会话时从持久化 Keystore 恢复；async 避免 JNI 轮询阻塞 UI）
#[tauri::command]
async fn mobile_biometric_unlock(store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let need_auth = {
        let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
        !s.is_unlocked()
    };
    if !need_auth {
        return Ok(());
    }
    if !android_jni::biometric_authenticate()? {
        return Err("指纹验证未通过或已取消".into());
    }
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    if s.is_unlocked() {
        return Ok(());
    }
    if s.can_bio_unlock() {
        return s.unlock_with_bio();
    }
    if let Some(b64) = android_jni::load_bio_key() {
        let key = bio_key_from_b64(&b64)?;
        s.restore_bio_key(key);
        return s.unlock_with_bio();
    }
    Err("当前没有可用的指纹会话，请使用主密码解锁".into())
}

/// 测试指纹：仅弹出系统生物识别验证（不改变解锁状态），用于设置页验证指纹链路可用
#[tauri::command]
fn mobile_test_biometric() -> Result<(), String> {
    if !android_jni::biometric_authenticate()? {
        return Err("指纹验证未通过或已取消".into());
    }
    Ok(())
}

/// 系统分享文本（备份导出等）
#[tauri::command]
fn mobile_share_text(text: String, subject: String) -> Result<(), String> {
    android_jni::share_text(&text, &subject);
    Ok(())
}

/// 自动更新：下载完成后调用系统安装器安装 APK（tauri-plugin-updater 在 Android 不实现 install）
#[tauri::command]
fn mobile_install_apk(path: String) -> Result<String, String> {
    android_jni::install_apk(&path)
}

/// 打开系统浏览器访问外部 URL（蓝奏云下载页等）
#[tauri::command]
fn mobile_open_url(url: String) -> Result<String, String> {
    android_jni::open_url(&url)
}

/// 前端返回键确认退出：结束 MainActivity（两次返回确认后调用）
#[tauri::command]
fn mobile_exit() {
    android_jni::finish_activity();
}

// ---------- 自动更新（与电脑版同一套机制：GitHub Releases 静态 JSON + minisign 签名校验） ----------
// 说明：tauri-plugin-updater 官方标注 Android 支持级别 none（2.13.1 起在 Android 上运行会 SIGABRT），
// 故这里自行实现 check/download/签名验证/安装，签名算法与电脑端 updater 完全一致（minisign，legacy=true）。

/// updater 公钥（与两端 tauri.conf.json plugins.updater.pubkey 一致）
const UPDATER_PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQ5Rjc1RjM2ODdBMzRCNDcKUldSSFM2T0hObC8zMllBdE9rU2RiYWdEbzlyZ2FJL0pEcW1HeTVvRTJ4L0drZHVXbVAyQ2pFU0EK";
/// 更新检查端点（GitHub Releases 静态 JSON，与电脑端一致）
const UPDATER_ENDPOINT: &str = "https://github.com/Tenderne1/LocalVault/releases/latest/download/latest.json";
/// Android 平台键（latest.json 中 platforms 的键）
const UPDATER_TARGET: &str = "android-aarch64";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    current_version: String,
}

async fn fetch_latest_json() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("初始化更新客户端失败：{e}"))?;
    let resp = client
        .get(UPDATER_ENDPOINT)
        .send()
        .await
        .map_err(|e| format!("请求更新信息失败：{e}"))?;
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("解析更新信息失败：{e}"))
}

fn updater_pubkey() -> Result<minisign_verify::PublicKey, String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(UPDATER_PUBKEY_B64)
        .map_err(|e| format!("公钥解码失败：{e}"))?;
    let pem = std::str::from_utf8(&decoded)
        .map_err(|e| format!("公钥解码失败：{e}"))?;
    minisign_verify::PublicKey::decode(pem)
        .map_err(|e| format!("公钥解析失败：{e}"))
}

fn verify_apk(bytes: &[u8], sig_b64: &str) -> Result<(), String> {
    let pubkey = updater_pubkey()?;
    let sig_decoded = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|e| format!("签名解码失败：{e}"))?;
    let sig_pem = std::str::from_utf8(&sig_decoded)
        .map_err(|e| format!("签名解码失败：{e}"))?;
    let signature = minisign_verify::Signature::decode(sig_pem)
        .map_err(|e| format!("签名解析失败：{e}"))?;
    pubkey
        .verify(bytes, &signature, true)
        .map_err(|e| format!("签名校验失败（更新来源不可信）：{e}"))
}

/// 检查更新（读取 GitHub Releases 的 latest.json，android-aarch64 平台）
#[tauri::command]
async fn mobile_check_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let current = app.package_info().version.to_string();
    let json = fetch_latest_json().await?;
    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "更新信息缺少 version".to_string())?;
    let platform = json
        .get("platforms")
        .and_then(|p| p.get(UPDATER_TARGET))
        .ok_or_else(|| format!("更新信息缺少 {UPDATER_TARGET} 平台配置"))?;
    let url = platform
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| "更新信息缺少下载地址".to_string())?;
    if url.is_empty() {
        return Ok(None);
    }
    let remote = semver::Version::parse(version.trim_start_matches('v'))
        .map_err(|e| format!("更新版本号格式错误：{e}"))?;
    let cur = semver::Version::parse(&current).map_err(|e| format!("当前版本号格式错误：{e}"))?;
    if remote > cur {
        Ok(Some(UpdateInfo {
            version: version.to_string(),
            current_version: current,
        }))
    } else {
        Ok(None)
    }
}

/// 下载更新（签名已在 minisign 校验通过后写入）→ 写缓存 → 调系统安装器
#[tauri::command]
async fn mobile_download_update(app: tauri::AppHandle) -> Result<String, String> {
    let json = fetch_latest_json().await?;
    let platform = json
        .get("platforms")
        .and_then(|p| p.get(UPDATER_TARGET))
        .ok_or_else(|| format!("更新信息缺少 {UPDATER_TARGET} 平台配置"))?;
    let url = platform
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| "更新信息缺少下载地址".to_string())?
        .to_string();
    let signature = platform
        .get("signature")
        .and_then(|s| s.as_str())
        .ok_or_else(|| "更新信息缺少签名".to_string())?
        .to_string();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("初始化下载客户端失败：{e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载更新失败：{e}"))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取更新数据失败：{e}"))?;
    verify_apk(&bytes, &signature)?;
    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let apk_path = dir.join("localvault-update.apk");
    std::fs::write(&apk_path, &bytes).map_err(|e| format!("写入更新文件失败：{e}"))?;
    android_jni::install_apk(&apk_path.to_string_lossy())
}

/// 跳转系统指纹录入设置（多级回退），返回已打开页面描述供前端提示
#[tauri::command]
fn mobile_open_biometric_settings() -> Result<String, String> {
    android_jni::open_biometric_settings()
}

// ---------- 密保（忘记主密码找回） ----------

/// 返回当前是否已设置密保
#[tauri::command]
fn mobile_security_status(store: State<'_, Mutex<MobileStore>>) -> Result<bool, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    Ok(s.has_security())
}

/// 密保及密码修改（参照电脑端）：验证当前主密码后，可选择性修改主密码/密保。
/// 修改密保会生成新的 Recovery Code 并返回（旧码立即失效）。
#[tauri::command]
async fn mobile_update_security(
    current_password: String,
    new_password: Option<String>,
    new_confirm: Option<String>,
    questions: Option<Vec<String>>,
    answers: Option<Vec<String>>,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Option<String>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    if !s.has_master() {
        return Err("手机端尚未设置主密码".into());
    }
    // 先验证当前主密码（与桌面端一致的安全要求）
    s.unlock(&current_password)?;
    // 修改主密码（可选）
    if let (Some(n), Some(c)) = (new_password, new_confirm) {
        s.change_master_password(&current_password, &n, &c)?;
    }
    // 修改密保（可选）：生成新 Recovery Code
    if let (Some(qs), Some(ans)) = (questions, answers) {
        let code = s.save_security(qs, ans)?;
        persist_bio_key(&s);
        return Ok(Some(code));
    }
    persist_bio_key(&s);
    Ok(None)
}

/// 忘记主密码：Recovery Code + 三组密保答案验证 → 设置新主密码（本地旧数据清空，重新同步恢复）
#[tauri::command]
async fn mobile_recovery_reset(
    recovery_code: String,
    answers: Vec<String>,
    new_password: String,
    confirm: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.recovery_reset(&recovery_code, &answers, &new_password, &confirm)?;
    persist_bio_key(&s);
    Ok(())
}

/// 电脑端 Recovery Code 找回状态：电脑端恢复材料是否可用 + 密保问题（供前端展示）
#[tauri::command]
fn mobile_pc_recovery_status(store: State<'_, Mutex<MobileStore>>) -> Result<serde_json::Value, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    Ok(serde_json::json!({
        "available": s.has_pc_recovery(),
        "questions": s.pc_recovery_questions(),
    }))
}

/// 用电脑端 Recovery Code + 3 组密保答案找回手机端主密码（方案 X）
#[tauri::command]
async fn mobile_pc_recovery_reset(
    recovery_code: String,
    answers: Vec<String>,
    new_password: String,
    confirm: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.pc_recovery_reset(&recovery_code, &answers, &new_password, &confirm)?;
    persist_bio_key(&s);
    Ok(())
}

// ---------- 加密备份（导出/导入） ----------

/// 导出加密备份（用主密码派生密钥加密，不含主密码）→ 返回文本，前端调分享面板发送
#[tauri::command]
fn mobile_export_backup(store: State<'_, Mutex<MobileStore>>) -> Result<String, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.export_backup()
}

/// 导入备份：粘贴文本 + 该备份对应的主密码 → 解密并合并进本地
#[tauri::command]
async fn mobile_import_backup(
    text: String,
    password: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<usize, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.import_backup(&text, &password)
}

#[tauri::command]
async fn mobile_change_master(
    old_password: String,
    new_password: String,
    confirm: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    s.change_master_password(&old_password, &new_password, &confirm)?;
    persist_bio_key(&s);
    Ok(())
}

#[tauri::command]
fn sync_health(ip: String, port: u16) -> Result<sync_client::HealthResponse, String> {
    sync_client::health(&ip, port)
}

/// 配对：二维码内容（localvault://sync?..）解析 → 配对 → 持久化到加密缓存
#[tauri::command]
async fn sync_connect(
    qr_payload: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<PairingInfo, String> {
    let (ip, port, code, key) = sync_client::parse_qr_payload(&qr_payload)?;
    let pairing = sync_client::pair(&ip, port, &code, &key, "安卓手机")?;
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    data.pairing = Some(pairing.clone());
    data.sync_since = 0;
    // 配对时刻作为推送基线：之后只推本地新改动，不把刚拉取的数据全量推回
    data.pushed_since = chrono_now_ms();
    s.persist()?;
    Ok(pairing)
}

/// 拉取增量（首次全量 since=0），合并到本地缓存并持久化
#[tauri::command]
async fn sync_pull(store: State<'_, Mutex<MobileStore>>) -> Result<usize, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    // 阶段1：读取配对并发起拉取
    let (pairing, since) = {
        let data = s.data().ok_or("请先解锁手机端")?;
        (data.pairing.clone().ok_or("尚未配对桌面端")?, data.sync_since)
    };
    let resp = match sync_client::pull(&pairing, since) {
        Ok(r) => r,
        Err(e) if e.contains("403") => {
            // 配对已失效（电脑端锁定后密钥未恢复/已解除配对）：清除本地配对，需重新扫码
            if let Some(data) = s.data_mut() {
                data.pairing = None;
                data.sync_since = 0;
            }
            let _ = s.persist();
            return Err("配对已失效，请在桌面端重新生成二维码并重新配对".into());
        }
        Err(e) => return Err(e),
    };
    // 阶段2：合并并持久化
    {
        let data = s.data_mut().ok_or("请先解锁手机端")?;
        let mut merged: Vec<SyncEntry> = data.entries.clone();
        for incoming in resp.entries {
            match merged.iter_mut().find(|e| e.id == incoming.id) {
                Some(local) if local.updated_at >= incoming.updated_at => {}
                Some(local) => *local = incoming,
                None => merged.push(incoming),
            }
        }
        merged.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        // 分类合并（按名称去重）
        let mut cats = data.categories.clone();
        for cat in resp.categories {
            if !cats.iter().any(|c| c.name == cat.name) {
                cats.push(cat);
            }
        }
        data.entries = merged;
        data.categories = cats;
        data.sync_since = resp.server_time.max(since);
        // 拉取到的数据不需要推回桌面端：推进推送游标到本次拉取时间
        data.pushed_since = data.pushed_since.max(resp.server_time);
        // 保存电脑端下发的找回密码恢复材料（方案 X）
        if let Some(rm) = resp.recovery.clone() {
            data.recovery = Some(rm);
        }
    }
    let count = s.data().map(|d| d.entries.len()).unwrap_or(0);
    s.persist()?;
    Ok(count)
}

/// 断开配对：通知桌面端作废 token + 清本地配对信息
#[tauri::command]
fn sync_disconnect(store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let Some(pairing) = data.pairing.clone() else {
        return Ok(());
    };
    let _ = sync_client::unpair(&pairing);
    data.pairing = None;
    data.sync_since = 0;
    s.persist()?;
    Ok(())
}

/// 推送本地改动到桌面端（增量）：只推 updated_at > pushed_since 的条目 + 待删除 id + 分类变更
/// 成功后推进游标、清空 deleted_ids 与 pending_cat_ops；未配对/推送失败返回错误由前端提示。
#[tauri::command]
async fn sync_push(store: State<'_, Mutex<MobileStore>>) -> Result<usize, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    // 阶段1：读取配对与待推数据并发起推送
    let (pairing, entries, deleted, cat_ops) = {
        let data = s.data().ok_or("请先解锁手机端")?;
        let p = data.pairing.clone().ok_or("尚未配对桌面端")?;
        let es: Vec<SyncEntry> = data
            .entries
            .iter()
            .filter(|e| e.updated_at > data.pushed_since)
            .cloned()
            .collect();
        (p, es, data.deleted_ids.clone(), data.pending_cat_ops.clone())
    };
    let accepted = match sync_client::push(&pairing, &entries, &deleted, &cat_ops) {
        Ok(n) => n,
        Err(e) if e.contains("403") => {
            // 配对已失效：清除本地配对，需重新扫码
            if let Some(data) = s.data_mut() {
                data.pairing = None;
                data.sync_since = 0;
            }
            let _ = s.persist();
            return Err("配对已失效，请在桌面端重新生成二维码并重新配对".into());
        }
        Err(e) => return Err(e),
    };
    // 阶段2：推进游标并清空待推队列
    {
        let data = s.data_mut().ok_or("请先解锁手机端")?;
        data.pushed_since = chrono_now_ms().max(data.pushed_since);
        data.deleted_ids.clear();
        data.pending_cat_ops.clear();
    }
    s.persist()?;
    Ok(accepted)
}

/// 列出条目（解锁后；密码字段以掩码形式返回，查看密码用 mobile_entry_password）
#[tauri::command]
fn mobile_list(store: State<'_, Mutex<MobileStore>>) -> Result<Vec<serde_json::Value>, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data() else {
        return Err("请先解锁手机端".into());
    };
    Ok(list_locked(&data.entries))
}

/// 查看单条密码（前端传入 id，Rust 仅返回这一条的明文，用后即弃）
#[tauri::command]
fn mobile_entry_password(
    entry_id: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<String, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data() else {
        return Err("请先解锁手机端".into());
    };
    data.entries
        .iter()
        .find(|e| e.id == entry_id)
        .map(|e| e.password.clone())
        .ok_or("条目不存在".into())
}

/// 条目完整详情（不含密码明文，用于详情页展示与编辑回填）
#[tauri::command]
fn mobile_entry_detail(
    entry_id: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<serde_json::Value, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data() else {
        return Err("请先解锁手机端".into());
    };
    let e = data
        .entries
        .iter()
        .find(|e| e.id == entry_id)
        .ok_or_else(|| "条目不存在".to_string())?;
    Ok(serde_json::json!({
        "id": e.id, "type": e.entry_type, "name": e.name, "username": e.username,
        "email": e.email, "phone": e.phone, "nickname": e.nickname, "url": e.url,
        "notes": e.notes, "category": e.category, "favorite": e.favorite,
        "updatedAt": e.updated_at,
    }))
}

/// 新增/编辑条目（前端传完整字段；密码明文仅在编辑页短暂驻留，提交后写入加密缓存）
#[tauri::command]
fn mobile_save_entry(
    entry: SyncEntry,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let now = chrono_now_ms();
    let mut e = entry;
    if e.id.is_empty() {
        e.id = new_id();
    }
    e.updated_at = now;
    match data.entries.iter_mut().find(|x| x.id == e.id) {
        Some(existing) => *existing = e,
        None => data.entries.push(e),
    }
    data.entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let result = list_locked(&data.entries);
    s.persist()?;
    Ok(result)
}

/// 删除条目（移入本地回收站；同时记 deleted_ids 待推送桌面端删除）
#[tauri::command]
fn mobile_delete_entry(
    entry_id: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let Some(idx) = data.entries.iter().position(|e| e.id == entry_id) else {
        return Ok(());
    };
    let e = data.entries.remove(idx);
    let now = chrono_now_ms();
    data.trash.push(store::TrashItem {
        entry: e,
        deleted_at: now,
    });
    if !data.deleted_ids.contains(&entry_id) {
        data.deleted_ids.push(entry_id);
    }
    s.persist()?;
    Ok(())
}

/// 回收站列表（名称/类型/删除时间/掩码）
#[tauri::command]
fn mobile_trash_list(store: State<'_, Mutex<MobileStore>>) -> Result<Vec<serde_json::Value>, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data() else {
        return Err("请先解锁手机端".into());
    };
    let mut out: Vec<serde_json::Value> = data
        .trash
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.entry.id, "type": t.entry.entry_type, "name": t.entry.name,
                "username": t.entry.username, "category": t.entry.category,
                "deletedAt": t.deleted_at,
                "passwordMasked": if t.entry.password.is_empty() { "" } else { "••••••••" },
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b["deletedAt"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&a["deletedAt"].as_i64().unwrap_or(0))
    });
    Ok(out)
}

/// 从回收站恢复条目（回到密码库；从待删除列表移除，恢复后会随增量推送到桌面端）
#[tauri::command]
fn mobile_restore_entry(
    entry_id: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let Some(idx) = data.trash.iter().position(|t| t.entry.id == entry_id) else {
        return Err("回收站中没有该条目".into());
    };
    let t = data.trash.remove(idx);
    let mut e = t.entry;
    e.updated_at = chrono_now_ms();
    data.entries.push(e);
    data.entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    data.deleted_ids.retain(|id| id != &entry_id);
    let out = list_locked(&data.entries);
    s.persist()?;
    Ok(out)
}

/// 彻底删除回收站中一条（无法恢复；删除动作已在 deleted_ids 待推送）
#[tauri::command]
fn mobile_trash_purge(
    entry_id: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    data.trash.retain(|t| t.entry.id != entry_id);
    s.persist()?;
    Ok(())
}

/// 清空回收站（全部彻底删除）
#[tauri::command]
fn mobile_trash_clear(store: State<'_, Mutex<MobileStore>>) -> Result<(), String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    data.trash.clear();
    s.persist()?;
    Ok(())
}

/// 分类列表
#[tauri::command]
fn mobile_category_list(store: State<'_, Mutex<MobileStore>>) -> Result<Vec<serde_json::Value>, String> {
    let s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data() else {
        return Err("请先解锁手机端".into());
    };
    Ok(data
        .categories
        .iter()
        .map(|c| serde_json::json!({"name": c.name, "icon": c.icon, "parentName": c.parent_name}))
        .collect())
}

/// 新建分类（记录 create 操作待推送桌面端）
#[tauri::command]
fn mobile_category_create(
    name: String,
    icon: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("分类名称不能为空".into());
    }
    if data.categories.iter().any(|c| c.name == name) {
        return Err("分类已存在".into());
    }
    data.categories.push(store::Category {
        name: name.clone(),
        icon: if icon.trim().is_empty() { "📁".into() } else { icon },
        parent_name: None,
    });
    data.pending_cat_ops.push(store::CategoryOp {
        op: "create".into(),
        name,
        icon: data.categories.last().unwrap().icon.clone(),
        old_name: String::new(),
        parent_name: None,
    });
    let out = data
        .categories
        .iter()
        .map(|c| serde_json::json!({"name": c.name, "icon": c.icon, "parentName": c.parent_name}))
        .collect::<Vec<_>>();
    s.persist()?;
    Ok(out)
}

/// 重命名分类（同步改所有条目的分类字段，记录 update 待推送）
#[tauri::command]
fn mobile_category_rename(
    old_name: String,
    new_name: String,
    icon: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("分类名称不能为空".into());
    }
    if new_name != old_name && data.categories.iter().any(|c| c.name == new_name) {
        return Err("新分类名称已存在".into());
    }
    let Some(cat) = data.categories.iter_mut().find(|c| c.name == old_name) else {
        return Err("分类不存在".into());
    };
    let icon = if icon.trim().is_empty() { cat.icon.clone() } else { icon };
    data.pending_cat_ops.push(store::CategoryOp {
        op: "update".into(),
        name: new_name.clone(),
        icon: icon.clone(),
        old_name: old_name.clone(),
        parent_name: cat.parent_name.clone(),
    });
    cat.name = new_name.clone();
    cat.icon = icon;
    // 同步条目分类字段
    for e in data.entries.iter_mut() {
        if e.category == old_name {
            e.category = new_name.clone();
            e.updated_at = chrono_now_ms();
        }
    }
    let out = data
        .categories
        .iter()
        .map(|c| serde_json::json!({"name": c.name, "icon": c.icon, "parentName": c.parent_name}))
        .collect::<Vec<_>>();
    s.persist()?;
    Ok(out)
}

/// 删除分类（条目回到默认分类，记录 delete 待推送）
#[tauri::command]
fn mobile_category_delete(
    name: String,
    store: State<'_, Mutex<MobileStore>>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut s = store.lock().map_err(|_| "状态锁异常".to_string())?;
    let Some(data) = s.data_mut() else {
        return Err("请先解锁手机端".into());
    };
    let Some(idx) = data.categories.iter().position(|c| c.name == name) else {
        return Err("分类不存在".into());
    };
    let cat = data.categories.remove(idx);
    data.pending_cat_ops.push(store::CategoryOp {
        op: "delete".into(),
        name: cat.name.clone(),
        icon: cat.icon,
        old_name: String::new(),
        parent_name: cat.parent_name,
    });
    for e in data.entries.iter_mut() {
        if e.category == name {
            e.category = "默认".to_string();
            e.updated_at = chrono_now_ms();
        }
    }
    let out = data
        .categories
        .iter()
        .map(|c| serde_json::json!({"name": c.name, "icon": c.icon, "parentName": c.parent_name}))
        .collect::<Vec<_>>();
    s.persist()?;
    Ok(out)
}

/// 密码生成器：安全随机，保证长度/字符集要求
#[tauri::command]
fn mobile_generate_password(length: Option<u8>) -> Result<String, String> {
    let len = length.unwrap_or(16).clamp(6, 64) as usize;
    let lower: Vec<char> = "abcdefghijklmnopqrstuvwxyz".chars().collect();
    let upper: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect();
    let digits: Vec<char> = "0123456789".chars().collect();
    let symbols: Vec<char> = "!@#$%^&*()-_=+[]{};:,.<>?".chars().collect();
    // 保证至少包含小写/大写/数字各一个，再随机填充
    let mut pool: Vec<char> = Vec::new();
    let mut chars: Vec<char> = vec![
        lower[crypto::random::<1>()[0] as usize % lower.len()],
        upper[crypto::random::<1>()[0] as usize % upper.len()],
        digits[crypto::random::<1>()[0] as usize % digits.len()],
    ];
    if len >= 8 {
        chars.push(symbols[crypto::random::<1>()[0] as usize % symbols.len()]);
    }
    pool.extend_from_slice(&lower);
    pool.extend_from_slice(&upper);
    pool.extend_from_slice(&digits);
    pool.extend_from_slice(&symbols);
    while chars.len() < len {
        let b = crypto::random::<1>()[0] as usize;
        chars.push(pool[b % pool.len()]);
    }
    // 洗牌
    for i in (1..chars.len()).rev() {
        let j = crypto::random::<2>()[0] as usize % (i + 1);
        chars.swap(i, j);
    }
    Ok(chars.into_iter().collect())
}

fn list_locked(entries: &[SyncEntry]) -> Vec<serde_json::Value> {
    entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id, "type": e.entry_type, "name": e.name,
                "username": e.username, "url": e.url, "category": e.category,
                "favorite": e.favorite, "updatedAt": e.updated_at,
                "passwordMasked": if e.password.is_empty() { "" } else { "••••••••" },
            })
        })
        .collect()
}

fn new_id() -> String {
    let r = crypto::random::<8>();
    hex::encode(r)
}

fn chrono_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ---------- 入口 ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_android_diag();
    tauri::Builder::default()
        .manage(Mutex::new(MobileStore::new()))
        .invoke_handler(tauri::generate_handler![
            mobile_status,
            mobile_setup,
            mobile_unlock,
            mobile_lock,
            mobile_lock_all,
            mobile_change_master,
            mobile_biometric_available,
            mobile_biometric_unlock,
            mobile_test_biometric,
            mobile_share_text,
            mobile_install_apk,
            mobile_open_url,
            mobile_exit,
            mobile_check_update,
            mobile_download_update,
            mobile_open_biometric_settings,
            mobile_security_status,
            mobile_update_security,
            mobile_recovery_reset,
            mobile_pc_recovery_status,
            mobile_pc_recovery_reset,
            mobile_export_backup,
            mobile_import_backup,
            sync_health,
            sync_connect,
            sync_pull,
            sync_push,
            sync_disconnect,
            mobile_list,
            mobile_entry_password,
            mobile_entry_detail,
            mobile_save_entry,
            mobile_delete_entry,
            mobile_trash_list,
            mobile_restore_entry,
            mobile_trash_purge,
            mobile_trash_clear,
            mobile_category_list,
            mobile_category_create,
            mobile_category_rename,
            mobile_category_delete,
            mobile_generate_password,
        ])
        .setup(|app| {
            diag("setup 阶段：应用已启动，写入探针");
            // 强制清理 WebView 缓存：避免更新安装后仍显示旧版前端界面（tauri://localhost 响应缓存）
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.clear_all_browsing_data();
            }
            if let Ok(dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&dir);
                std::env::set_var("LOCALVAULT_MOBILE_DATA_DIR", &dir);
                diag(&format!("数据目录已设置为：{}", dir.display()));
                // MobileStore::new() 在环境变量设置前执行，路径可能回退为相对路径
                // （相对 CWD 落到只读文件系统），这里拿到真实目录后强制修正 store 路径
                if let Some(s) = app.try_state::<Mutex<MobileStore>>() {
                    let mut s = s.lock().unwrap();
                    s.set_data_dir(&dir);
                    diag(&format!("store 数据路径修正为：{}", s.path_str()));
                }
            }
            let _ = app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            diag(&format!("tauri 启动失败：{e}"));
            panic!("error while running LocalVault Mobile: {e}");
        });
}

#[cfg(target_os = "android")]
fn install_android_diag() {
    use std::io::Write;
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("LOCALVAULT_MOBILE_PANIC: {info}");
        eprintln!("{msg}");
        let mut targets = Vec::new();
        if let Ok(d) = std::env::var("LOCALVAULT_MOBILE_DATA_DIR") {
            targets.push(d);
        }
        targets.push("/data/data/com.localvault.mobile/files".to_string());
        for d in targets {
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true).open(std::path::Path::new(&d).join("lv_crash.log"))
            {
                let _ = f.write_all(msg.as_bytes());
            }
        }
    }));
}

#[cfg(not(target_os = "android"))]
fn install_android_diag() {}

fn diag(msg: &str) {
    eprintln!("LOCALVAULT_MOBILE: {msg}");
}
