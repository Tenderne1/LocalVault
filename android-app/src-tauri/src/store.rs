// LocalVault 手机端本地加密缓存
// 存储：单文件 vault.mobile = base64( salt(16) || nonce(24) || XChaCha20-Poly1305 密文 )
// 明文 JSON 结构：
//   { "entries": [SyncEntry...], "categories": [...], "syncSince": <last pull timestamp>,
//     "pairing": { "ip", "port", "token", "keyB64" } | null }
// 解锁后整个明文常驻内存（手机端自身内存 dump 风险与桌面端同量级）；锁定清空。
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::crypto::{self, KEY_LEN};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEntry {
    pub id: String,
    #[serde(default)] pub seq: i64,
    #[serde(rename = "type", default = "default_type")] pub entry_type: String,
    #[serde(default)] pub name: String,
    #[serde(default)] pub username: String,
    #[serde(default)] pub email: String,
    #[serde(default)] pub phone: String,
    #[serde(default)] pub password: String,
    #[serde(default)] pub nickname: String,
    #[serde(default)] pub url: String,
    #[serde(default)] pub notes: String,
    #[serde(default = "default_category")] pub category: String,
    #[serde(default = "empty_tags")] pub tags: Vec<String>,
    #[serde(default)] pub favorite: bool,
    #[serde(default = "zero")] pub updated_at: i64,
    #[serde(default)] pub expires_at: Option<i64>,
}

fn default_type() -> String { "网站".into() }
fn default_category() -> String { "默认".into() }
fn empty_tags() -> Vec<String> { Vec::new() }
fn zero() -> i64 { 0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub icon: String,
    #[serde(default)] pub parent_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingInfo {
    pub ip: String,
    pub port: u16,
    pub token: String,
    pub key_b64: String, // 传输密钥 base64（XChaCha20 32 字节）
}

/// 分类变更操作（推送桌面端 apply_category_ops）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryOp {
    pub op: String, // create / update / delete
    pub name: String,
    pub icon: String,
    #[serde(default)] pub old_name: String,
    #[serde(default)] pub parent_name: Option<String>,
}

/// 回收站条目（删除的完整条目 + 删除时间）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub entry: SyncEntry,
    pub deleted_at: i64,
}

/// 电脑端下发的找回密码恢复材料（方案 X）：
/// 用「电脑端 Recovery Code + 3 组密保答案」组合派生密钥尝试解密 wrapped，成功即身份通过。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryMaterial {
    pub salt_b64: String,
    pub wrapped_b64: String,
    pub questions: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    #[serde(default)] pub entries: Vec<SyncEntry>,
    #[serde(default)] pub categories: Vec<Category>,
    #[serde(default)] pub sync_since: i64,
    #[serde(default)] pub pairing: Option<PairingInfo>,
    /// 本地删除待推送的条目 id（推送成功后清空）
    #[serde(default)] pub deleted_ids: Vec<String>,
    /// 上次成功推送时间（增量推送游标：只推 updated_at > pushed_since 的条目）
    #[serde(default)] pub pushed_since: i64,
    /// 本地回收站
    #[serde(default)] pub trash: Vec<TrashItem>,
    /// 待推送的分类变更（推送成功后清空）
    #[serde(default)] pub pending_cat_ops: Vec<CategoryOp>,
    /// 电脑端下发的找回密码恢复材料（方案 X：电脑端 Recovery Code 找回）
    #[serde(default)] pub recovery: Option<RecoveryMaterial>,
}

pub struct MobileStore {
    path: PathBuf,
    /// 解锁后的内存明文缓存
    data: Option<VaultData>,
    /// 会话密钥（主密码派生，仅内存驻留）
    key: Option<[u8; KEY_LEN]>,
    /// 指纹解锁缓存密钥（锁定后保留在内存；立即锁定/App 退出即清空）
    bio_key: Option<[u8; KEY_LEN]>,
}

/// 密保文件（独立于主密钥存储，供忘记主密码时验证身份后重置）
/// 文件：vault.mobile.security（明文 JSON，仅存哈希与密保问题，不存答案原文）
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityFile {
    pub recovery_salt_b64: String,
    pub recovery_hash_b64: String,
    pub questions: Vec<String>,
    pub answer_salts_b64: Vec<String>,
    pub answer_hashes_b64: Vec<String>,
}

