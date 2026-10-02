// LocalVault 浏览器自动填充本地桥（v1.9.2）
// 设计：手动开关驱动启动/停止；Vault 锁定强制停止并销毁全部凭据；
//      仅监听 127.0.0.1；接口全部 Bearer token 鉴权；match 永不返回密码；
//      配对码 6 位、5 分钟一次性；密码仅在 fill 时刻下发。
use chacha20poly1305::aead::OsRng;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::vault::VaultManager;

pub const AUTOFILL_PORT: u16 = 38527;
pub const PAIR_TTL_MS: u64 = 5 * 60 * 1000; // 配对码有效期 5 分钟

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairDevice {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutofillStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub vault_unlocked: bool,
    pub paired_count: usize,
    pub last_error: Option<String>,
}

#[derive(Clone, Serialize)]
struct MatchEntry {
    id: String,
    name: String,
    username: String,
    url: String,
    category: String,
}

#[derive(Default)]
pub struct BridgeState {
    pub token: Option<String>,
    pub pair_code: Option<(String, u64)>, // (code, expires_at_ms)
    pub paired: Vec<PairDevice>,
    pub running: bool,
}

pub struct AutofillBridge {
    enabled: bool,
    running: bool,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    // 保留 Server 句柄，停止时主动 unblock 接收线程；Drop 会关闭监听 socket。
    server: Option<Arc<Server>>,
    state: Arc<Mutex<BridgeState>>,
    last_error: Option<String>,
    config_path: PathBuf,
}

fn config_path() -> PathBuf {
    let portable = std::env::var_os("LOCALVAULT_PORTABLE").is_some()
        || std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("portable.flag")))
            .map(|p| p.exists())
            .unwrap_or(false);
    let base = if portable {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("data")))
            .unwrap_or_else(|| PathBuf::from("data"))
    } else {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LocalVault")
    };
    base.join("autofill.json")
}

fn pairing_path() -> PathBuf {
    config_path().with_file_name("pairing.json")
}

fn load_pairing() -> Option<(String, PairDevice)> {
    let s = std::fs::read_to_string(pairing_path()).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let token = v.get("token")?.as_str()?.to_string();
    let device: PairDevice = serde_json::from_value(v.get("device")?.clone()).ok()?;
    Some((token, device))
}

fn save_pairing(token: &str, device: &PairDevice) -> Result<(), String> {
    let p = pairing_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    let json = serde_json::json!({ "token": token, "device": device });
    std::fs::write(&p, json.to_string()).map_err(|e| format!("写入配对信息失败：{e}"))
}

fn clear_pairing() {
    let p = pairing_path();
    if p.exists() {
        let _ = std::fs::remove_file(p);
    }
}

fn read_enabled(path: &PathBuf) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("enabled").and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

fn write_enabled(path: &PathBuf, enabled: bool) -> Result<(), String> {
    let dir = path.parent().ok_or("配置目录不存在")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    let json = serde_json::json!({ "enabled": enabled });
    std::fs::write(path, json.to_string()).map_err(|e| format!("写入填充服务配置失败：{e}"))
}

impl AutofillBridge {
    pub fn new() -> Self {
        let config_path = config_path();
        let enabled = read_enabled(&config_path);
        Self {
            enabled,
            running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
            server: None,
            state: Arc::new(Mutex::new(BridgeState::default())),
            last_error: None,
            config_path,
        }
    }

    pub fn status(&self) -> AutofillStatus {
        AutofillStatus {
            enabled: self.enabled,
            running: self.running,
            port: AUTOFILL_PORT,
            vault_unlocked: false, // 由调用方（main.rs）基于 VaultManager 实时填充
            paired_count: self.state.lock().map(|st| st.paired.len()).unwrap_or(0),
            last_error: self.last_error.clone(),
        }
    }

