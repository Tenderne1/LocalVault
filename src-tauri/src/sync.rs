// LocalVault 局域网同步桥（v1.9.4 · 局域网同步版）
// 设计：
//  - 解锁 Vault 自动启动同步服务（绑定 0.0.0.0:38528），锁定 Vault 强制停止并销毁全部凭据；
//  - 所有业务请求/响应体均用 XChaCha20-Poly1305 加密（密钥来自配对二维码），
//    网络传输始终是密文，桌面 WebView 前端不接触任何密码明文；
//  - 配对码 6 位、5 分钟一次性；配对后 token+设备持久化，解锁/重启无需重新配对；
//  - pull/push 双向增量同步（按 updated_at 新者胜）；密码明文仅在 Rust 层解密并立即加密传输。
use base64::Engine;
use chacha20poly1305::{aead::{Aead,OsRng,Payload},KeyInit,XChaCha20Poly1305,XNonce};
use rand_core::RngCore;
use serde::{Deserialize,Serialize};
use std::path::PathBuf;
use tauri::Emitter;
use std::sync::atomic::{AtomicBool,Ordering};
use std::sync::{Arc,Mutex};
use std::thread::JoinHandle;
use tiny_http::{Header,Method,Request,Response,Server,StatusCode};

use crate::vault::{Entry,VaultManager};

pub const SYNC_PORT:u16=38528;
pub const PAIR_TTL_MS:u64=5*60*1000; // 配对码有效期 5 分钟
const KEY_LEN:usize=32;

#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SyncDevice{
    pub id:String,
    pub name:String,
    pub paired_at:u64,
}

/// 同步日志条目（不含密码，只记录提示级信息）
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SyncLog{
    pub time:u64,
    pub device:String,
    pub action:String, // upsert / delete / pull / pair
    pub name:String,   // 条目名（删除时为 id 尾部）
    pub kind:String,   // 条目类型（网站/银行卡/…）
}

#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SyncStatus{
    pub running:bool,
    pub port:u16,
    pub vault_unlocked:bool,
    pub paired_count:usize,
    pub ip:Option<String>,
    pub last_error:Option<String>,
    pub sync_logs:Vec<SyncLog>,
}

/// 生成二维码所需信息（前端拼 localvault:// 链接后渲染二维码）
#[derive(Clone,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SyncPairInfo{
    pub ip:String,
    pub port:u16,
    pub code:String,
    pub key:String, // base64 32 字节传输密钥
}

/// 手机端推送的分类操作
#[derive(Clone,Debug,Deserialize,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CategoryOp{
    pub op:String,        // create / update / delete
    pub name:String,
    pub icon:String,
    #[serde(default)]
    pub old_name:String,
    #[serde(default)]
    pub parent_name:Option<String>,
}

/// 同步传输用条目结构（不含桌面会话密文/评分等前端专用字段；密码明文仅在加密体内传输）
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SyncEntry{
    pub id:String,
    #[serde(default)] pub seq:i64,
    #[serde(rename="type",default="default_type")] pub entry_type:String,
    #[serde(default)] pub name:String,
    #[serde(default)] pub username:String,
    #[serde(default)] pub email:String,
    #[serde(default)] pub phone:String,
    #[serde(default)] pub password:String,
    #[serde(default)] pub nickname:String,
    #[serde(default)] pub url:String,
    #[serde(default)] pub notes:String,
    #[serde(default="default_category")] pub category:String,
    #[serde(default="empty_tags")] pub tags:Vec<String>,
    #[serde(default)] pub favorite:bool,
    #[serde(default="zero")] pub updated_at:i64,
    #[serde(default)] pub expires_at:Option<i64>,
}
fn default_type()->String{"网站".into()}
fn default_category()->String{"默认".into()}
fn empty_tags()->Vec<String>{Vec::new()}
fn zero()->i64{0}

impl From<&Entry> for SyncEntry{
    fn from(e:&Entry)->Self{
        Self{
            id:e.id.clone(),seq:e.seq,entry_type:e.entry_type.clone(),
            name:e.name.clone(),username:e.username.clone(),email:e.email.clone(),
            phone:e.phone.clone(),password:e.password.clone(),nickname:e.nickname.clone(),
            url:e.url.clone(),notes:e.notes.clone(),category:e.category.clone(),
            tags:e.tags.clone(),favorite:e.favorite,updated_at:e.updated_at,
            expires_at:e.expires_at,
        }
    }
}
impl From<SyncEntry> for Entry{
    fn from(e:SyncEntry)->Self{
        Self{
            id:e.id,seq:e.seq,entry_type:e.entry_type,
            name:e.name,username:e.username,email:e.email,
            phone:e.phone,password:e.password,nickname:e.nickname,
            url:e.url,notes:e.notes,category:e.category,
            tags:e.tags,favorite:e.favorite,updated_at:e.updated_at,
            expires_at:e.expires_at,
            password_encrypted:None,password_score:None,password_reused:None,
        }
    }
}

