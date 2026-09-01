use argon2::{Algorithm,Argon2,Params,Version};
use chacha20poly1305::{aead::{Aead,OsRng,Payload},KeyInit,XChaCha20Poly1305,XNonce};
use rand_core::RngCore;
use rusqlite::{Connection,params};
use serde::{Deserialize,Serialize};
use std::{fs,path::PathBuf};
use zeroize::Zeroize;

const FORMAT_VERSION:u32=2;
const SALT_LEN:usize=16;
const DEK_LEN:usize=32;
const NONCE_LEN:usize=24;
const ARGON_MEM_KIB:u32=131072;
const ARGON_ITERS:u32=3;
const ARGON_LANES:u32=2;
const EXPORT_MAGIC:&[u8]=b"LVX2";
const EXPORT_V1_MAGIC:&[u8]=b"LVX1";
const MIN_MASTER_PASSWORD_LEN:usize=8;

fn validate_new_master_password(pass:&str)->Result<(),String>{
 if pass.chars().count()<MIN_MASTER_PASSWORD_LEN{return Err("主密码至少 8 位".into())}
 if !pass.chars().any(|c|c.is_ascii_digit()){return Err("主密码必须包含数字".into())}
 if !pass.chars().any(|c|c.is_ascii_lowercase()){return Err("主密码必须包含小写字母".into())}
 if !pass.chars().any(|c|c.is_ascii_uppercase()){return Err("主密码必须包含大写字母".into())}
 if !pass.chars().any(|c|!c.is_ascii_alphanumeric()){return Err("主密码必须包含特殊符号".into())}
 Ok(())
}