impl MobileStore {
    pub fn new() -> Self {
        let path = dirs_data_dir().join("LocalVaultMobile").join("vault.mobile");
        Self { path, data: None, key: None, bio_key: None }
    }

    /// Android 上由 lib.rs 的 setup() 在拿到真实 app 数据目录后调用。
    /// 注意：MobileStore::new() 在 tauri setup 之前执行，此时环境变量尚未设置，
    /// 路径可能回退为相对路径（相对 CWD 落到只读目录），必须在此强制修正。
    pub fn set_data_dir(&mut self, dir: &std::path::Path) {
        self.path = dir.join("LocalVaultMobile").join("vault.mobile");
    }

    pub fn path_str(&self) -> String {
        self.path.display().to_string()
    }

    pub fn has_master(&self) -> bool {
        self.path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.data.is_some()
    }

    /// 首次设置主密码并初始化（不要求密码规则之外的额外条件）
    pub fn setup(&mut self, password: &str, confirm: &str) -> Result<(), String> {
        crypto::validate_new_master_password(password)?;
        if password != confirm {
            return Err("两次输入的主密码不一致".into());
        }
        if self.has_master() {
            return Err("手机端已设置主密码，请直接解锁".into());
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
        }
        let salt = crypto::random::<16>();
        let key = crypto::derive(password, &salt)?;
        let empty = VaultData::default();
        let json = serde_json::to_vec(&empty).map_err(|e| e.to_string())?;
        let enc = crypto::encrypt(&key, &json, b"vault.mobile")?;
        let mut blob = salt.to_vec();
        blob.extend_from_slice(&enc);
        std::fs::write(&self.path, base64::engine::general_purpose::STANDARD.encode(&blob))
            .map_err(|e| format!("写入缓存失败：{e}"))?;
        self.key = Some(key);
        self.data = Some(empty);
        self.bio_key = Some(key);
        Ok(())
    }

    /// 解锁：主密码 → Argon2id → 解密缓存
    pub fn unlock(&mut self, password: &str) -> Result<(), String> {
        if !self.has_master() {
            return Err("手机端尚未设置主密码".into());
        }
        let raw = std::fs::read(&self.path).map_err(|e| format!("读取缓存失败：{e}"))?;
        let blob = base64::engine::general_purpose::STANDARD
            .decode(&raw)
            .map_err(|_| "缓存数据损坏".to_string())?;
        if blob.len() < 16 + 24 + 16 {
            return Err("缓存数据不完整".into());
        }
        let (salt, enc) = blob.split_at(16);
        let salt_arr: [u8; 16] = salt.try_into().map_err(|_| "缓存数据损坏".to_string())?;
        let key = crypto::derive(password, &salt_arr)?;
        let plain = crypto::decrypt(&key, enc, b"vault.mobile")?;
        let data: VaultData =
            serde_json::from_slice(&plain).map_err(|_| "缓存解析失败，可能是主密码错误".to_string())?;
        self.key = Some(key);
        self.data = Some(data);
        self.bio_key = Some(key);
        Ok(())
    }

    /// 锁定（保留指纹缓存密钥：界面锁定后仍可用指纹解锁）
    pub fn lock_to_bio(&mut self) {
        if let Some(mut k) = self.key.take() {
            crypto::zeroize_key(&mut k);
        }
        self.data = None;
    }

    /// 立即锁定（清空指纹缓存：必须输入主密码才能再次解锁）
    pub fn lock_all(&mut self) {
        self.lock_to_bio();
        if let Some(mut k) = self.bio_key.take() {
            crypto::zeroize_key(&mut k);
        }
    }

    pub fn can_bio_unlock(&self) -> bool {
        self.bio_key.is_some()
    }

    /// 当前指纹会话密钥（供持久化到 Keystore 加密文件）
    pub fn current_bio_key(&self) -> Option<&[u8; KEY_LEN]> {
        self.bio_key.as_ref()
    }

    /// 从持久化（Keystore 加密文件）恢复指纹会话密钥，供冷启动后指纹解锁
    pub fn restore_bio_key(&mut self, key: [u8; KEY_LEN]) {
        self.bio_key = Some(key);
    }