#[derive(Default)]
pub struct SyncState{
    pub pair_code:Option<(String,u64)>, // (code, expires_at_ms)
    pub pair_key:Option<[u8;KEY_LEN]>,
    pub token:Option<String>,
    pub paired:Vec<SyncDevice>,
    pub running:bool,
    pub sync_logs:Vec<SyncLog>,
    /// 桌面端 AppHandle：手机推送成功后向前端发 sync-updated 事件，前端自动刷新列表
    pub app:Option<tauri::AppHandle>,
}

pub struct SyncBridge{
    running:bool,
    stop_flag:Arc<AtomicBool>,
    handle:Option<JoinHandle<()>>,
    server:Option<Arc<Server>>,
    state:Arc<Mutex<SyncState>>,
    last_error:Option<String>,
    config_path:PathBuf,
}

fn config_path()->PathBuf{
    let portable=std::env::var_os("LOCALVAULT_PORTABLE").is_some()
        ||std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("portable.flag"))).map(|p|p.exists()).unwrap_or(false);
    let base=if portable{
        std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("data"))).unwrap_or_else(||PathBuf::from("data"))
    }else{
        dirs::data_local_dir().unwrap_or_else(||PathBuf::from(".")).join("LocalVault")
    };
    base.join("sync.json")
}
fn pairing_path()->PathBuf{config_path().with_file_name("sync-pairing.json")}

/// 创建带 SO_REUSEADDR 的监听 socket：避免 Windows 上停止服务后监听端口
/// 因 TIME_WAIT 未释放，导致「锁定→再解锁」重启服务时绑定失败（os error 10048）。
fn create_reuse_listener(addr:&str)->Result<tiny_http::Listener,String>{
    use socket2::{Domain,Protocol,Socket,Type};
    let sock_addr:std::net::SocketAddr=addr.parse().map_err(|e|format!("地址解析失败：{e}"))?;
    let socket=Socket::new(Domain::IPV4,Type::STREAM,Some(Protocol::TCP))
        .map_err(|e|format!("创建监听 socket 失败：{e}"))?;
    socket.set_reuse_address(true)
        .map_err(|e|format!("设置端口复用（SO_REUSEADDR）失败：{e}"))?;
    socket.bind(&sock_addr.into())
        .map_err(|e|format!("无法启动局域网同步服务（端口 {SYNC_PORT} 可能被占用）：{e}"))?;
    socket.listen(1024).map_err(|e|format!("开始监听失败：{e}"))?;
    let std_listener:std::net::TcpListener=socket.into();
    Ok(std_listener.into())
}

/// 加载持久化配对。返回 (token, device, key_b64)。
/// 旧版 pairing.json 没有 key 字段：视为未配对（返回 None），手机端需重新扫码配对一次，
/// 新版会同时持久化传输密钥，锁定/解锁后自动恢复全部凭据。
fn load_pairing()->Option<(String,SyncDevice,String)>{
    let s=std::fs::read_to_string(pairing_path()).ok()?;
    let v:serde_json::Value=serde_json::from_str(&s).ok()?;
    let token=v.get("token")?.as_str()?.to_string();
    let device:SyncDevice=serde_json::from_value(v.get("device")?.clone()).ok()?;
    let key_b64=v.get("key")?.as_str()?.to_string();
    Some((token,device,key_b64))
}
fn save_pairing(token:&str,device:&SyncDevice,key_b64:&str)->Result<(),String>{
    let p=pairing_path();
    if let Some(dir)=p.parent(){std::fs::create_dir_all(dir).map_err(|e|format!("创建配置目录失败：{e}"))?;}
    let json=serde_json::json!({"token":token,"device":device,"key":key_b64});
    std::fs::write(&p,json.to_string()).map_err(|e|format!("写入配对信息失败：{e}"))
}
fn clear_pairing(){
    let p=pairing_path();
    if p.exists(){let _=std::fs::remove_file(p);}
}

impl SyncBridge{
    pub fn new()->Self{
        Self{
            running:false,
            stop_flag:Arc::new(AtomicBool::new(false)),
            handle:None,
            server:None,
            state:Arc::new(Mutex::new(SyncState::default())),
            last_error:None,
            config_path:config_path(),
        }
    }

    /// 注入桌面端 AppHandle：手机推送成功后向前端 emit sync-updated，触发前端自动刷新
    pub fn set_app(&mut self, app: tauri::AppHandle){
        if let Ok(mut st)=self.state.lock(){st.app=Some(app);}
    }

    pub fn status(&self)->SyncStatus{
        SyncStatus{
            running:self.running,
            port:SYNC_PORT,
            vault_unlocked:false, // 由 main.rs 实时填充
            paired_count:self.state.lock().map(|st|st.paired.len()).unwrap_or(0),
            ip:lan_ips().first().cloned(),
            last_error:self.last_error.clone(),
            sync_logs:self.state.lock().map(|st|st.sync_logs.clone()).unwrap_or_default(),
        }
    }

