// LocalVault 手机端加密核心（v1.9.5）
// 与桌面端完全一致的算法：Argon2id(128MB,3,2) + XChaCha20-Poly1305
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, OsRng, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use rand_core::RngCore;
use zeroize::Zeroize;

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const ARGON_MEM_KIB: u32 = 131_072;
pub const ARGON_ITERS: u32 = 3;
pub const ARGON_LANES: u32 = 2;
pub const MIN_MASTER_PASSWORD_LEN: usize = 8;

pub fn random<const N: usize>() -> [u8; N] {
    let mut x = [0u8; N];
    OsRng.fill_bytes(&mut x);
    x
}

/// Argon2id 密钥派生（与桌面端参数一致，保证同一主密码可互操作）
pub fn derive(password: &str, salt: &[u8; SALT_LEN]) -> Result<[u8; KEY_LEN], String> {
    let p = Params::new(ARGON_MEM_KIB, ARGON_ITERS, ARGON_LANES, Some(32))
        .map_err(|e| format!("Argon2 参数错误：{e}"))?;
    let a = Argon2::new(Algorithm::Argon2id, Version::V0x13, p);
    let mut out = [0u8; KEY_LEN];
    a.hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| format!("密钥派生失败：{e}"))?;
    Ok(out)
}

/// XChaCha20-Poly1305 加密：输出 nonce(24) || ciphertext
pub fn encrypt(key: &[u8; KEY_LEN], plain: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = random::<NONCE_LEN>();
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| "加密失败".to_string())?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn decrypt(key: &[u8; KEY_LEN], data: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < NONCE_LEN + 16 {
        return Err("密文不完整".into());
    }
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(
            XNonce::from_slice(&data[..NONCE_LEN]),
            Payload { msg: &data[NONCE_LEN..], aad },
        )
        .map_err(|_| "解密失败或校验不通过".into())
}

/// 主密码规则（与桌面端一致：≥8 位，含数字、小写、大写、特殊符号）
pub fn validate_new_master_password(pass: &str) -> Result<(), String> {
    if pass.chars().count() < MIN_MASTER_PASSWORD_LEN {
        return Err("主密码至少 8 位".into());
    }
    if !pass.chars().any(|c| c.is_ascii_digit()) {
        return Err("主密码必须包含数字".into());
    }
    if !pass.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("主密码必须包含小写字母".into());
    }
    if !pass.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("主密码必须包含大写字母".into());
    }
    if !pass.chars().any(|c| !c.is_ascii_alphanumeric()) {
        return Err("主密码必须包含特殊符号".into());
    }
    Ok(())
}

pub fn zeroize_key(k: &mut [u8; KEY_LEN]) {
    k.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_and_encrypt_roundtrip() {
        let salt = random::<SALT_LEN>();
        let key = derive("Test@1234", &salt).unwrap();
        let plain = b"{\"entries\":[]}";
        let enc = encrypt(&key, plain, b"vault.mobile").unwrap();
        let dec = decrypt(&key, &enc, b"vault.mobile").unwrap();
        assert_eq!(dec, plain);
        // 错误 AAD 必须失败
        assert!(decrypt(&key, &enc, b"other").is_err());
        // 错误密钥必须失败
        let key2 = derive("Test@5678", &salt).unwrap();
        assert!(decrypt(&key2, &enc, b"vault.mobile").is_err());
    }

    #[test]
    fn master_password_rule() {
        assert!(validate_new_master_password("abc12345").is_err()); // 无大写/特殊
        assert!(validate_new_master_password("Abcdef12!").is_ok());
        assert!(validate_new_master_password("short1!").is_err()); // 少于 8 位
    }
}