    /// 指纹解锁：用内存中缓存的会话密钥直接解密（不重新派生主密码）
    pub fn unlock_with_bio(&mut self) -> Result<(), String> {
        let Some(bk) = self.bio_key else {
            return Err("当前没有可用的指纹会话，请使用主密码解锁".into());
        };
        if !self.has_master() {
            return Err("手机端尚未设置主密码".into());
        }
        let raw = std::fs::read(&self.path).map_err(|e| format!("读取缓存失败：{e}"))?;
        let blob = base64::engine::general_purpose::STANDARD
            .decode(&raw)
            .map_err(|_| "缓存数据损坏".to_string())?;
        if blob.len() < 16 + 24 + 16 {
            return Err("缓存数据不完整".into());
        }
        let (_, enc) = blob.split_at(16);
        let plain = crypto::decrypt(&bk, enc, b"vault.mobile")?;
        let data: VaultData =
            serde_json::from_slice(&plain).map_err(|_| "缓存解析失败".to_string())?;
        self.key = Some(bk);
        self.data = Some(data);
        Ok(())
    }

    /// 修改主密码（重新派生密钥并重写缓存）
    pub fn change_master_password(&mut self, old: &str, new: &str, confirm: &str) -> Result<(), String> {
        crypto::validate_new_master_password(new)?;
        if new != confirm {
            return Err("两次输入的新主密码不一致".into());
        }
        if !self.has_master() {
            return Err("手机端尚未设置主密码".into());
        }
        // 先验证旧密码
        self.unlock(old)?;
        let data = self.data.clone().unwrap_or_default();
        let salt = crypto::random::<16>();
        let key = crypto::derive(new, &salt)?;
        let json = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
        let enc = crypto::encrypt(&key, &json, b"vault.mobile")?;
        let mut blob = salt.to_vec();
        blob.extend_from_slice(&enc);
        std::fs::write(&self.path, base64::engine::general_purpose::STANDARD.encode(&blob))
            .map_err(|e| format!("写入缓存失败：{e}"))?;
        if let Some(mut k) = self.key.take() {
            crypto::zeroize_key(&mut k);
        }
        self.key = Some(key);
        self.data = Some(data);
        self.bio_key = Some(key);
        Ok(())
    }

    pub fn data(&self) -> Option<&VaultData> {
        self.data.as_ref()
    }

    pub fn data_mut(&mut self) -> Option<&mut VaultData> {
        self.data.as_mut()
    }

    /// 持久化当前明文缓存（写回加密文件）
    pub fn persist(&self) -> Result<(), String> {
        let (Some(key), Some(data)) = (&self.key, &self.data) else {
            return Err("手机端未解锁".into());
        };
        let salt = read_salt(&self.path)?;
        let json = serde_json::to_vec(data).map_err(|e| e.to_string())?;
        let enc = crypto::encrypt(key, &json, b"vault.mobile")?;
        let mut blob = salt.to_vec();
        blob.extend_from_slice(&enc);
        std::fs::write(&self.path, base64::engine::general_purpose::STANDARD.encode(&blob))
            .map_err(|e| format!("写入缓存失败：{e}"))?;
        Ok(())
    }

    // ---------- 密保（忘记主密码时验证身份） ----------

    fn security_path(&self) -> PathBuf {
        self.path.with_file_name("vault.mobile.security")
    }

    pub fn has_security(&self) -> bool {
        self.security_path().exists()
    }