    fn start(&mut self,vault:Arc<Mutex<VaultManager>>)->Result<(),String>{
        if self.running{return Ok(());}
        let addr=format!("0.0.0.0:{SYNC_PORT}");
        let server=match create_reuse_listener(&addr){
            Ok(l)=>Arc::new(Server::from_listener(l,None).map_err(|e|{
                let msg=format!("无法启动局域网同步服务：{e}");
                eprintln!("[sync] from_listener failed: {msg}");
                msg
            })?),
            Err(msg)=>{
                self.last_error=Some(msg.clone());
                eprintln!("[sync] start failed: {msg}");
                return Err(msg);
            }
        };
        let stop=Arc::new(AtomicBool::new(false));
        let stop_in_thread=stop.clone();
        let server_in_thread=server.clone();
        self.stop_flag=stop;
        self.server=Some(server);
        let state=self.state.clone();
        let handle=std::thread::spawn(move||{
            loop{
                if stop_in_thread.load(Ordering::Acquire){break;}
                match server_in_thread.recv_timeout(std::time::Duration::from_millis(200)){
                    Ok(Some(req))=>{
                        let st=state.clone();
                        let vt=vault.clone();
                        std::thread::spawn(move||handle_request(req,&st,&vt));
                    }
                    Ok(None)=>{}
                    Err(_)=>break,
                }
            }
        });
        self.handle=Some(handle);
        self.running=true;
        self.last_error=None;
        // 启动后恢复已持久化的配对（token、设备与传输密钥）：一次配对，之后解锁/重启均无需重新配对
        if let Ok(mut st)=self.state.lock(){
            st.pair_code=None;
            st.running=true;
            match load_pairing(){
                Some((token,device,key_b64))=>{
                    st.token=Some(token);
                    st.paired=vec![device];
                    st.pair_key=base64::Engine::decode(&base64::engine::general_purpose::STANDARD,key_b64)
                        .ok()
                        .and_then(|b|b.try_into().ok());
                }
                None=>{
                    st.token=None;
                    st.paired.clear();
                    st.pair_key=None;
                }
            }
        }
        Ok(())
    }

    pub fn stop(&mut self){
        let had_service=self.running||self.handle.is_some()||self.server.is_some();
        if had_service{
            self.stop_flag.store(true,Ordering::Release);
            if let Some(server)=self.server.as_ref(){server.unblock();}
            if let Some(h)=self.handle.take(){let _=h.join();}
            self.server.take();
            self.running=false;
            self.stop_flag=Arc::new(AtomicBool::new(false));
        }
        if let Ok(mut st)=self.state.lock(){
            st.running=false;
            st.token=None;
            st.pair_code=None;
            st.pair_key=None;
            st.paired.clear();
        }
    }

    /// Vault 锁定：停止服务、销毁内存凭据与传输密钥；保留磁盘配对记录（解锁后自动恢复）
    pub fn vault_locked(&mut self){
        self.stop();
        self.last_error=None;
    }

    /// Vault 解锁：自动启动同步服务并恢复持久化配对，用户无需每次手动开服务
    pub fn on_vault_unlocked(&mut self,vault:&Arc<Mutex<VaultManager>>){
        if self.running{return;}
        let unlocked=vault.lock().map(|v|v.is_unlocked()).unwrap_or(false);
        if !unlocked{
            eprintln!("[sync] on_vault_unlocked: vault 未处于解锁状态，跳过自动启动");
            return;
        }
        match self.start(vault.clone()){
            Ok(_)=>eprintln!("[sync] 解锁后自动启动成功，端口 {SYNC_PORT}"),
            Err(e)=>{
                self.last_error=Some(e.clone());
                eprintln!("[sync] 解锁后自动启动失败：{e}");
            }
        }
    }

    /// 生成一次性 6 位配对码 + 32 字节传输密钥（含局域网 IP，供前端拼二维码）
    pub fn begin_pair(&mut self)->Result<SyncPairInfo,String>{
        if !self.running{
            return Err("局域网同步服务未运行，请先解锁 Vault".into());
        }
        let code=gen_pair_code();
        let key=secure_random::<KEY_LEN>();
        let now=now_ms();
        let mut st=self.state.lock().map_err(|_|"state lock poisoned".to_string())?;
        st.pair_code=Some((code.clone(),now+PAIR_TTL_MS));
        st.pair_key=Some(key);
        let ip=lan_ips().first().cloned().unwrap_or_else(||"127.0.0.1".to_string());
        Ok(SyncPairInfo{
            ip,
            port:SYNC_PORT,
            code,
            key:base64::engine::general_purpose::STANDARD.encode(&key),
        })
    }

    pub fn unpair_all(&mut self)->Result<(),String>{
        let mut st=self.state.lock().map_err(|_|"state lock poisoned".to_string())?;
        st.token=None;
        st.paired.clear();
        st.pair_key=None;
        drop(st);
        clear_pairing();
        Ok(())
    }
}

// ---------- HTTP 请求处理 ----------

