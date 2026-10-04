// LocalVault 手机端局域网同步客户端
// 对接桌面端 LocalVault v1.9.4 的 /api/sync/* 服务（端口 38528）。
// 协议：
//   - 所有 POST 请求体 = base64(nonce24 || XChaCha20-Poly1305(key, nonce, plaintext, aad=path))
//   - 响应 = { "data": "<base64 密文>" }，用同一 key 解密（aad=path）
//   - key 来自配对二维码 localvault://sync?ip=..&port=..&code=..&key=<base64>
//   - 配对成功后 token 持久化；pull 带 token + since 增量拉取
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::store::{Category, CategoryOp, PairingInfo, RecoveryMaterial, SyncEntry};

const KEY_LEN: usize = 32;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairResponse {
    pub token: String,
    pub device: SyncDevice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncDevice {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullResponse {
    #[serde(default)] pub entries: Vec<SyncEntry>,
    #[serde(default)] pub categories: Vec<Category>,
    #[serde(default)] pub server_time: i64,
    /// 电脑端下发的找回密码恢复材料（未设置密保时为 null）
    #[serde(default)] pub recovery: Option<RecoveryMaterial>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub running: bool,
    pub port: u16,
    pub vault_unlocked: bool,
    pub paired_count: usize,
    pub ip: Option<String>,
}

fn b64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| "base64 解码失败".to_string())
}

/// 加密请求体：plaintext(json) → base64( nonce24 || ct )
fn encrypt_body(key: &[u8; KEY_LEN], path: &str, plain: &serde_json::Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(plain).map_err(|e| e.to_string())?;
    let enc = crypto::encrypt(key, &bytes, path.as_bytes())?;
    Ok(b64_encode(&enc))
}

fn decrypt_body(key: &[u8; KEY_LEN], path: &str, body: &[u8]) -> Result<serde_json::Value, String> {
    let dec = crypto::decrypt(key, body, path.as_bytes())?;
    serde_json::from_slice(&dec).map_err(|_| "响应解析失败".to_string())
}

/// 把二维码 key（base64）解析为 32 字节传输密钥
pub fn parse_qr_key(key_b64: &str) -> Result<[u8; KEY_LEN], String> {
    let raw = b64_decode(key_b64.trim())?;
    if raw.len() != KEY_LEN {
        return Err("传输密钥长度不正确".into());
    }
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(&raw);
    Ok(k)
}

/// 桌面端前端生成二维码时对 key 做了 encodeURIComponent（%2B/%2F/%3D 等），
/// 这里做百分号解码还原原始 base64（未编码的输入原样通过）。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 解析桌面端二维码内容：
/// 1) localvault://sync?ip=..&port=..&code=..&key=..（原生/手动粘贴）
/// 2) http://ip:port/mobile-qr?c=配对码&k=key（微信引导页二维码，扫码后 App 直接解析）
pub fn parse_qr_payload(payload: &str) -> Result<(String, u16, String, [u8; KEY_LEN]), String> {
    let mut ip = String::new();
    let mut port: u16 = 38528;
    let mut code = String::new();
    let mut key_b64 = String::new();
    // http 引导页格式：http://ip:port/mobile-qr?c=..&k=..
    if payload.contains("mobile-qr?") {
        let (_, rest) = payload
            .split_once("mobile-qr?")
            .ok_or("二维码内容不完整")?;
        let host_start = payload.find("://").map(|i| i + 3).unwrap_or(0);
        let host_end = payload[host_start..]
            .find('/')
            .map(|i| host_start + i)
            .unwrap_or(payload.len());
        let host = &payload[host_start..host_end];
        match host.rsplit_once(':') {
            Some((h, p)) => {
                ip = h.to_string();
                port = p.parse().unwrap_or(38528);
            }
            None => ip = host.to_string(),
        }
        for kv in rest.split('&') {
            let mut it = kv.splitn(2, '=');
            let (k, v) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            match k {
                "c" => code = v.to_string(),
                "k" => key_b64 = percent_decode(v),
                _ => {}
            }
        }
        if code.len() != 6 {
            return Err("二维码内容不完整".into());
        }
        let key = parse_qr_key(&key_b64)?;
        return Ok((ip, port, code, key));
    }
    // localvault:// 格式
    let rest = payload
        .strip_prefix("localvault://sync?")
        .or_else(|| payload.strip_prefix("localvault://pair?"))
        .ok_or("不是 LocalVault 配对二维码")?;
    for kv in rest.split('&') {
        let mut it = kv.splitn(2, '=');
        let (k, v) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
        match k {
            "ip" => ip = v.to_string(),
            "port" => port = v.parse().unwrap_or(38528),
            "code" => code = v.to_string(),
            "key" => key_b64 = percent_decode(v),
            _ => {}
        }
    }
    if ip.is_empty() || code.len() != 6 {
        return Err("二维码内容不完整".into());
    }
    let key = parse_qr_key(&key_b64)?;
    Ok((ip, port, code, key))
}