    /// 保存/更新密保：3 组问题 + 答案，自动生成新的 Recovery Code 并返回
    pub fn save_security(
        &mut self,
        questions: Vec<String>,
        answers: Vec<String>,
    ) -> Result<String, String> {
        if questions.len() != 3 || answers.len() != 3 {
            return Err("需要 3 组密保问题和答案".into());
        }
        let qs: Vec<String> = questions.iter().map(|q| q.trim().to_string()).collect();
        let ans: Vec<String> = answers.iter().map(|a| a.trim().to_string()).collect();
        if qs.iter().any(|q| q.is_empty()) {
            return Err("每个密保问题不能为空".into());
        }
        if ans.iter().any(|a| a.is_empty()) {
            return Err("每个密保答案不能为空".into());
        }
        let recovery_code = gen_recovery_code();
        let rec_salt = crypto::random::<16>();
        let rec_hash = crypto::derive(&recovery_code, &rec_salt)?;
        let mut ans_salts = Vec::new();
        let mut ans_hashes = Vec::new();
        for a in &ans {
            let salt = crypto::random::<16>();
            let h = crypto::derive(a, &salt)?;
            ans_salts.push(base64::engine::general_purpose::STANDARD.encode(salt));
            ans_hashes.push(base64::engine::general_purpose::STANDARD.encode(h));
        }
        let sf = SecurityFile {
            recovery_salt_b64: base64::engine::general_purpose::STANDARD.encode(rec_salt),
            recovery_hash_b64: base64::engine::general_purpose::STANDARD.encode(rec_hash),
            questions: qs,
            answer_salts_b64: ans_salts,
            answer_hashes_b64: ans_hashes,
        };
        let json = serde_json::to_vec(&sf).map_err(|e| e.to_string())?;
        if let Some(dir) = self.security_path().parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
        }
        std::fs::write(self.security_path(), json).map_err(|e| format!("写入密保文件失败：{e}"))?;
        Ok(recovery_code)
    }

    fn load_security(&self) -> Result<SecurityFile, String> {
        let raw = std::fs::read(self.security_path()).map_err(|_| "未设置密保".to_string())?;
        serde_json::from_slice(&raw).map_err(|_| "密保文件损坏".to_string())
    }

    fn verify_answer(&self, sf: &SecurityFile, idx: usize, answer: &str) -> bool {
        if idx >= sf.questions.len() || idx >= sf.answer_salts_b64.len() || idx >= sf.answer_hashes_b64.len() {
            return false;
        }
        let Ok(salt_bytes) = base64::engine::general_purpose::STANDARD.decode(&sf.answer_salts_b64[idx]) else {
            return false;
        };
        let Ok(salt) = <[u8; 16]>::try_from(salt_bytes.as_slice()) else { return false };
        let Ok(h) = crypto::derive(answer.trim(), &salt) else { return false };
        let Ok(want) = base64::engine::general_purpose::STANDARD.decode(&sf.answer_hashes_b64[idx]) else {
            return false;
        };
        h.as_slice() == want.as_slice()
    }

    /// 校验 Recovery Code + 三组密保答案（用于忘记主密码后的身份验证）
    pub fn verify_security(&self, recovery_code: &str, answers: &[String]) -> Result<(), String> {
        if !self.has_security() {
            return Err("尚未设置密保，无法找回主密码".into());
        }
        let sf = self.load_security()?;
        let Ok(salt_bytes) = base64::engine::general_purpose::STANDARD.decode(&sf.recovery_salt_b64) else {
            return Err("密保文件损坏".into());
        };
        let Ok(salt) = <[u8; 16]>::try_from(salt_bytes.as_slice()) else { return Err("密保文件损坏".into()) };
        let Ok(h) = crypto::derive(recovery_code.trim(), &salt) else { return Err("校验失败".into()) };
        let Ok(want) = base64::engine::general_purpose::STANDARD.decode(&sf.recovery_hash_b64) else {
            return Err("密保文件损坏".into());
        };
        if h.as_slice() != want.as_slice() {
            return Err("Recovery Code 不正确".into());
        }
        if answers.len() != 3 || !(0..3).all(|i| self.verify_answer(&sf, i, &answers[i])) {
            return Err("密保答案不正确".into());
        }
        Ok(())
    }

    /// 忘记主密码：验证通过后清空本地数据（旧密文无法解密）并设置新主密码
    pub fn recovery_reset(
        &mut self,
        recovery_code: &str,
        answers: &[String],
        new_password: &str,
        confirm: &str,
    ) -> Result<(), String> {
        self.verify_security(recovery_code, answers)?;
        self.reset_with_new_master(new_password, confirm)
    }

    /// 电脑端恢复材料是否可用（同步拉取过且电脑端设置了密保）
    pub fn has_pc_recovery(&self) -> bool {
        self.data.as_ref().and_then(|d| d.recovery.as_ref()).is_some()
    }

    /// 电脑端密保问题（用于提示用户）
    pub fn pc_recovery_questions(&self) -> Option<Vec<String>> {
        self.data.as_ref().and_then(|d| d.recovery.as_ref()).map(|r| r.questions.clone())
    }

    /// 验证电脑端 Recovery Code + 3 组密保答案（与桌面端完全一致：combo 派生密钥解密 recovery_wrapped）
    pub fn verify_pc_recovery(&self, recovery_code: &str, answers: &[String]) -> Result<(), String> {
        let Some(rm) = self.data.as_ref().and_then(|d| d.recovery.clone()) else {
            return Err("尚未获取电脑端恢复材料，请先与电脑端同步一次".into());
        };
        if answers.len() != 3 {
            return Err("需要 3 组密保答案".into());
        }
        let salt_bytes = base64::engine::general_purpose::STANDARD
            .decode(&rm.salt_b64)
            .map_err(|_| "恢复材料损坏".to_string())?;
        let salt: [u8; 16] = salt_bytes.try_into().map_err(|_| "恢复材料损坏".to_string())?;
        let wrapped = base64::engine::general_purpose::STANDARD
            .decode(&rm.wrapped_b64)
            .map_err(|_| "恢复材料损坏".to_string())?;
        let combo = format!(
            "{}\0{}\0{}\0{}",
            recovery_code.trim(),
            answers[0].trim(),
            answers[1].trim(),
            answers[2].trim()
        );
        let mut rk = crypto::derive(&combo, &salt)?;
        let r = crypto::decrypt(&rk, &wrapped, b"LocalVault|recovery|v1")
            .map(|_| ())
            .map_err(|_| "Recovery Code 或密保答案不正确".to_string());
        crypto::zeroize_key(&mut rk);
        r
    }

    /// 用电脑端 Recovery Code 找回：身份验证通过后重置本地数据并设置新主密码
    pub fn pc_recovery_reset(
        &mut self,
        recovery_code: &str,
        answers: &[String],
        new_password: &str,
        confirm: &str,
    ) -> Result<(), String> {
        self.verify_pc_recovery(recovery_code, answers)?;
        self.reset_with_new_master(new_password, confirm)
    }

    /// 身份验证通过后的通用重置：清空本地数据（旧密文无法解密）→ 新主密码重新加密
    fn reset_with_new_master(&mut self, new_password: &str, confirm: &str) -> Result<(), String> {
        crypto::validate_new_master_password(new_password)?;
        if new_password != confirm {
            return Err("两次输入的新主密码不一致".into());
        }
        self.lock_all();
        let salt = crypto::random::<16>();
        let key = crypto::derive(new_password, &salt)?;
        let empty = VaultData::default();
        let json = serde_json::to_vec(&empty).map_err(|e| e.to_string())?;
        let enc = crypto::encrypt(&key, &json, b"vault.mobile")?;
        let mut blob = salt.to_vec();
        blob.extend_from_slice(&enc);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建数据目录失败：{e}"))?;
        }
        std::fs::write(&self.path, base64::engine::general_purpose::STANDARD.encode(&blob))
            .map_err(|e| format!("写入缓存失败：{e}"))?;
        self.key = Some(key);
        self.data = Some(empty);
        self.bio_key = Some(key);
        Ok(())
    }

    // ---------- 加密备份（导出/导入） ----------

    /// 导出：用当前主密钥加密的完整数据（不含主密码），返回 base64 文本
    pub fn export_backup(&self) -> Result<String, String> {
        let (Some(key), Some(data)) = (&self.key, &self.data) else {
            return Err("手机端未解锁".into());
        };
        let json = serde_json::to_vec(data).map_err(|e| e.to_string())?;
        let enc = crypto::encrypt(key, &json, b"backup")?;
        let mut blob = enc.to_vec(); // nonce||ct
        let salt = read_salt(&self.path)?;
        let mut header = salt.to_vec();
        header.append(&mut blob);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&header);
        let obj = serde_json::json!({ "v": 1, "aad": "localvault-backup", "data": b64 });
        serde_json::to_string(&obj).map_err(|e| e.to_string())
    }

    /// 导入：输入备份文本 + 该备份对应的主密码，解密并合并进本地（条目按 updated_at 新者胜、分类按名去重）
    pub fn import_backup(&mut self, text: &str, password: &str) -> Result<usize, String> {
        let obj: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "备份文件格式不正确".to_string())?;
        let data_b64 = obj
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or("备份文件缺少数据")?;
        let blob = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .map_err(|_| "备份数据损坏".to_string())?;
        if blob.len() < 16 + 24 + 16 {
            return Err("备份数据不完整".into());
        }
        let (salt_bytes, enc) = blob.split_at(16);
        let salt: [u8; 16] = salt_bytes.try_into().map_err(|_| "备份数据损坏".to_string())?;
        let key = crypto::derive(password, &salt)?;
        let plain = crypto::decrypt(&key, enc, b"backup")?;
        let incoming: VaultData =
            serde_json::from_slice(&plain).map_err(|_| "备份解密失败：主密码不正确或备份已损坏".to_string())?;
        let Some(data) = self.data_mut() else {
            return Err("手机端未解锁".into());
        };
        for e in incoming.entries {
            match data.entries.iter_mut().find(|x| x.id == e.id) {
                Some(local) if local.updated_at >= e.updated_at => {}
                Some(local) => *local = e,
                None => data.entries.push(e),
            }
        }
        for c in incoming.categories {
            if !data.categories.iter().any(|x| x.name == c.name) {
                data.categories.push(c);
            }
        }
        let count = data.entries.len();
        self.persist()?;
        Ok(count)
    }
}