fn cors_headers()->Vec<Header>{
    vec![
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..],&b"*"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Methods"[..],&b"GET, POST, OPTIONS"[..]).unwrap(),
        Header::from_bytes(&b"Access-Control-Allow-Headers"[..],&b"Content-Type"[..]).unwrap(),
    ]
}
fn json_headers()->Vec<Header>{
    let mut h=cors_headers();
    h.push(Header::from_bytes(&b"Content-Type"[..],&b"application/json"[..]).unwrap());
    h
}
fn build_response(status:u16,headers:Vec<Header>,body:Vec<u8>)->Resp{
    let mut resp=Response::from_data(body).with_status_code(StatusCode(status));
    for h in headers{resp=resp.with_header(h);}
    resp
}
fn json_response(status:u16,obj:&serde_json::Value)->Resp{
    let body=serde_json::to_vec(obj).unwrap_or_default();
    build_response(status,json_headers(),body)
}
type Resp=tiny_http::Response<std::io::Cursor<Vec<u8>>>;

// 加密体：base64(nonce24 || XChaCha20-Poly1305(key, nonce, plaintext, aad=path))
fn encrypt_body(key:&[u8;KEY_LEN],path:&str,plain:&[u8])->Result<String,String>{
    let cipher=XChaCha20Poly1305::new(key.into());
    let rand=secure_random::<24>();
    let nonce=XNonce::from_slice(&rand);
    let ct=cipher.encrypt(nonce,Payload{msg:plain,aad:path.as_bytes()}).map_err(|_|"加密失败".to_string())?;
    let mut out=Vec::with_capacity(24+ct.len());
    out.extend_from_slice(&rand);
    out.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(&out))
}
fn decrypt_body(key:&[u8;KEY_LEN],path:&str,body:&[u8])->Result<Vec<u8>,String>{
    let raw=base64::engine::general_purpose::STANDARD.decode(body).map_err(|_|"数据格式错误".to_string())?;
    if raw.len()<24{return Err("数据不完整".into());}
    let (nonce,ct)=raw.split_at(24);
    let cipher=XChaCha20Poly1305::new(key.into());
    cipher.decrypt(XNonce::from_slice(nonce),Payload{msg:ct,aad:path.as_bytes()}).map_err(|_|"解密失败".to_string())
}

fn handle_request(mut req:Request,state:&Arc<Mutex<SyncState>>,vault:&Arc<Mutex<VaultManager>>){
    let method=req.method().clone();
    if method==Method::Options{
        let _=req.respond(build_response(204,cors_headers(),Vec::new()));
        return;
    }
    let url=req.url().to_string();
    let path=url.split('?').next().unwrap_or(&url).to_string();
    let mut body=Vec::new();
    if method==Method::Post{
        let reader=req.as_reader();
        let _=reader.read_to_end(&mut body);
    }
    // 微信等第三方扫码引导页（GET，无需加密）
    let resp=if path=="/mobile-qr"&&method==Method::Get{
        handle_mobile_qr(&url,state)
    }else{
        route(&method,&path,&body,state,vault)
    };
    let _=req.respond(resp);
}