fn zero()->i64{0} fn empty()->String{String::new()} fn empty_tags()->Vec<String>{Vec::new()} fn default_type()->String{"网站".into()} fn default_category()->String{"默认".into()} fn default_expiry()->Option<i64>{None}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Entry{
 #[serde(default)] pub id:String,
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
 #[serde(default="default_expiry")] pub expires_at:Option<i64>,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Category{pub name:String,pub icon:String}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HistoryRecord{pub changed_at:i64,pub fields:Vec<String>}
#[derive(Debug,Serialize,Deserialize)]
pub struct BootstrapStatus{pub exists:bool,pub version:u32,pub recovery_enabled:bool}
#[derive(Debug,Serialize,Deserialize)]struct ExportPackage{version:u32,entries:Vec<Entry>}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct BackupData{pub entries:Vec<Entry>,pub categories:Vec<Category>}

pub struct VaultManager{path:PathBuf,dek:Option<[u8;DEK_LEN]>,recovery_pending_dek:Option<[u8;DEK_LEN]>}
impl VaultManager{
 pub fn new()->Self{
  let portable=std::env::var_os("LOCALVAULT_PORTABLE").is_some()||std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("portable.flag"))).map(|p|p.exists()).unwrap_or(false);
  let base=if portable{
    std::env::current_exe().ok().and_then(|p|p.parent().map(|d|d.join("data"))).unwrap_or_else(||PathBuf::from("data"))
  }else{
    dirs::data_local_dir().unwrap_or_else(||PathBuf::from(".")).join("LocalVault")
  };
  Self{path:base.join("vault.db"),dek:None,recovery_pending_dek:None}
}
 pub fn status(&self)->Result<BootstrapStatus,String>{
   if !self.path.exists(){return Ok(BootstrapStatus{exists:false,version:FORMAT_VERSION,recovery_enabled:false})}
   let c=Connection::open(&self.path).map_err(|e|e.to_string())?;self.init_schema(&c)?;
   let v=self.meta(&c,"format_version").ok().and_then(|b|if b.len()==4{Some(u32::from_be_bytes(b.try_into().ok()?))}else{None}).unwrap_or(1);
   Ok(BootstrapStatus{exists:true,version:v,recovery_enabled:self.meta(&c,"recovery_wrapped").is_ok()})
 }
 fn init_schema(&self,c:&Connection)->Result<(),String>{
   c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value BLOB NOT NULL); CREATE TABLE IF NOT EXISTS entries(id TEXT PRIMARY KEY,cipher BLOB NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS categories(name TEXT PRIMARY KEY,icon TEXT NOT NULL DEFAULT '📁',created_at INTEGER NOT NULL,sort_order INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS history(id INTEGER PRIMARY KEY AUTOINCREMENT,entry_id TEXT NOT NULL,changed_at INTEGER NOT NULL,cipher BLOB NOT NULL); CREATE INDEX IF NOT EXISTS idx_history_entry_time ON history(entry_id,changed_at DESC); CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY,cipher BLOB NOT NULL,deleted_at INTEGER NOT NULL,original_seq INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_trash_deleted_at ON trash(deleted_at);").map_err(|e|e.to_string())?;
   let mut st=c.prepare("PRAGMA table_info(categories)").map_err(|e|e.to_string())?;let mut rows=st.query([]).map_err(|e|e.to_string())?;let mut icon=false;while let Some(r)=rows.next().map_err(|e|e.to_string())?{let n:String=r.get(1).map_err(|e|e.to_string())?;if n=="icon"{icon=true;break}}drop(rows);drop(st);if !icon{c.execute("ALTER TABLE categories ADD COLUMN icon TEXT NOT NULL DEFAULT '📁'",[]).map_err(|e|e.to_string())?;}
   Ok(())
 }
 fn db(&self)->Result<Connection,String>{if let Some(p)=self.path.parent(){fs::create_dir_all(p).map_err(|e|e.to_string())?}let c=Connection::open(&self.path).map_err(|e|e.to_string())?;self.init_schema(&c)?;let defaults=[("默认","▦",0i64),("工作","💼",1),("财务","💰",2),("社交","👥",3),("开发","💻",4),("其他","📁",5)];for (n,i,o) in defaults{c.execute("INSERT OR IGNORE INTO categories(name,icon,created_at,sort_order) VALUES(?1,?2,strftime('%s','now'),?3)",params![n,i,o]).map_err(|e|e.to_string())?;}Ok(c)}
 fn random<const N:usize>()->[u8;N]{let mut x=[0u8;N];OsRng.fill_bytes(&mut x);x}
 fn derive(password:&str,salt:&[u8;SALT_LEN])->Result<[u8;32],String>{let p=Params::new(ARGON_MEM_KIB,ARGON_ITERS,ARGON_LANES,Some(32)).map_err(|e|e.to_string())?;let a=Argon2::new(Algorithm::Argon2id,Version::V0x13,p);let mut out=[0u8;32];a.hash_password_into(password.as_bytes(),salt,&mut out).map_err(|e|e.to_string())?;Ok(out)}
 fn enc(key:&[u8;32],plain:&[u8],aad:&[u8])->Result<Vec<u8>,String>{let c=XChaCha20Poly1305::new(key.into());let n=Self::random::<24>();let ct=c.encrypt(XNonce::from_slice(&n),Payload{msg:plain,aad}).map_err(|e|e.to_string())?;let mut out=n.to_vec();out.extend_from_slice(&ct);Ok(out)}
 fn dec(key:&[u8;32],data:&[u8],aad:&[u8])->Result<Vec<u8>,String>{if data.len()<NONCE_LEN+16{return Err("invalid ciphertext".into())}let c=XChaCha20Poly1305::new(key.into());c.decrypt(XNonce::from_slice(&data[..NONCE_LEN]),Payload{msg:&data[NONCE_LEN..],aad}).map_err(|_|"authentication failed".into())}
 fn meta(&self,c:&Connection,k:&str)->Result<Vec<u8>,String>{c.query_row("SELECT value FROM meta WHERE key=?1",params![k],|r|r.get(0)).map_err(|e|e.to_string())}
 fn salt(&self,c:&Connection)->Result<[u8;16],String>{let b=self.meta(c,"salt")?;if b.len()!=16{return Err("invalid salt".into())}let mut s=[0u8;16];s.copy_from_slice(&b);Ok(s)}
 fn salt_from_connection(&self,c:&Connection)->Result<[u8;16],String>{let b=self.meta(c,"salt")?;if b.len()!=SALT_LEN{return Err("invalid salt".into())}let mut s=[0u8;SALT_LEN];s.copy_from_slice(&b);Ok(s)}
 pub fn create(&mut self,pass:&str,confirm:&str)->Result<(),String>{validate_new_master_password(pass)?;if pass!=confirm{return Err("两次输入的主密码不一致".into())}if self.path.exists(){return Err("Vault 已存在".into())}let c=self.db()?;let salt=Self::random::<16>();let dek=Self::random::<32>();let kek=Self::derive(pass,&salt)?;let wrapped=Self::enc(&kek,&dek,b"LocalVault|wrapped_dek|v1")?;c.execute("INSERT INTO meta(key,value)VALUES('format_version',?),('salt',?),('wrapped_dek',?)",params![FORMAT_VERSION.to_be_bytes().to_vec(),salt.to_vec(),wrapped]).map_err(|e|e.to_string())?;self.dek=Some(dek);Ok(())}
 pub fn unlock(&mut self,pass:&str)->Result<Vec<Entry>,String>{let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"wrapped_dek")?;let kek=Self::derive(pass,&salt)?;let raw=Self::dec(&kek,&w,b"LocalVault|wrapped_dek|v1").map_err(|_|"bad credentials".to_string())?;if raw.len()!=32{return Err("invalid DEK".into())}let mut d=[0u8;32];d.copy_from_slice(&raw);self.dek=Some(d);self.read_entries(&c,&d)}
 fn read_entries(&self,c:&Connection,dek:&[u8;32])->Result<Vec<Entry>,String>{let mut st=c.prepare("SELECT id,cipher FROM entries ORDER BY updated_at DESC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for row in rows{let(id,cipher)=row.map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",id);let plain=Self::dec(dek,&cipher,aad.as_bytes())?;let mut e:Entry=serde_json::from_slice(&plain).map_err(|_|"entry decode failed".to_string())?;e.id=id;out.push(e)}Ok(out)}
 fn changed_fields(a:&Entry,b:&Entry)->Vec<String>{let mut f=Vec::new();if a.entry_type!=b.entry_type{f.push("类型".into())}if a.name!=b.name{f.push("密码名称".into())}if a.username!=b.username{f.push("用户名".into())}if a.email!=b.email{f.push("邮箱".into())}if a.phone!=b.phone{f.push("手机号".into())}if a.password!=b.password{f.push("密码".into())}if a.nickname!=b.nickname{f.push("平台昵称".into())}if a.url!=b.url{f.push("网址/IP/APP".into())}if a.notes!=b.notes{f.push("备注".into())}if a.category!=b.category{f.push("分类".into())}if a.tags!=b.tags{f.push("标签".into())}if a.favorite!=b.favorite{f.push("收藏状态".into())}if a.expires_at!=b.expires_at{f.push("密码过期设置".into())}f}
 pub fn save(&mut self,entries:&[Entry])->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;let old_path=self.path.clone();let tmp=old_path.with_extension("new");if tmp.exists(){let _=fs::remove_file(&tmp);}let c=Connection::open(&tmp).map_err(|e|e.to_string())?;self.init_schema(&c)?;let old=Connection::open(&old_path).map_err(|e|e.to_string())?;self.init_schema(&old)?;let salt=self.salt(&old)?;let w=self.meta(&old,"wrapped_dek")?;let recovery=self.meta(&old,"recovery_wrapped").ok();let questions=self.meta(&old,"recovery_questions").ok();c.execute("INSERT INTO meta VALUES('format_version',?),('salt',?),('wrapped_dek',?)",params![FORMAT_VERSION.to_be_bytes().to_vec(),salt.to_vec(),w]).map_err(|e|e.to_string())?;if let Some(x)=recovery{c.execute("INSERT INTO meta VALUES('recovery_wrapped',?)",params![x]).map_err(|e|e.to_string())?;}if let Some(x)=questions{c.execute("INSERT INTO meta VALUES('recovery_questions',?)",params![x]).map_err(|e|e.to_string())?;}
   {let mut st=old.prepare("SELECT name,icon,created_at,sort_order FROM categories ORDER BY sort_order").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(|e|e.to_string())?;for r in rows{let(n,i,ca,so)=r.map_err(|e|e.to_string())?;c.execute("INSERT OR REPLACE INTO categories(name,icon,created_at,sort_order)VALUES(?,?,?,?)",params![n,i,ca,so]).map_err(|e|e.to_string())?;} }
   {let mut st=old.prepare("SELECT entry_id,changed_at,cipher FROM history").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).map_err(|e|e.to_string())?;for r in rows{let(e,t,ct)=r.map_err(|e|e.to_string())?;c.execute("INSERT INTO history(entry_id,changed_at,cipher)VALUES(?,?,?)",params![e,t,ct]).map_err(|e|e.to_string())?;} }
   {let mut st=old.prepare("SELECT id,cipher,deleted_at,original_seq FROM trash").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(|e|e.to_string())?;for r in rows{let(id,ct,t,seq)=r.map_err(|e|e.to_string())?;c.execute("INSERT INTO trash(id,cipher,deleted_at,original_seq)VALUES(?,?,?,?)",params![id,ct,t,seq]).map_err(|e|e.to_string())?;} }
   let old_entries=self.read_entries(&old,&dek)?;let old_map=old_entries.iter().map(|e|(e.id.clone(),e)).collect::<std::collections::HashMap<_,_>>();let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;for e in entries{if let Some(prev)=old_map.get(&e.id){let fields=Self::changed_fields(prev,e);if !fields.is_empty(){let p=serde_json::to_vec(&fields).map_err(|e|e.to_string())?;let aad=format!("LocalVault|history|{}|v1",e.id);let ct=Self::enc(&dek,&p,aad.as_bytes())?;tx.execute("INSERT INTO history(entry_id,changed_at,cipher)VALUES(?,?,?)",params![e.id,now_ms(),ct]).map_err(|e|e.to_string())?;tx.execute("DELETE FROM history WHERE entry_id=?1 AND id NOT IN (SELECT id FROM history WHERE entry_id=?1 ORDER BY changed_at DESC,id DESC LIMIT 3)",params![e.id]).map_err(|e|e.to_string())?;}}
     let p=serde_json::to_vec(e).map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",e.id);let ct=Self::enc(&dek,&p,aad.as_bytes())?;tx.execute("INSERT INTO entries(id,cipher,updated_at)VALUES(?,?,?)",params![e.id,ct,e.updated_at]).map_err(|e|e.to_string())?;}tx.commit().map_err(|e|e.to_string())?;drop(old);drop(c);if old_path.exists(){let bak=old_path.with_extension("bak");let _=fs::copy(&old_path,&bak);}fs::rename(&tmp,&old_path).map_err(|e|e.to_string())?;Ok(())}
 pub fn list_categories(&self)->Result<Vec<Category>,String>{let c=self.db()?;let mut st=c.prepare("SELECT name,icon FROM categories ORDER BY sort_order ASC,created_at ASC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok(Category{name:r.get(0)?,icon:r.get(1)?})).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{out.push(r.map_err(|e|e.to_string())?)}Ok(out)}
 pub fn create_category(&self,name:&str,icon:&str)->Result<(),String>{let n=name.trim();if n.is_empty(){return Err("分类名称不能为空".into())}if n.chars().count()>40{return Err("分类名称过长".into())}let c=self.db()?;if c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![n],|r|r.get(0)).is_ok(){return Err("分类已经存在".into())}let order:i64=c.query_row("SELECT COALESCE(MAX(sort_order),-1)+1 FROM categories",[],|r|r.get(0)).map_err(|e|e.to_string())?;c.execute("INSERT INTO categories(name,icon,created_at,sort_order)VALUES(?1,?2,strftime('%s','now'),?3)",params![n,if icon.trim().is_empty(){"📁"}else{icon.trim()},order]).map_err(|e|e.to_string())?;Ok(())}
 pub fn move_to_trash(&mut self,entry_id:&str,retention_days:i64)->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let row=c.query_row("SELECT cipher FROM entries WHERE id=?1",params![entry_id],|r|r.get::<_,Vec<u8>>(0)).map_err(|_|"密码条目不存在".to_string())?;let aad=format!("LocalVault|entry|{}|v1",entry_id);let plain=Self::dec(&dek,&row,&aad.as_bytes())?;let e:Entry=serde_json::from_slice(&plain).map_err(|_|"entry decode failed".to_string())?;let deleted_at=now_ms();let _retention=(retention_days.max(7).min(30))*86400000;c.execute("INSERT OR REPLACE INTO trash(id,cipher,deleted_at,original_seq)VALUES(?1,?2,?3,?4)",params![entry_id,row,deleted_at,e.seq]).map_err(|e|e.to_string())?;Ok(())}
 pub fn list_trash(&self,retention_days:i64)->Result<Vec<Entry>,String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let days=retention_days.max(7).min(30);let cutoff=now_ms()-days*86400000;c.execute("DELETE FROM trash WHERE deleted_at<?1",params![cutoff]).map_err(|e|e.to_string())?;let mut st=c.prepare("SELECT id,cipher FROM trash ORDER BY deleted_at DESC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{let(id,cipher)=r.map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",id);let p=Self::dec(&dek,&cipher,aad.as_bytes())?;let mut e:Entry=serde_json::from_slice(&p).map_err(|_|"trash decode failed".to_string())?;e.id=id;out.push(e)}Ok(out)}
 pub fn restore_trash(&mut self,entry_id:&str)->Result<Vec<Entry>,String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let row=c.query_row("SELECT cipher FROM trash WHERE id=?1",params![entry_id],|r|r.get::<_,Vec<u8>>(0)).map_err(|_|"回收站中不存在该条目".to_string())?;let aad=format!("LocalVault|entry|{}|v1",entry_id);let p=Self::dec(&dek,&row,&aad.as_bytes())?;let e:Entry=serde_json::from_slice(&p).map_err(|_|"trash decode failed".to_string())?;let mut entries=self.read_entries(&c,&dek)?;if entries.iter().any(|x|x.id==entry_id){return Err("当前 Vault 已存在同一条目".into())}entries.push(e);c.execute("DELETE FROM trash WHERE id=?1",params![entry_id]).map_err(|e|e.to_string())?;drop(c);self.save(&entries)?;Ok(entries)}
 pub fn purge_trash(&mut self,entry_id:Option<String>)->Result<(),String>{let _=self.dek.ok_or("Vault locked")?;let c=self.db()?;match entry_id{Some(id)=>{c.execute("DELETE FROM trash WHERE id=?1",params![id]).map_err(|e|e.to_string())?;},None=>{c.execute("DELETE FROM trash",[]).map_err(|e|e.to_string())?;}}Ok(())}
 pub fn update_category(&self,old_name:&str,name:&str,icon:&str)->Result<(),String>{let old=old_name.trim();let n=name.trim();if old=="默认"&&n!="默认"{return Err("默认分类不能重命名".into())}if n.is_empty(){return Err("分类名称不能为空".into())}let c=self.db()?;if old!=n&&c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![n],|r|r.get(0)).is_ok(){return Err("分类已经存在".into())}c.execute("UPDATE categories SET name=?1,icon=?2 WHERE name=?3",params![n,if icon.trim().is_empty(){"📁"}else{icon.trim()},old]).map_err(|e|e.to_string())?;Ok(())}
 pub fn delete_category(&self,name:&str)->Result<(),String>{let n=name.trim();if n=="默认"{return Err("默认分类不能删除".into())}let c=self.db()?;c.execute("DELETE FROM categories WHERE name=?1",params![n]).map_err(|e|e.to_string())?;Ok(())}
 pub fn history_list(&self,entry_id:&str)->Result<Vec<HistoryRecord>,String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let mut st=c.prepare("SELECT changed_at,cipher FROM history WHERE entry_id=?1 ORDER BY changed_at DESC,id DESC LIMIT 3").map_err(|e|e.to_string())?;let rows=st.query_map(params![entry_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{let(t,ct)=r.map_err(|e|e.to_string())?;let aad=format!("LocalVault|history|{}|v1",entry_id);let p=Self::dec(&dek,&ct,aad.as_bytes())?;let fields:Vec<String>=serde_json::from_slice(&p).map_err(|_|"history decode failed".to_string())?;out.push(HistoryRecord{changed_at:t,fields})}Ok(out)}
 pub fn export_entries(&mut self,entries:&[Entry],destination:&str,master_password:&str)->Result<(),String>{
   if master_password.chars().count()<MIN_MASTER_PASSWORD_LEN{return Err("主密码至少 8 位".into())}
   let dek=self.dek.ok_or("Vault locked")?;
   let c=self.db()?;
   let salt=self.salt(&c)?;
   let wrapped=self.meta(&c,"wrapped_dek")?;
   let kek=Self::derive(master_password,&salt)?;
   let raw=Self::dec(&kek,&wrapped,b"LocalVault|wrapped_dek|v1").map_err(|_|"原 Vault 主密码错误，导出已取消".to_string())?;
   if raw.len()!=DEK_LEN{return Err("原 Vault 密钥无效".into())}
   if raw.as_slice()!=dek.as_slice(){return Err("当前 Vault 密钥校验失败，导出已取消".into())}
   let payload=serde_json::to_vec(&ExportPackage{version:2,entries:entries.to_vec()}).map_err(|e|e.to_string())?;
   let ct=Self::enc(&dek,&payload,b"LocalVault|export|v2")?;
   let mut out=EXPORT_MAGIC.to_vec();
   out.extend_from_slice(&salt);
   out.extend_from_slice(&wrapped);
   out.extend_from_slice(&ct);
   fs::write(destination,out).map_err(|e|e.to_string())?;
   Ok(())
 }
 pub fn import_entries(&mut self,source:&str,source_master_password:&str)->Result<Vec<Entry>,String>{
   if source_master_password.chars().count()<MIN_MASTER_PASSWORD_LEN{return Err("主密码至少 8 位".into())}
   let b=fs::read(source).map_err(|e|e.to_string())?;
   if b.len()<4||&b[..4]!=EXPORT_MAGIC{
     if b.len()>=4&&&b[..4]==EXPORT_V1_MAGIC{return Err("这是旧版 LVX1 导出文件。为避免把当前 Vault 密钥误当成源 Vault 密钥，旧版导出暂不支持跨 Vault 导入；请在原电脑重新导出一次。".into())}
     return Err("不是有效的 LocalVault 可迁移加密数据文件，禁止导入明文文件".into())
   }
   let header=4+SALT_LEN+72;
   if b.len()<header+NONCE_LEN+16{return Err("导出文件不完整或已损坏".into())}
   let mut salt=[0u8;SALT_LEN];salt.copy_from_slice(&b[4..4+SALT_LEN]);
   let wrapped=&b[4+SALT_LEN..header];
   let kek=Self::derive(source_master_password,&salt)?;
   let raw=Self::dec(&kek,wrapped,b"LocalVault|wrapped_dek|v1").map_err(|_|"原电脑主密码错误，无法解密导入文件".to_string())?;
   if raw.len()!=DEK_LEN{return Err("导出文件中的密钥无效".into())}
   let mut dek=[0u8;DEK_LEN];dek.copy_from_slice(&raw);
   let p=Self::dec(&dek,&b[header..],b"LocalVault|export|v2").map_err(|_|"导出文件认证失败：文件可能被篡改或损坏".to_string())?;
   let pkg:ExportPackage=serde_json::from_slice(&p).map_err(|_|"加密导出文件损坏或版本不兼容".to_string())?;
   if pkg.version!=2{return Err("不支持的导出数据版本".into())}
   Ok(pkg.entries)
 }
 pub fn lock(&mut self)->Result<(),String>{if let Some(mut d)=self.dek.take(){d.zeroize()}if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}Ok(())}
 pub fn backup(&mut self,destination:&str,master_password:&str)->Result<(),String>{let _=self.dek.ok_or("Vault locked")?;if master_password.is_empty(){return Err("请输入当前 Vault 主密码".into())}let _=self.verify_master_dek(master_password)?;if !self.path.exists(){return Err("Vault file does not exist".into())}fs::copy(&self.path,destination).map_err(|e|e.to_string())?;Ok(())}
 fn read_backup_file(&self,backup_path:&str,pass:&str)->Result<([u8;DEK_LEN],BackupData),String>{
   let bp=PathBuf::from(backup_path);
   if !bp.exists(){return Err("备份不存在".into())}
   let c=Connection::open(&bp).map_err(|e|e.to_string())?;
   let salt=self.salt_from_connection(&c)?;
   let wrapped=self.meta(&c,"wrapped_dek")?;
   let kek=Self::derive(pass,&salt)?;
   let raw=Self::dec(&kek,&wrapped,b"LocalVault|wrapped_dek|v1").map_err(|_|"备份验证失败：主密码不正确".to_string())?;
   if raw.len()!=DEK_LEN{return Err("备份中的 Vault 密钥无效".into())}
   let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);
   let entries=self.read_entries(&c,&d).map_err(|e|format!("备份验证失败：{}",e))?;
   let mut st=c.prepare("SELECT name,icon FROM categories ORDER BY sort_order ASC,created_at ASC").map_err(|e|e.to_string())?;
   let rows=st.query_map([],|r|Ok(Category{name:r.get(0)?,icon:r.get(1)?})).map_err(|e|e.to_string())?;
   let mut categories=Vec::new();
   for r in rows{categories.push(r.map_err(|e|e.to_string())?);}
   Ok((d,BackupData{entries,categories}))
 }
 pub fn backup_preview(&mut self,backup_path:&str,pass:&str)->Result<BackupData,String>{
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   let (_d,data)=self.read_backup_file(backup_path,pass)?;
   Ok(data)
 }
 pub fn restore(&mut self,backup_path:&str,pass:&str)->Result<Vec<Entry>,String>{
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   let bp=PathBuf::from(backup_path);
   if !bp.exists(){return Err("备份不存在".into())}
   let tmp=self.path.with_extension("restore");
   if tmp.exists(){let _=fs::remove_file(&tmp);}
   fs::copy(&bp,&tmp).map_err(|e|e.to_string())?;
   let c=Connection::open(&tmp).map_err(|e|e.to_string())?;
   self.init_schema(&c)?;
   let salt=self.salt_from_connection(&c)?;
   let wrapped=self.meta(&c,"wrapped_dek")?;
   let kek=Self::derive(pass,&salt)?;
   let raw=match Self::dec(&kek,&wrapped,b"LocalVault|wrapped_dek|v1"){
     Ok(v)=>v,
     Err(_)=>{drop(c);let _=fs::remove_file(&tmp);return Err("备份验证失败：主密码不正确，原 Vault 未被替换".into())}
   };
   if raw.len()!=DEK_LEN{drop(c);let _=fs::remove_file(&tmp);return Err("备份中的 Vault 密钥无效，原 Vault 未被替换".into())}
   let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);
   let entries=match self.read_entries(&c,&d){Ok(v)=>v,Err(e)=>{drop(c);let _=fs::remove_file(&tmp);return Err(format!("备份验证失败：{}；原 Vault 未被替换",e))}};
   drop(c);
   if self.path.exists(){let bak=self.path.with_extension("pre_restore.bak");let _=fs::copy(&self.path,&bak);}
   fs::rename(&tmp,&self.path).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;
   if let Some(mut old)=self.dek.take(){old.zeroize()}
   self.dek=Some(d);
   Ok(entries)
 }
 pub fn merge_backup(&mut self,backup_path:&str,pass:&str)->Result<Vec<Entry>,String>{
   let _=self.dek.ok_or("Vault locked")?;
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   let (_source_dek,data)=self.read_backup_file(backup_path,pass)?;
   let current_dek=self.dek.ok_or("Vault locked")?;
   let current_db=self.db()?;
   let mut current=self.read_entries(&current_db,&current_dek)?;
   let mut seen_ids=std::collections::HashSet::new();
   for e in &current{seen_ids.insert(e.id.clone());}
   let mut max_seq=current.iter().map(|e|e.seq).max().unwrap_or(0);
   for e in data.entries{
     if seen_ids.contains(&e.id){continue;}
     let mut ne=e;
     max_seq+=1;
     ne.seq=max_seq;
     ne.updated_at=now_ms();
     current.push(ne);
     seen_ids.insert(current.last().map(|x|x.id.clone()).unwrap_or_default());
   }
   drop(current_db);
   for cat in data.categories{
     if cat.name.trim().is_empty(){continue;}
     let c=self.db()?;
     let exists=c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![cat.name],|r|r.get(0)).is_ok();
     drop(c);
     if !exists{self.create_category(&cat.name,&cat.icon)?;}
   }
   self.save(&current)?;
   Ok(current)
 }
 pub fn generate_recovery_code(&mut self)->Result<String,String>{let alphabet=b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";let mut r=[0u8;24];OsRng.fill_bytes(&mut r);let mut out=String::with_capacity(29);for(i,n)in r.iter().enumerate(){if i>0&&i%6==0{out.push('-')}out.push(alphabet[(*n as usize)%alphabet.len()]as char)}Ok(out)}
 pub fn recovery_questions(&self)->Result<Vec<String>,String>{let c=self.db()?;match self.meta(&c,"recovery_questions"){Ok(raw)=>serde_json::from_slice(&raw).map_err(|_|"恢复问题数据损坏".into()),Err(_)=>Ok(vec!["我最喜欢的一本童年读物是什么？".into(),"我自己定义的长期不变短语是什么？".into(),"我记得的第一个特别地点是什么？".into()])}}
 fn verify_master_dek(&self,pass:&str)->Result<[u8;DEK_LEN],String>{if pass.chars().count()<MIN_MASTER_PASSWORD_LEN{return Err("主密码至少 8 位".into())}let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"wrapped_dek")?;let kek=Self::derive(pass,&salt)?;let raw=Self::dec(&kek,&w,b"LocalVault|wrapped_dek|v1").map_err(|_|"主密码错误".to_string())?;if raw.len()!=DEK_LEN{return Err("Vault 密钥无效".into())}let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);if let Some(current)=self.dek{if current!=d{return Err("当前 Vault 密钥校验失败".into())}}Ok(d)}
 pub fn verify_master(&self,pass:&str)->Result<(),String>{let _=self.verify_master_dek(pass)?;Ok(())}
 pub fn enable_recovery(&mut self,code:&str,questions:&[String],answers:&[String])->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;if code.trim().chars().count()<20{return Err("恢复码太短".into())}if questions.len()!=3||answers.len()!=3{return Err("必须有 3 组问题/答案".into())}if answers.iter().any(|x|x.trim().is_empty()){return Err("每个密保答案不能为空".into())}let c=self.db()?;let salt=self.salt(&c)?;let combo=format!("{}\0{}\0{}\0{}",code.trim(),answers[0].trim(),answers[1].trim(),answers[2].trim());let rk=Self::derive(&combo,&salt)?;let wrapped=Self::enc(&rk,&dek,b"LocalVault|recovery|v1")?;let qs=serde_json::to_vec(questions).map_err(|e|e.to_string())?;c.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('recovery_wrapped',?),('recovery_questions',?)",params![wrapped,qs]).map_err(|e|e.to_string())?;Ok(())}
 pub fn recovery_verify(&mut self,code:&str,answers:&[String])->Result<(),String>{if answers.len()!=3{return Err("恢复答案数量错误".into())}let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"recovery_wrapped")?;let combo=format!("{}\0{}\0{}\0{}",code.trim(),answers[0].trim(),answers[1].trim(),answers[2].trim());let rk=Self::derive(&combo,&salt)?;let raw=Self::dec(&rk,&w,b"LocalVault|recovery|v1").map_err(|_|"recovery auth failed".to_string())?;if raw.len()!=DEK_LEN{return Err("invalid recovery key".into())}let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);if let Some(mut old)=self.recovery_pending_dek.take(){old.zeroize()}self.recovery_pending_dek=Some(d);Ok(())}
 pub fn recovery_cancel(&mut self)->Result<(),String>{if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}Ok(())}
 pub fn recovery_set_master(&mut self,new_password:&str,confirm:&str)->Result<(),String>{validate_new_master_password(new_password)?;if new_password!=confirm{return Err("两次输入的新主密码不一致".into())}let d=*self.recovery_pending_dek.as_ref().ok_or("请先完成恢复验证")?;let c=self.db()?;let salt=self.salt(&c)?;let old_wrapped=self.meta(&c,"wrapped_dek")?;let new_kek=Self::derive(new_password,&salt)?;if Self::dec(&new_kek,&old_wrapped,b"LocalVault|wrapped_dek|v1").is_ok(){return Err("新主密码不能与旧主密码相同".into())}let wrapped=Self::enc(&new_kek,&d,b"LocalVault|wrapped_dek|v1")?;c.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('wrapped_dek',?)",params![wrapped]).map_err(|e|e.to_string())?;let _=self.recovery_pending_dek.take();if let Some(mut old)=self.dek.take(){old.zeroize()}Ok(())}

 pub fn update_security_settings(&mut self,current_password:&str,new_password:Option<&str>,new_confirm:Option<&str>,questions:Option<&[String]>,answers:Option<&[String]>)->Result<Option<String>,String>{let d=self.verify_master_dek(current_password)?;if new_password.is_none()&&questions.is_none(){return Err("至少选择一项修改".into())}let c=self.db()?;let salt=self.salt(&c)?;let mut new_code=None;let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;if let Some(p)=new_password{let confirm=new_confirm.unwrap_or("");validate_new_master_password(p)?;if p==current_password{return Err("新主密码不能与旧主密码相同".into())}if p!=confirm{return Err("两次输入的新主密码不一致".into())}let kek=Self::derive(p,&salt)?;let wrapped=Self::enc(&kek,&d,b"LocalVault|wrapped_dek|v1")?;tx.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('wrapped_dek',?)",params![wrapped]).map_err(|e|e.to_string())?;}if let Some(qs)=questions{let ans=answers.ok_or("缺少密保答案")?;if qs.len()!=3||ans.len()!=3{return Err("必须有 3 组问题/答案".into())}if qs.iter().any(|q|q.trim().is_empty())||ans.iter().any(|a|a.trim().is_empty()){return Err("密保问题和答案不能为空".into())}let code=self.generate_recovery_code()?;let combo=format!("{}\0{}\0{}\0{}",code,ans[0].trim(),ans[1].trim(),ans[2].trim());let rk=Self::derive(&combo,&salt)?;let wrapped=Self::enc(&rk,&d,b"LocalVault|recovery|v1")?;let raw_qs=serde_json::to_vec(qs).map_err(|e|e.to_string())?;tx.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('recovery_wrapped',?),('recovery_questions',?)",params![wrapped,raw_qs]).map_err(|e|e.to_string())?;new_code=Some(code)}tx.commit().map_err(|e|e.to_string())?;self.dek=Some(d);Ok(new_code)}
}
fn now_ms()->i64{use std::time::{SystemTime,UNIX_EPOCH};SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()as i64}
impl Drop for VaultManager{fn drop(&mut self){if let Some(mut d)=self.dek.take(){d.zeroize()}if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}}}