/// 探测桌面端同步服务
pub fn health(ip: &str, port: u16) -> Result<HealthResponse, String> {
    let url = format!("http://{ip}:{port}/api/sync/health");
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .call()
        .map_err(|e| format!("无法连接桌面端（{e}）"))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| format!("响应解析失败（{e}）"))?;
    serde_json::from_value(json).map_err(|e| format!("响应格式异常（{e}）"))
}

/// 配对：6 位配对码换 token（传输密钥来自二维码，配对请求用它加密）
pub fn pair(
    ip: &str,
    port: u16,
    code: &str,
    key: &[u8; KEY_LEN],
    device_name: &str,
) -> Result<PairingInfo, String> {
    let url = format!("http://{ip}:{port}/api/sync/pair");
    let req_body = encrypt_body(
        key,
        "/api/sync/pair",
        &serde_json::json!({ "code": code, "name": device_name }),
    )?;
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send_string(&req_body)
        .map_err(|e| format!("配对请求失败（{e}）"))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("配对响应解析失败（{e}）"))?;
    if status != 200 {
        let msg = body.get("error").and_then(|v| v.as_str()).unwrap_or("配对失败");
        return Err(msg.to_string());
    }
    let data = body
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or("配对响应缺少数据")?;
    let plain = decrypt_body(key, "/api/sync/pair", &b64_decode(data)?)?;
    let pr: PairResponse =
        serde_json::from_value(plain).map_err(|_| "配对响应格式异常".to_string())?;
    Ok(PairingInfo {
        ip: ip.to_string(),
        port,
        token: pr.token,
        key_b64: b64_encode(key),
    })
}

/// 增量拉取：since 之后变更的条目与分类
pub fn pull(
    pairing: &PairingInfo,
    since: i64,
) -> Result<PullResponse, String> {
    let key = parse_qr_key(&pairing.key_b64)?;
    let url = format!("http://{}:{}/api/sync/pull", pairing.ip, pairing.port);
    let req_body = encrypt_body(
        &key,
        "/api/sync/pull",
        &serde_json::json!({ "token": pairing.token, "since": since }),
    )?;
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send_string(&req_body)
        .map_err(|e| format!("拉取请求失败（{e}）"))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("拉取响应解析失败（{e}）"))?;
    if status != 200 {
        let msg = body.get("error").and_then(|v| v.as_str()).unwrap_or("拉取失败");
        return Err(msg.to_string());
    }
    let data = body.get("data").and_then(|v| v.as_str()).ok_or("拉取响应缺少数据")?;
    let plain = decrypt_body(&key, "/api/sync/pull", &b64_decode(data)?)?;
    serde_json::from_value(plain).map_err(|_| "拉取响应格式异常".to_string())
}