/// 百分号解码（与前端 encodeURIComponent 对应）
fn pct_decode(s:&str)->String{
    let bytes=s.as_bytes();
    let mut out=Vec::with_capacity(bytes.len());
    let mut i=0;
    while i<bytes.len(){
        if bytes[i]==b'%'&&i+2<bytes.len(){
            if let Ok(b)=u8::from_str_radix(&s[i+1..i+3],16){out.push(b);i+=3;continue;}
        }
        out.push(bytes[i]);i+=1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 微信/浏览器扫码引导页：http://ip:38528/mobile-qr?c=配对码&k=key
/// 页面不泄露密钥明文，提供"复制链接到 App"与"在 App 中打开"两种方式。
fn handle_mobile_qr(url:&str,state:&Arc<Mutex<SyncState>>)->Resp{
    let query=url.split('?').nth(1).unwrap_or("");
    let mut code=""; let mut key_b64="";
    for kv in query.split('&'){
        let mut it=kv.splitn(2,'=');
        let (k,v)=(it.next().unwrap_or(""),it.next().unwrap_or(""));
        match k{"c"=>code=v,"k"=>key_b64=v,_=>{}}
    }
    let key_b64=pct_decode(key_b64);
    let valid=match state.lock(){
        Ok(st)=>st.pair_code.as_ref().map(|(c,exp)|c==code&&now_ms()<=*exp).unwrap_or(false),
        Err(_)=>false,
    };
    if !valid||key_b64.is_empty(){
        let html=String::from("<!DOCTYPE html><html lang=\"zh\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>LocalVault 配对</title><style>body{font-family:system-ui,-apple-system,sans-serif;background:#f6f7f9;display:flex;justify-content:center;padding:40px 16px;margin:0} .card{background:#fff;border-radius:16px;padding:28px;max-width:420px;width:100%;box-shadow:0 2px 12px rgba(0,0,0,.06)} h1{font-size:20px;margin:0 0 12px} p{color:#555;line-height:1.6;font-size:14px;margin:8px 0} .warn{background:#fff3cd;border-radius:8px;padding:12px;color:#856404;font-size:13px;margin:16px 0}</style></head><body><div class=\"card\"><h1>🔐 LocalVault 配对</h1><div class=\"warn\">配对码无效或已过期（5 分钟内有效）。请在桌面端 LocalVault「手机同步」弹窗中重新生成配对二维码。</div></div></body></html>");
        return build_response(200,vec![Header::from_bytes(&b"Content-Type"[..],&b"text/html; charset=utf-8"[..]).unwrap()],html.into_bytes())
    }
    let ip=lan_ips().first().cloned().unwrap_or_else(||"127.0.0.1".to_string());
    let app_url=format!("localvault://sync?ip={ip}&port={SYNC_PORT}&code={code}&key={key_b64}");
    let html=format!(r#"<!DOCTYPE html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>LocalVault 配对</title><style>body{{font-family:system-ui,-apple-system,sans-serif;background:#f6f7f9;display:flex;justify-content:center;padding:40px 16px;margin:0}} .card{{background:#fff;border-radius:16px;padding:28px;max-width:420px;width:100%;box-shadow:0 2px 12px rgba(0,0,0,.06)}} h1{{font-size:20px;margin:0 0 12px}} p{{color:#555;line-height:1.6;font-size:14px;margin:8px 0}} ol{{color:#444;font-size:14px;line-height:1.9;padding-left:20px}} .urlbox{{background:#f2f4f7;border-radius:8px;padding:10px 12px;font-size:12px;word-break:break-all;color:#333;margin:12px 0}} .btn{{display:block;width:100%;text-align:center;background:#2563eb;color:#fff;border:none;border-radius:10px;padding:12px;font-size:15px;margin:10px 0;cursor:pointer;text-decoration:none;box-sizing:border-box}} .btn2{{background:#eef2ff;color:#4338ca}} .hint{{font-size:12px;color:#999;text-align:center}}</style></head><body><div class="card"><h1>🔐 LocalVault 手机配对</h1><p>这是一个 LocalVault 局域网配对链接。微信/浏览器无法直接打开 <code>localvault://</code> 协议，请按下面任一方式在手机端 LocalVault App 中完成配对：</p><ol><li>复制下方配对链接，打开手机端 LocalVault App →「同步」→「手动配对」粘贴</li><li>或点击「在 App 中打开」（已安装 LocalVault 时可直接唤起）</li></ol><div class="urlbox" id="url">{app_url}</div><button class="btn" onclick="navigator.clipboard.writeText(document.getElementById('url').textContent).then(()=>{{this.textContent='✅ 已复制'}})">复制配对链接</button><a class="btn btn2" href="{app_url}">在 LocalVault App 中打开</a><p class="hint">配对码 5 分钟内有效，过期请在桌面端重新生成</p></div></body></html>"#);
    let mut h=vec![Header::from_bytes(&b"Content-Type"[..],&b"text/html; charset=utf-8"[..]).unwrap()];
    h.extend(cors_headers());
    build_response(200,h,html.into_bytes())
}

fn route(method:&Method,path:&str,body:&[u8],state:&Arc<Mutex<SyncState>>,vault:&Arc<Mutex<VaultManager>>)->Resp{
    match path{
        "/api/sync/health"=>{
            if *method!=Method::Get{
                return json_response(405,&serde_json::json!({"error":"method not allowed"}));
            }
            let (running,paired)=match state.lock(){
                Ok(st)=>(st.running,st.paired.len()),
                Err(_)=>(false,0),
            };
            let vault_unlocked=vault.lock().map(|v|v.is_unlocked()).unwrap_or(false);
            json_response(200,&serde_json::json!({
                "running":running,
                "port":SYNC_PORT,
                "vaultUnlocked":vault_unlocked,
                "pairedCount":paired,
                "ip":lan_ips().first().cloned().unwrap_or_else(||"127.0.0.1".to_string()),
            }))
        }
        "/api/sync/pair"=>{
            if *method!=Method::Post{return json_response(405,&serde_json::json!({"error":"method not allowed"}));}
            handle_pair(body,state)
        }
        "/api/sync/pull"=>{
            if *method!=Method::Post{return json_response(405,&serde_json::json!({"error":"method not allowed"}));}
            handle_pull(body,state,vault)
        }
        "/api/sync/push"=>{
            if *method!=Method::Post{return json_response(405,&serde_json::json!({"error":"method not allowed"}));}
            handle_push(body,state,vault)
        }
        "/api/sync/unpair"=>{
            if *method!=Method::Post{return json_response(405,&serde_json::json!({"error":"method not allowed"}));}
            handle_unpair(body,state)
        }
        _=>json_response(404,&serde_json::json!({"error":"not found"})),
    }
}

#[derive(Deserialize)]
struct PairReq{
    code:String,
    #[serde(default)]
    name:Option<String>,
}

fn handle_pair(body:&[u8],state:&Arc<Mutex<SyncState>>)->Resp{
    let key=match state.lock(){
        Ok(st)=>st.pair_key,
        Err(_)=>return json_response(500,&serde_json::json!({"error":"内部状态异常"})),
    };
    let Some(key)=key else{
        return json_response(403,&serde_json::json!({"error":"当前没有待配对的会话，请先在软件内生成配对二维码"}));
    };
    let Ok(plain)=decrypt_body(&key,"/api/sync/pair",body) else{
        return json_response(403,&serde_json::json!({"error":"配对数据解密失败"}));
    };
    let Ok(req)=serde_json::from_slice::<PairReq>(&plain) else{
        return json_response(400,&serde_json::json!({"error":"请求格式错误"}));
    };
    let code=req.code.trim().to_string();
    if code.chars().count()!=6||!code.chars().all(|c|c.is_ascii_digit()){
        return json_response(400,&serde_json::json!({"error":"配对码应为 6 位数字"}));
    }
    let Ok(mut st)=state.lock() else{
        return json_response(500,&serde_json::json!({"error":"内部状态异常"}));
    };
    let now=now_ms();
    // 一次性：无论成功失败都消费当前配对码
    let issued=st.pair_code.take();
    match issued{
        Some((valid,expires)) if valid==code&&now<=expires=>{
            let token=gen_token();
            let device=SyncDevice{
                id:gen_device_id(),
                name:req.name.unwrap_or_else(||"安卓手机".into()),
                paired_at:now,
            };
            st.token=Some(token.clone());
            st.paired.push(device.clone());
            // 持久化配对（含传输密钥）：解锁/重启后自动恢复，手机无需重复输入配对码
            let key_b64=base64::Engine::encode(&base64::engine::general_purpose::STANDARD,key);
            let _=save_pairing(&token,&device,&key_b64);
            let resp_body=serde_json::json!({"token":token,"device":device});
            match encrypt_body(&key,"/api/sync/pair",&serde_json::to_vec(&resp_body).unwrap_or_default()){
                Ok(enc)=>json_response(200,&serde_json::json!({"data":enc})),
                Err(_)=>json_response(500,&serde_json::json!({"error":"响应加密失败"})),
            }
        }
        _=>json_response(403,&serde_json::json!({"error":"配对码无效或已过期"})),
    }
}

#[derive(Deserialize)]
struct PullReq{
    token:String,
    #[serde(default)]
    since:u64,
}

fn handle_pull(body:&[u8],state:&Arc<Mutex<SyncState>>,vault:&Arc<Mutex<VaultManager>>)->Resp{
    let Some(key)=state.lock().ok().and_then(|st|st.pair_key) else{
        return json_response(403,&serde_json::json!({"error":"同步会话未建立"}));
    };
    let Ok(plain)=decrypt_body(&key,"/api/sync/pull",body) else{
        return json_response(401,&serde_json::json!({"error":"数据解密失败"}));
    };
    let Ok(req)=serde_json::from_slice::<PullReq>(&plain) else{
        return json_response(400,&serde_json::json!({"error":"请求格式错误"}));
    };
    if !is_valid_token(state,&req.token){
        return json_response(401,&serde_json::json!({"error":"访问令牌无效"}));
    }
    let entries=match vault.lock().map_err(|_|"state lock poisoned".to_string()).and_then(|v|v.list_entries_full()){
        Ok(xs)=>xs,
        Err(_)=>return json_response(503,&serde_json::json!({"error":"Vault 未解锁或不可用"})),
    };
    let categories=match vault.lock().map_err(|_|"state lock poisoned".to_string()).and_then(|v|v.list_categories()){
        Ok(xs)=>xs,
        Err(_)=>return json_response(503,&serde_json::json!({"error":"Vault 未解锁或不可用"})),
    };
    let since=i64::try_from(req.since).unwrap_or(i64::MAX);
    let filtered:Vec<SyncEntry>=entries
        .into_iter()
        .filter(|e|e.updated_at>=since)
        .map(|e|SyncEntry::from(&e))
        .collect();
    // 手机端找回密码用的恢复材料（电脑端设置了密保时下发；未设置则为 null）
    let recovery=vault.lock().ok().and_then(|v|v.recovery_material().ok()).flatten();
    let resp_body=serde_json::json!({
        "entries":filtered,
        "categories":categories,
        "serverTime":now_ms(),
        "recovery":recovery,
    });
    match encrypt_body(&key,"/api/sync/pull",&serde_json::to_vec(&resp_body).unwrap_or_default()){
        Ok(enc)=>json_response(200,&serde_json::json!({"data":enc})),
        Err(_)=>json_response(500,&serde_json::json!({"error":"响应加密失败"})),
    }
}

#[derive(Deserialize)]
struct PushReq{
    token:String,
    #[serde(default)]
    entries:Vec<SyncEntry>,
    #[serde(default)]
    deleted_ids:Vec<String>,
    #[serde(default)]
    category_ops:Vec<CategoryOp>,
}

fn handle_push(body:&[u8],state:&Arc<Mutex<SyncState>>,vault:&Arc<Mutex<VaultManager>>)->Resp{
    let Some(key)=state.lock().ok().and_then(|st|st.pair_key) else{
        return json_response(403,&serde_json::json!({"error":"同步会话未建立"}));
    };
    let Ok(plain)=decrypt_body(&key,"/api/sync/push",body) else{
        return json_response(401,&serde_json::json!({"error":"数据解密失败"}));
    };
    let Ok(req)=serde_json::from_slice::<PushReq>(&plain) else{
        return json_response(400,&serde_json::json!({"error":"请求格式错误"}));
    };
    if !is_valid_token(state,&req.token){
        return json_response(401,&serde_json::json!({"error":"访问令牌无效"}));
    }
    let entries:Vec<Entry>=req.entries.into_iter().map(Entry::from).collect();
    let apply=match vault.lock().map_err(|_|"state lock poisoned".to_string()).and_then(|mut v|{
        v.sync_apply(&entries,&req.deleted_ids)?;
        v.apply_category_ops(&req.category_ops)
    }){
        Ok(_)=>(),
        Err(_)=>return json_response(503,&serde_json::json!({"error":"Vault 未解锁或写入失败"})),
    };
    let _=apply;
    // 记录同步日志（提示级，不含密码）
    {
        if let Ok(mut st)=state.lock(){
            // 通知桌面端前端：手机端有数据推送，解锁态下自动刷新列表（无需锁定再解锁）
            if let Some(app)=&st.app{
                let _=app.emit("sync-updated",());
            }
            let now=now_ms();
            let device=st.paired.iter().find(|d|st.token.as_deref().is_some()).map(|d|d.name.clone()).unwrap_or_else(||"手机端".into());
            for e in &entries{
                st.sync_logs.push(SyncLog{
                    time:now,device:device.clone(),action:"upsert".into(),
                    name:e.name.clone(),kind:e.entry_type.clone(),
                });
            }
            for id in &req.deleted_ids{
                st.sync_logs.push(SyncLog{
                    time:now,device:device.clone(),action:"delete".into(),
                    name:id.chars().rev().take(6).collect::<String>().chars().rev().collect(),
                    kind:"".into(),
                });
            }
            if st.sync_logs.len()>100{
                let excess=st.sync_logs.len()-100;
                st.sync_logs.drain(0..excess);
            }
        }
    }
    let resp_body=serde_json::json!({
        "ok":true,
        "serverTime":now_ms(),
        "accepted":entries.len(),
    });
    match encrypt_body(&key,"/api/sync/push",&serde_json::to_vec(&resp_body).unwrap_or_default()){
        Ok(enc)=>json_response(200,&serde_json::json!({"data":enc})),
        Err(_)=>json_response(500,&serde_json::json!({"error":"响应加密失败"})),
    }
}

fn handle_unpair(body:&[u8],state:&Arc<Mutex<SyncState>>)->Resp{
    let Some(key)=state.lock().ok().and_then(|st|st.pair_key) else{
        return json_response(403,&serde_json::json!({"error":"同步会话未建立"}));
    };
    let Ok(plain)=decrypt_body(&key,"/api/sync/unpair",body) else{
        return json_response(401,&serde_json::json!({"error":"数据解密失败"}));
    };
    #[derive(Deserialize)]
    struct UnpairReq{token:String}
    let Ok(req)=serde_json::from_slice::<UnpairReq>(&plain) else{
        return json_response(400,&serde_json::json!({"error":"请求格式错误"}));
    };
    let Ok(mut st)=state.lock() else{
        return json_response(500,&serde_json::json!({"error":"内部状态异常"}));
    };
    if st.token.as_deref()!=Some(req.token.as_str()){
        return json_response(401,&serde_json::json!({"error":"访问令牌无效"}));
    }
    st.token=None;
    st.paired.clear();
    drop(st);
    clear_pairing();
    let resp_body=serde_json::json!({"ok":true});
    match encrypt_body(&key,"/api/sync/unpair",&serde_json::to_vec(&resp_body).unwrap_or_default()){
        Ok(enc)=>json_response(200,&serde_json::json!({"data":enc})),
        Err(_)=>json_response(500,&serde_json::json!({"error":"响应加密失败"})),
    }
}

fn is_valid_token(state:&Mutex<SyncState>,token:&str)->bool{
    match state.lock(){
        Ok(st)=>st.token.as_deref()==Some(token),
        Err(_)=>false,
    }
}

// ---------- 局域网 IP 枚举 ----------

#[cfg(target_os="windows")]
fn lan_ips()->Vec<String>{
    use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW,NO_ERROR};
    use windows::Win32::NetworkManagement::IpHelper::{GetAdaptersAddresses,GAA_FLAG_SKIP_ANYCAST,GAA_FLAG_SKIP_DNS_SERVER,GAA_FLAG_SKIP_MULTICAST,IP_ADAPTER_ADDRESSES_LH,IP_ADAPTER_UNICAST_ADDRESS_LH};
    use windows::Win32::Networking::WinSock::{AF_INET,AF_UNSPEC,SOCKADDR_IN};
    let flags=GAA_FLAG_SKIP_ANYCAST|GAA_FLAG_SKIP_MULTICAST|GAA_FLAG_SKIP_DNS_SERVER;
    let family=AF_UNSPEC.0 as u32;
    let mut out=Vec::new();
    unsafe{
        let mut size:u32=0;
        let first=GetAdaptersAddresses(family,flags,None,None,&mut size);
        if first!=NO_ERROR.0&&first!=ERROR_BUFFER_OVERFLOW.0{return out;}
        let mut buf=vec![0u8;size as usize];
        let p=buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(family,flags,None,Some(p),&mut size)!=NO_ERROR.0{return out;}
        let mut cur:*mut IP_ADAPTER_ADDRESSES_LH=p;
        while !cur.is_null(){
            let mut ua:*mut IP_ADAPTER_UNICAST_ADDRESS_LH=(*cur).FirstUnicastAddress;
            while !ua.is_null(){
                let sa=&*(*ua).Address.lpSockaddr;
                if (*sa).sa_family==AF_INET{
                    let sin=&*(sa as *const _ as *const SOCKADDR_IN);
                    let host=u32::from_be(sin.sin_addr.S_un.S_addr);
                    let ip=std::net::Ipv4Addr::from(host).to_string();
                    if !ip.starts_with("127.")&&!ip.starts_with("169.254."){out.push(ip);}
                }
                ua=(*ua).Next;
            }
            cur=(*cur).Next;
        }
    }
    out.sort();
    out.dedup();
    out
}
#[cfg(not(target_os="windows"))]
fn lan_ips()->Vec<String>{Vec::new()}

// ---------- 工具函数 ----------

fn secure_random<const N:usize>()->[u8;N]{
    let mut x=[0u8;N];
    OsRng.fill_bytes(&mut x);
    x
}
fn gen_pair_code()->String{
    let r=secure_random::<3>();
    let n=u32::from_be_bytes([0,r[0],r[1],r[2]])%1_000_000;
    format!("{n:06}")
}
fn gen_token()->String{
    hex::encode(secure_random::<16>())
}
fn gen_device_id()->String{
    hex::encode(secure_random::<8>())
}
fn now_ms()->u64{
    use std::time::{SystemTime,UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn encrypt_roundtrip(){
        let key=[7u8;KEY_LEN];
        let enc=encrypt_body(&key,"/api/sync/pull",b"hello sync").unwrap();
        let dec=decrypt_body(&key,"/api/sync/pull",enc.as_bytes()).unwrap();
        assert_eq!(dec,b"hello sync");
        // 错误路径/AAD 不匹配时解密必须失败
        assert!(decrypt_body(&key,"/api/sync/push",enc.as_bytes()).is_err());
    }

    #[test]
    fn pair_code_format(){
        for _ in 0..50{
            let c=gen_pair_code();
            assert_eq!(c.len(),6);
            assert!(c.chars().all(|x|x.is_ascii_digit()));
        }
    }

    #[test]
    fn sync_entry_roundtrip(){
        let e=Entry{
            id:"abc".into(),seq:1,entry_type:"网站".into(),name:"GitHub".into(),
            username:"u".into(),email:"".into(),phone:"".into(),password:"secret".into(),
            nickname:"".into(),url:"https://github.com".into(),notes:"".into(),
            category:"默认".into(),tags:vec![],favorite:false,updated_at:123,
            expires_at:None,password_encrypted:None,password_score:None,password_reused:None,
        };
        let se=SyncEntry::from(&e);
        assert_eq!(se.password,"secret");
        let back:Entry=se.into();
        assert_eq!(back.password,"secret");
        assert_eq!(back.id,"abc");
    }

    /// 核心生命周期回归：start → stop → start（对应 锁定→解锁 循环）。
    /// 验证 stop 后监听端口真正释放、再次 start 能重新绑定同一端口。
    #[test]
    fn start_stop_restart_cycle(){
        let vault=Arc::new(Mutex::new(VaultManager::new()));
        let mut b=SyncBridge::new();

        // 第一次启动：端口应可连接
        assert!(b.start(vault.clone()).is_ok(),"首次启动失败");
        assert!(b.running);
        let probe=std::net::TcpStream::connect_timeout(
            &"127.0.0.1:38528".parse().unwrap(),
            std::time::Duration::from_millis(500),
        );
        assert!(probe.is_ok(),"启动后 38528 应可连接");

        // 停止：端口必须释放
        b.stop();
        assert!(!b.running,"stop 后 running 应为 false");
        // 此刻应能立即重新绑定同一端口（证明 socket 已关闭；与生产一致使用 SO_REUSEADDR）
        let rebind=create_reuse_listener("0.0.0.0:38528");
        assert!(rebind.is_ok(),"stop 后端口未释放，无法重新绑定: {:?}",rebind.err());
        drop(rebind);

        // 再次启动（模拟锁定再解锁）：应能重新绑定并恢复服务
        assert!(b.start(vault.clone()).is_ok(),"重启失败（锁定再解锁后服务未恢复）");
        assert!(b.running);
        let probe2=std::net::TcpStream::connect_timeout(
            &"127.0.0.1:38528".parse().unwrap(),
            std::time::Duration::from_millis(500),
        );
        assert!(probe2.is_ok(),"重启后 38528 应可连接");

        // 清理：停止并释放
        b.stop();
        let rebind2=create_reuse_listener("0.0.0.0:38528");
        assert!(rebind2.is_ok(),"最终 stop 后端口未释放");
        drop(rebind2);
    }
}