/// 生成 18 位随机 Recovery Code（避免易混淆字符）
fn gen_recovery_code() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let r = crypto::random::<18>();
    r.iter().map(|b| CHARS[(b % CHARS.len() as u8) as usize] as char).collect()
}

fn read_salt(path: &std::path::Path) -> Result<[u8; 16], String> {
    let raw = std::fs::read(path).map_err(|e| format!("读取缓存失败：{e}"))?;
    let blob = base64::engine::general_purpose::STANDARD
        .decode(&raw)
        .map_err(|_| "缓存数据损坏".to_string())?;
    if blob.len() < 16 {
        return Err("缓存数据不完整".into());
    }
    let salt: [u8; 16] = blob[..16].try_into().map_err(|_| "缓存数据损坏".to_string())?;
    Ok(salt)
}

#[cfg(target_os = "android")]
fn dirs_data_dir() -> PathBuf {
    // Tauri Android 下由 tauri::Manager 提供应用数据目录；这里使用 app 私有目录。
    // 由于 store 是独立模块，运行时通过 AppHandle 的 path resolver 注入更可靠，
    // 这里提供一个可被 main.rs 覆盖的默认路径（main.rs 会调用 set_data_dir）。
    std::env::var("LOCALVAULT_MOBILE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("LocalVaultMobile"))
}

#[cfg(not(target_os = "android"))]
fn dirs_data_dir() -> PathBuf {
    std::env::var("LOCALVAULT_MOBILE_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("LocalVaultMobile"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> (MobileStore, PathBuf) {
        let r = crypto::random::<4>();
        let n = u32::from_be_bytes(r);
        let d = std::env::temp_dir().join(format!("lv_mobile_test_{}", n));
        let _ = std::fs::create_dir_all(&d);
        std::env::set_var("LOCALVAULT_MOBILE_DATA_DIR", &d);
        (MobileStore::new(), d)
    }

    #[test]
    fn setup_unlock_roundtrip() {
        let (mut s, d) = tmp_store();
        let p = s.path.clone();
        s.setup("Abcdef12!", "Abcdef12!").unwrap();
        assert!(s.has_master());
        assert!(s.is_unlocked());

        // 锁定再解锁
        s.lock_all();
        assert!(!s.is_unlocked());
        assert!(s.unlock("wrong123!").is_err());
        s.unlock("Abcdef12!").unwrap();
        assert!(s.is_unlocked());

        // 写入一条数据后持久化，再解锁仍存在
        let data = s.data_mut().unwrap();
        data.entries.push(SyncEntry {
            id: "e1".into(), seq: 0, entry_type: "网站".into(), name: "GitHub".into(),
            username: "u".into(), email: "".into(), phone: "".into(), password: "p".into(),
            nickname: "".into(), url: "https://github.com".into(), notes: "".into(),
            category: "默认".into(), tags: vec![], favorite: false, updated_at: 100,
            expires_at: None,
        });
        s.persist().unwrap();
        s.lock_all();
        s.unlock("Abcdef12!").unwrap();
        assert_eq!(s.data().unwrap().entries.len(), 1);
        assert_eq!(s.data().unwrap().entries[0].name, "GitHub");

        // 指纹会话：锁定保留 bio_key，可用 unlock_with_bio 直接解锁
        s.lock_to_bio();
        assert!(!s.is_unlocked());
        assert!(s.can_bio_unlock());
        s.unlock_with_bio().unwrap();
        assert!(s.is_unlocked());
        assert_eq!(s.data().unwrap().entries[0].name, "GitHub");
        // 立即锁定清除 bio_key
        s.lock_all();
        assert!(!s.can_bio_unlock());

        let _ = std::fs::remove_dir_all(d);
        let _ = p;
    }

    #[test]
    fn change_master_password() {
        let (mut s, d) = tmp_store();
        let p = s.path.clone();
        s.setup("Abcdef12!", "Abcdef12!").unwrap();
        s.change_master_password("Abcdef12!", "Xyz7890!", "Xyz7890!").unwrap();
        s.lock_all();
        assert!(s.unlock("Abcdef12!").is_err());
        s.unlock("Xyz7890!").unwrap();
        assert!(s.is_unlocked());
        let _ = std::fs::remove_dir_all(d);
        let _ = p;
    }

    #[test]
    fn security_and_backup() {
        let (mut s, d) = tmp_store();
        let p = s.path.clone();
        s.setup("Abcdef12!", "Abcdef12!").unwrap();
        let data = s.data_mut().unwrap();
        data.entries.push(SyncEntry {
            id: "e1".into(), seq: 0, entry_type: "网站".into(), name: "GitHub".into(),
            username: "u".into(), email: "".into(), phone: "".into(), password: "p".into(),
            nickname: "".into(), url: "https://github.com".into(), notes: "".into(),
            category: "默认".into(), tags: vec![], favorite: false, updated_at: 100,
            expires_at: None,
        });
        s.persist().unwrap();

        // 密保：保存后校验，错误答案失败
        let code = s.save_security(
            vec!["Q1".into(), "Q2".into(), "Q3".into()],
            vec!["A1".into(), "A2".into(), "A3".into()],
        ).unwrap();
        assert_eq!(code.len(), 18);
        assert!(s.has_security());
        assert!(s.verify_security(&code, &["A1".into(), "A2".into(), "A3".into()]).is_ok());
        assert!(s.verify_security(&code, &["X1".into(), "A2".into(), "A3".into()]).is_err());
        assert!(s.verify_security("WRONGCODE", &["A1".into(), "A2".into(), "A3".into()]).is_err());

        // 忘记密码：验证通过 → 数据清空 + 新主密码
        s.recovery_reset(&code, &["A1".into(), "A2".into(), "A3".into()], "NewPass123!", "NewPass123!").unwrap();
        assert_eq!(s.data().unwrap().entries.len(), 0);
        s.lock_all();
        s.unlock("NewPass123!").unwrap();
        assert!(s.is_unlocked());

        // 备份：导出 → 重置/导入恢复
        s.data_mut().unwrap().entries.push(SyncEntry {
            id: "e2".into(), seq: 0, entry_type: "网站".into(), name: "Aliyun".into(),
            username: "u".into(), email: "".into(), phone: "".into(), password: "p".into(),
            nickname: "".into(), url: "https://aliyun.com".into(), notes: "".into(),
            category: "默认".into(), tags: vec![], favorite: false, updated_at: 200,
            expires_at: None,
        });
        s.persist().unwrap();
        let backup = s.export_backup().unwrap();
        let cnt = s.import_backup(&backup, "NewPass123!").unwrap();
        assert!(cnt >= 1);
        let names: Vec<String> = s.data().unwrap().entries.iter().map(|e| e.name.clone()).collect();
        assert!(names.contains(&"Aliyun".to_string()));
        assert!(s.import_backup(&backup, "WrongPass1!").is_err());

        let _ = std::fs::remove_dir_all(d);
        let _ = p;
    }
}