/// 推送本地改动到桌面端：新增/更新条目 + 待删除 id + 分类变更，返回桌面端接受条数
/// 桌面端按 updated_at 新者胜合并，幂等安全；调用方应只传增量（updated_at > 上次推送时间）。
pub fn push(
    pairing: &PairingInfo,
    entries: &[SyncEntry],
    deleted_ids: &[String],
    category_ops: &[CategoryOp],
) -> Result<usize, String> {
    let key = parse_qr_key(&pairing.key_b64)?;
    let url = format!("http://{}:{}/api/sync/push", pairing.ip, pairing.port);
    let req_body = encrypt_body(
        &key,
        "/api/sync/push",
        &serde_json::json!({
            "token": pairing.token,
            "entries": entries,
            "deleted_ids": deleted_ids,
            "category_ops": category_ops,
        }),
    )?;
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send_string(&req_body)
        .map_err(|e| format!("推送请求失败（{e}）"))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("推送响应解析失败（{e}）"))?;
    if status != 200 {
        let msg = body.get("error").and_then(|v| v.as_str()).unwrap_or("推送失败");
        return Err(msg.to_string());
    }
    let data = body
        .get("data")
        .and_then(|v| v.as_str())
        .ok_or("推送响应缺少数据")?;
    let plain = decrypt_body(&key, "/api/sync/push", &b64_decode(data)?)?;
    Ok(plain
        .get("accepted")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize)
}

/// 解除配对（服务端 token 作废）
pub fn unpair(pairing: &PairingInfo) -> Result<(), String> {
    let key = parse_qr_key(&pairing.key_b64)?;
    let url = format!("http://{}:{}/api/sync/unpair", pairing.ip, pairing.port);
    let req_body = encrypt_body(
        &key,
        "/api/sync/unpair",
        &serde_json::json!({ "token": pairing.token }),
    )?;
    let resp = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send_string(&req_body)
        .map_err(|e| format!("断开请求失败（{e}）"))?;
    let _ = resp.into_json::<serde_json::Value>();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_payload_parse() {
        let key = crypto::random::<32>();
        let payload = format!(
            "localvault://sync?ip=192.168.1.5&port=38528&code=123456&key={}",
            b64_encode(&key)
        );
        let (ip, port, code, k) = parse_qr_payload(&payload).unwrap();
        assert_eq!(ip, "192.168.1.5");
        assert_eq!(port, 38528);
        assert_eq!(code, "123456");
        assert_eq!(k, key);
    }

    #[test]
    fn qr_payload_http_guide_format() {
        let key = crypto::random::<32>();
        let payload = format!(
            "http://192.168.71.41:38528/mobile-qr?c=654321&k={}",
            b64_encode(&key)
        );
        let (ip, port, code, k) = parse_qr_payload(&payload).unwrap();
        assert_eq!(ip, "192.168.71.41");
        assert_eq!(port, 38528);
        assert_eq!(code, "654321");
        assert_eq!(k, key);
    }

    #[test]
    fn qr_payload_rejects_bad() {
        assert!(parse_qr_payload("localvault://sync?ip=1.2.3.4&code=123").is_err());
        assert!(parse_qr_payload("https://example.com/x").is_err());
        let key = crypto::random::<32>();
        let bad_key = b64_encode(&key[..16]);
        let payload = format!("localvault://sync?ip=1.2.3.4&port=38528&code=123456&key={bad_key}");
        assert!(parse_qr_payload(&payload).is_err());
    }

    #[test]
    fn encrypt_decrypt_body_roundtrip() {
        let key = crypto::random::<32>();
        let v = serde_json::json!({"token":"t","since":0});
        let enc = encrypt_body(&key, "/api/sync/pull", &v).unwrap();
        let plain = decrypt_body(&key, "/api/sync/pull", &b64_decode(&enc).unwrap()).unwrap();
        assert_eq!(plain["token"], "t");
    }
}