    /// 手动开关：开 -> 校验 Vault 已解锁并启动服务；关 -> 停止服务并销毁全部凭据。
    pub fn set_enabled(&mut self, enabled: bool, vault: &Arc<Mutex<VaultManager>>) -> Result<(), String> {
        if enabled {
            let unlocked = vault
                .lock()
                .map_err(|_| "state lock poisoned".to_string())?
                .is_unlocked();
            if !unlocked {
                self.last_error = Some("Vault 未解锁".into());
                return Err("请先解锁 Vault，再开启浏览器填充服务".into());
            }
            self.start(vault.clone())?;
        } else {
            self.stop();
        }
        self.enabled = enabled;
        write_enabled(&self.config_path, enabled)?;
        Ok(())
    }

    fn start(&mut self, vault: Arc<Mutex<VaultManager>>) -> Result<(), String> {
        if self.running {
            return Ok(());
        }
        let addr = format!("127.0.0.1:{AUTOFILL_PORT}");
        let server = Arc::new(
            Server::http(&addr)
                .map_err(|e| format!("无法启动填充服务（端口 {AUTOFILL_PORT} 可能被占用）：{e}"))?,
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in_thread = stop.clone();
        let server_in_thread = server.clone();
        self.stop_flag = stop;
        self.server = Some(server);
        let state = self.state.clone();
        let handle = std::thread::spawn(move || {
            // 正常运行时使用短超时；停止时由 stop() 显式调用 Server::unblock()，
            // 不依赖“200ms 轮询碰巧返回”来完成关闭。
            // 请求处理仍然独立线程：POST body 的 read_to_end 可能阻塞，
            // 不能让它阻塞监听/停止线程。
            loop {
                if stop_in_thread.load(Ordering::Acquire) {
                    break;
                }
                match server_in_thread.recv_timeout(std::time::Duration::from_millis(200)) {
                    Ok(Some(req)) => {
                        let st = state.clone();
                        let vt = vault.clone();
                        std::thread::spawn(move || handle_request(req, &st, &vt));
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        self.handle = Some(handle);
        self.running = true;
        self.last_error = None;
        // 启动后恢复已持久化的配对（token 与设备）：一次配对，之后解锁/重启均无需重新配对
        if let Ok(mut st) = self.state.lock() {
            st.pair_code = None;
            st.running = true;
            match load_pairing() {
                Some((token, device)) => {
                    st.token = Some(token);
                    st.paired = vec![device];
                }
                None => {
                    st.token = None;
                    st.paired.clear();
                }
            }
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        // 只要存在服务线程或 Server 句柄，就执行完整停止流程。
        // 不再允许出现 running=false 但监听 socket 仍存在的状态分裂。
        let had_service = self.running || self.handle.is_some() || self.server.is_some();

        if had_service {
            self.stop_flag.store(true, Ordering::Release);

            // tiny_http 官方提供的主动唤醒机制。停止不再依赖 recv_timeout 的轮询。
            if let Some(server) = self.server.as_ref() {
                server.unblock();
            }

            // 监听线程只负责接收请求，不执行 read_to_end，因此这里可以确定性退出。
            // 使用 join，而不是 3 秒后强行把 running 设为 false。
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }

            // 线程持有的 Arc 已释放；这里释放最后一个 Server 句柄，
            // tiny_http::Server::Drop 会关闭监听 socket。
            self.server.take();
            self.running = false;

            // 下一次启动必须使用新的、未置位的停止标志。
            self.stop_flag = Arc::new(AtomicBool::new(false));
        }

        if let Ok(mut st) = self.state.lock() {
            st.running = false;
            st.token = None;
            st.pair_code = None;
            st.paired.clear();
        }
    }

    /// Vault 锁定事件：停止服务、清空内存凭据；保留"已启用"开关记忆与磁盘配对，
    /// 以便再次解锁后自动恢复服务，无需重新配对。
    pub fn vault_locked(&mut self) {
        self.stop();
        self.last_error = None;
    }

    /// Vault 解锁事件：若总开关处于启用状态，自动启动服务并恢复持久化配对，
    /// 用户无需再手动开开关、无需重新输入配对码。
    pub fn on_vault_unlocked(&mut self, vault: &Arc<Mutex<VaultManager>>) {
        if !self.enabled || self.running {
            return;
        }
        let unlocked = vault.lock().map(|v| v.is_unlocked()).unwrap_or(false);
        if unlocked {
            let _ = self.start(vault.clone());
        }
    }

    /// 生成一次性 6 位配对码（仅服务运行时有效）。
    pub fn begin_pair(&mut self) -> Result<String, String> {
        if !self.running {
            return Err("填充服务未运行，请先打开开关".into());
        }
        let code = gen_pair_code();
        let now = now_ms();
        let mut st = self.state.lock().map_err(|_| "state lock poisoned".to_string())?;
        st.pair_code = Some((code.clone(), now + PAIR_TTL_MS));
        Ok(code)
    }

    pub fn unpair_all(&mut self) -> Result<(), String> {
        let mut st = self.state.lock().map_err(|_| "state lock poisoned".to_string())?;
        st.token = None;
        st.paired.clear();
        drop(st);
        clear_pairing();
        Ok(())
    }
}

// ---------- HTTP 请求处理 ----------

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, OPTIONS"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Authorization, Content-Type"[..]).unwrap(),
    ]
}

fn json_headers() -> Vec<Header> {
    let mut h = cors_headers();
    h.push(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    h
}

fn build_response(status: u16, headers: Vec<Header>, body: Vec<u8>) -> Resp {
    let mut resp = Response::from_data(body).with_status_code(StatusCode(status));
    for h in headers {
        resp = resp.with_header(h);
    }
    resp
}

fn json_response(status: u16, obj: &serde_json::Value) -> Resp {
    let body = serde_json::to_vec(obj).unwrap_or_default();
    build_response(status, json_headers(), body)
}

type Resp = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn handle_request(
    mut req: Request,
    state: &Arc<Mutex<BridgeState>>,
    vault: &Arc<Mutex<VaultManager>>,
) {
    let method = req.method().clone();
    // CORS 预检
    if method == Method::Options {
        let _ = req.respond(build_response(204, cors_headers(), Vec::new()));
        return;
    }
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or(&url).to_string();
    let mut body = Vec::new();
    if method == Method::Post {
        let reader = req.as_reader();
        let _ = reader.read_to_end(&mut body);
    }
    let auth = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().to_string());
    let resp = route(&method, &path, &url, &body, auth.as_deref(), state, vault);
    let _ = req.respond(resp);
}

fn bearer_token(auth: Option<&str>) -> Option<String> {
    auth.and_then(|a| a.strip_prefix("Bearer ")).map(|t| t.trim().to_string())
}

fn route(
    method: &Method,
    path: &str,
    url: &str,
    body: &[u8],
    auth: Option<&str>,
    state: &Arc<Mutex<BridgeState>>,
    vault: &Arc<Mutex<VaultManager>>,
) -> Resp {
    match path {
        "/api/health" => {
            if *method != Method::Get {
                return json_response(405, &serde_json::json!({"error": "method not allowed"}));
            }
            // running 状态由 start()/stop() 维护在 BridgeState 中，不再硬编码 true；
            // 锁定 Vault 后服务已停止时，前端能准确看到"未运行"。
            let (running, paired) = match state.lock() {
                Ok(st) => (st.running, st.paired.len()),
                Err(_) => (false, 0),
            };
            let vault_unlocked = vault.lock().map(|v| v.is_unlocked()).unwrap_or(false);
            json_response(
                200,
                &serde_json::json!({
                    "running": running,
                    "port": AUTOFILL_PORT,
                    "vaultUnlocked": vault_unlocked,
                    "pairedCount": paired
                }),
            )
        }
        "/api/pair" => {
            if *method != Method::Post {
                return json_response(405, &serde_json::json!({"error": "method not allowed"}));
            }
            handle_pair(body, state)
        }
        "/api/match" => {
            if *method != Method::Get {
                return json_response(405, &serde_json::json!({"error": "method not allowed"}));
            }
            handle_match(url, auth, state, vault)
        }
        "/api/fill" => {
            if *method != Method::Post {
                return json_response(405, &serde_json::json!({"error": "method not allowed"}));
            }
            handle_fill(body, auth, state, vault)
        }
        "/api/unpair" => {
            if *method != Method::Post {
                return json_response(405, &serde_json::json!({"error": "method not allowed"}));
            }
            handle_unpair(auth, state)
        }
        _ => json_response(404, &serde_json::json!({"error": "not found"})),
    }
}

#[derive(Deserialize)]
struct PairReq {
    code: String,
    #[serde(default)]
    name: Option<String>,
}

fn handle_pair(body: &[u8], state: &Arc<Mutex<BridgeState>>) -> Resp {
    let Ok(req) = serde_json::from_slice::<PairReq>(body) else {
        return json_response(400, &serde_json::json!({"error": "请求格式错误"}));
    };
    let code = req.code.trim().to_string();
    if code.chars().count() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
        return json_response(400, &serde_json::json!({"error": "配对码应为 6 位数字"}));
    }
    let Ok(mut st) = state.lock() else {
        return json_response(500, &serde_json::json!({"error": "内部状态异常"}));
    };
    let now = now_ms();
    // 一次性：无论成功失败都消费当前配对码
    let issued = st.pair_code.take();
    match issued {
        Some((valid, expires)) if valid == code && now <= expires => {
            let token = gen_token();
            let device = PairDevice {
                id: gen_device_id(),
                name: req.name.unwrap_or_else(|| "浏览器扩展".into()),
                paired_at: now,
            };
            st.token = Some(token.clone());
            st.paired.push(device.clone());
            // 持久化配对：解锁/重启后自动恢复，扩展无需重复输入配对码
            let _ = save_pairing(&token, &device);
            json_response(
                200,
                &serde_json::json!({ "token": token, "device": device }),
            )
        }
        _ => json_response(403, &serde_json::json!({"error": "配对码无效或已过期"})),
    }
}

fn handle_match(
    url: &str,
    auth: Option<&str>,
    state: &Arc<Mutex<BridgeState>>,
    vault: &Arc<Mutex<VaultManager>>,
) -> Resp {
    let Some(token) = bearer_token(auth) else {
        return json_response(401, &serde_json::json!({"error": "缺少访问令牌"}));
    };
    if !is_valid_token(state, &token) {
        return json_response(401, &serde_json::json!({"error": "访问令牌无效"}));
    }
    let page_host = url
        .split('?')
        .nth(1)
        .and_then(|q| {
            q.split('&')
                .find(|kv| kv.starts_with("url="))
                .and_then(|kv| kv.splitn(2, '=').nth(1))
        })
        .and_then(|u| percent_decode(u))
        .and_then(|u| host_of(&u));
    let Some(page_host) = page_host else {
        return json_response(400, &serde_json::json!({"error": "缺少有效的 url 参数"}));
    };
    let entries = match vault.lock().map_err(|_| "state lock poisoned".to_string()).and_then(|v| v.list_entry_meta()) {
        Ok(xs) => xs,
        Err(_) => return json_response(503, &serde_json::json!({"error": "Vault 未解锁或不可用"})),
    };
    let matched: Vec<MatchEntry> = entries
        .into_iter()
        .filter(|e| url_matches(&e.url, &page_host))
        .map(|e| MatchEntry {
            id: e.id,
            name: e.name,
            username: e.username,
            url: e.url,
            category: e.category,
        })
        .collect();
    json_response(
        200,
        &serde_json::json!({ "entries": matched, "count": matched.len() }),
    )
}

#[derive(Deserialize)]
struct FillReq {
    url: String,
    entry_id: String,
}

fn handle_fill(
    body: &[u8],
    auth: Option<&str>,
    state: &Arc<Mutex<BridgeState>>,
    vault: &Arc<Mutex<VaultManager>>,
) -> Resp {
    let Some(token) = bearer_token(auth) else {
        return json_response(401, &serde_json::json!({"error": "缺少访问令牌"}));
    };
    if !is_valid_token(state, &token) {
        return json_response(401, &serde_json::json!({"error": "访问令牌无效"}));
    }
    let Ok(req) = serde_json::from_slice::<FillReq>(body) else {
        return json_response(400, &serde_json::json!({"error": "请求格式错误"}));
    };
    let Some(page_host) = host_of(&req.url) else {
        return json_response(400, &serde_json::json!({"error": "缺少有效的 url 参数"}));
    };
    // 匹配阶段只取元数据，不解密任何密码
    let metas = match vault.lock().map_err(|_| "state lock poisoned".to_string()).and_then(|v| v.list_entry_meta()) {
        Ok(xs) => xs,
        Err(_) => return json_response(503, &serde_json::json!({"error": "Vault 未解锁或不可用"})),
    };
    let Some(meta) = metas.into_iter().find(|e| e.id == req.entry_id) else {
        return json_response(404, &serde_json::json!({"error": "密码条目不存在"}));
    };
    if !url_matches(&meta.url, &page_host) {
        return json_response(403, &serde_json::json!({"error": "条目与当前页面不匹配"}));
    }
    // 密码只在用户点击填充的瞬间，于 Rust 层解密单条后直接下发给扩展
    let plain = match vault
        .lock()
        .map_err(|_| "state lock poisoned".to_string())
        .and_then(|v| v.entry_plain_password(&req.entry_id))
    {
        Ok(p) => p,
        Err(_) => return json_response(503, &serde_json::json!({"error": "Vault 未解锁或不可用"})),
    };
    json_response(200, &serde_json::json!({ "password": plain }))
}

fn handle_unpair(auth: Option<&str>, state: &Arc<Mutex<BridgeState>>) -> Resp {
    let Some(token) = bearer_token(auth) else {
        return json_response(401, &serde_json::json!({"error": "缺少访问令牌"}));
    };
    let Ok(mut st) = state.lock() else {
        return json_response(500, &serde_json::json!({"error": "内部状态异常"}));
    };
    if st.token.as_deref() != Some(token.as_str()) {
        return json_response(401, &serde_json::json!({"error": "访问令牌无效"}));
    }
    st.token = None;
    st.paired.clear();
    json_response(200, &serde_json::json!({ "ok": true }))
}

fn is_valid_token(state: &Mutex<BridgeState>, token: &str) -> bool {
    match state.lock() {
        Ok(st) => st.token.as_deref() == Some(token),
        Err(_) => false,
    }
}

// ---------- 工具函数 ----------

fn secure_random<const N: usize>() -> [u8; N] {
    let mut x = [0u8; N];
    OsRng.fill_bytes(&mut x);
    x
}

fn gen_pair_code() -> String {
    let r = secure_random::<3>();
    let n = u32::from_be_bytes([0, r[0], r[1], r[2]]) % 1_000_000;
    format!("{n:06}")
}

fn gen_token() -> String {
    hex::encode(secure_random::<16>())
}

fn gen_device_id() -> String {
    hex::encode(secure_random::<8>())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = (bytes[i + 1] as char).to_digit(16)?;
            let l = (bytes[i + 2] as char).to_digit(16)?;
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 解析 URL 的 host（去协议、端口、路径；统一小写；支持 IPv6 [..]）。
fn host_of(raw: &str) -> Option<String> {
    let s = raw.trim();
    let without_scheme = if let Some(idx) = s.find("://") {
        &s[idx + 3..]
    } else {
        s
    };
    let authority = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(close) = authority.find(']') {
        // IPv6：[::1]:port
        authority[1..close].to_string()
    } else if let Some(idx) = authority.rfind(':') {
        let after = &authority[idx + 1..];
        if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
            authority[..idx].to_string()
        } else {
            authority.to_string()
        }
    } else {
        authority.to_string()
    };
    let h = host.trim_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        None
    } else {
        Some(h)
    }
}

/// 子域双向匹配：条目 host 与页面 host 相同，或一方是另一方的子域。
fn url_matches(entry_url: &str, page_host: &str) -> bool {
    let Some(entry_host) = host_of(entry_url) else {
        return false;
    };
    if entry_host.is_empty() || page_host.is_empty() {
        return false;
    }
    if entry_host == page_host {
        return true;
    }
    if entry_host.ends_with(&format!(".{page_host}"))
        || page_host.ends_with(&format!(".{entry_host}"))
    {
        return true;
    }
    false
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn host_parsing_cases() {
        assert_eq!(host_of("https://example.com/login?x=1").unwrap(), "example.com");
        assert_eq!(host_of("http://a.b.example.com:8080/path").unwrap(), "a.b.example.com");
        assert_eq!(host_of("example.com").unwrap(), "example.com");
        assert_eq!(host_of("https://[::1]:38527/api").unwrap(), "::1");
        assert_eq!(host_of("  https://EXAMPLE.COM  ").unwrap(), "example.com");
        assert!(host_of("").is_none());
        assert!(host_of("https://").is_none());
    }

    #[test]
    fn subdomain_matching() {
        assert!(url_matches("https://example.com", "example.com"));
        assert!(url_matches("https://example.com", "login.example.com"));
        assert!(url_matches("https://mail.example.com", "example.com"));
        assert!(url_matches("https://a.b.example.com", "b.example.com"));
        // 平级子域（a.b.example.com 与 c.example.com）不是对方的子域，不匹配（安全优先）
        assert!(!url_matches("https://a.b.example.com", "c.example.com"));
        assert!(url_matches("https://example.com:8443/x", "sub.example.com"));
        assert!(!url_matches("https://example.com", "notexample.com"));
        assert!(!url_matches("https://example.com", "example.com.evil.com"));
        assert!(!url_matches("", "example.com"));
        assert!(!url_matches("https://example.com", ""));
    }

    #[test]
    fn pair_code_format() {
        for _ in 0..50 {
            let c = gen_pair_code();
            assert_eq!(c.len(), 6);
            assert!(c.chars().all(|x| x.is_ascii_digit()));
        }
    }

    #[test]
    fn token_and_device_generation() {
        let t1 = gen_token();
        let t2 = gen_token();
        assert_eq!(t1.len(), 32);
        assert_ne!(t1, t2);
        assert_ne!(gen_device_id(), gen_device_id());
    }

    #[test]
    fn enabled_persistence_roundtrip() {
        let path = std::env::temp_dir().join(format!("lv-autofill-cfg-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(!read_enabled(&path));
        write_enabled(&path, true).unwrap();
        assert!(read_enabled(&path));
        write_enabled(&path, false).unwrap();
        assert!(!read_enabled(&path));
        let _ = std::fs::remove_file(&path);
    }

    fn test_vault() -> Arc<Mutex<VaultManager>> {
        // 未解锁的 VaultManager：list_entries 立即返回 "Vault locked"，不会触碰磁盘。
        Arc::new(Mutex::new(VaultManager::new()))
    }

    fn start_test_server(state: Arc<Mutex<BridgeState>>, vault: Arc<Mutex<VaultManager>>) -> u16 {
        let server = Server::http("127.0.0.1:0").unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        let _handle = std::thread::spawn(move || {
            for req in server.incoming_requests() {
                handle_request(req, &state, &vault);
            }
        });
        port
    }

    fn http_get(port: u16, path: &str, token: Option<&str>) -> (u16, String) {
        use std::io::Write;
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n");
        if let Some(t) = token {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        req.push_str("Connection: close\r\n\r\n");
        s.write_all(req.as_bytes()).unwrap();
        read_http_response(&mut s)
    }

    fn http_post(port: u16, path: &str, body: &str, token: Option<&str>) -> (u16, String) {
        use std::io::Write;
        let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut req = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
        if let Some(t) = token {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        req.push_str("Connection: close\r\n\r\n");
        req.push_str(body);
        s.write_all(req.as_bytes()).unwrap();
        read_http_response(&mut s)
    }

    fn read_http_response(s: &mut std::net::TcpStream) -> (u16, String) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = s.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                // 尝试读完 body（简单处理：短连接下再读一次）
                let _ = s.read(&mut chunk);
                break;
            }
        }
        let text = String::from_utf8_lossy(&buf).to_string();
        let status: u16 = text
            .split_whitespace()
            .nth(1)
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    #[test]
    fn http_health_unauth() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        let vault = test_vault();
        let port = start_test_server(state, vault);
        let (status, body) = http_get(port, "/api/health", None);
        assert_eq!(status, 200);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["running"], true);
        assert_eq!(v["port"], AUTOFILL_PORT);
        assert_eq!(v["vaultUnlocked"], false);
    }

    #[test]
    fn http_auth_enforcement() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        let vault = test_vault();
        let port = start_test_server(state, vault);
        // 无 token -> 401
        let (s, _) = http_get(port, "/api/match?url=https%3A%2F%2Fexample.com", None);
        assert_eq!(s, 401);
        let (s, _) = http_post(port, "/api/fill", r#"{"url":"https://example.com","entry_id":"x"}"#, None);
        assert_eq!(s, 401);
        let (s, _) = http_post(port, "/api/unpair", "", None);
        assert_eq!(s, 401);
        // 错误 token -> 401
        let (s, _) = http_get(port, "/api/match?url=https%3A%2F%2Fexample.com", Some("deadbeef"));
        assert_eq!(s, 401);
        // 未知路由 -> 404
        let (s, _) = http_get(port, "/api/nope", None);
        assert_eq!(s, 404);
        // 错误方法 -> 405
        let (s, _) = http_post(port, "/api/health", "", None);
        assert_eq!(s, 405);
    }

    #[test]
    fn http_pair_then_vault_locked_503() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        let vault = test_vault();
        let port = start_test_server(state.clone(), vault.clone());
        // 无配对码 -> 403
        let (s, _) = http_post(port, "/api/pair", r#"{"code":"000000"}"#, None);
        assert_eq!(s, 403);
        // 预置配对码
        state.lock().unwrap().pair_code = Some(("123456".into(), now_ms() + 60_000));
        let (s, body) = http_post(port, "/api/pair", r#"{"code":"123456","name":"Edge"}"#, None);
        assert_eq!(s, 200);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let token = v["token"].as_str().unwrap().to_string();
        assert!(v["device"]["name"].as_str().unwrap().contains("Edge"));
        // 配对码一次性：再次使用同一码 -> 403
        let (s, _) = http_post(port, "/api/pair", r#"{"code":"123456"}"#, None);
        assert_eq!(s, 403);
        // 有 token 但 Vault 未解锁 -> match/fill 503
        let (s, _) = http_get(port, "/api/match?url=https%3A%2F%2Fexample.com", Some(&token));
        assert_eq!(s, 503);
        let (s, _) = http_post(port, "/api/fill", r#"{"url":"https://example.com","entry_id":"x"}"#, Some(&token));
        assert_eq!(s, 503);
        // unpair 后 token 失效
        let (s, _) = http_post(port, "/api/unpair", "", Some(&token));
        assert_eq!(s, 200);
        let (s, _) = http_get(port, "/api/match?url=https%3A%2F%2Fexample.com", Some(&token));
        assert_eq!(s, 401);
    }

    #[test]
    fn pair_code_expiry_rejected() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        let vault = test_vault();
        let port = start_test_server(state.clone(), vault);
        state.lock().unwrap().pair_code = Some(("654321".into(), now_ms() - 1000));
        let (s, _) = http_post(port, "/api/pair", r#"{"code":"654321"}"#, None);
        assert_eq!(s, 403);
    }

    #[test]
    fn bad_pair_code_shape() {
        let state = Arc::new(Mutex::new(BridgeState::default()));
        let vault = test_vault();
        let port = start_test_server(state, vault);
        let (s, _) = http_post(port, "/api/pair", r#"{"code":"12345"}"#, None);
        assert_eq!(s, 400);
        let (s, _) = http_post(port, "/api/pair", r#"{"code":"12ab56"}"#, None);
        assert_eq!(s, 400);
    }
}
