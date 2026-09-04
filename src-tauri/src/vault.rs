use argon2::{Algorithm,Argon2,Params,Version};
use chacha20poly1305::{aead::{Aead,OsRng,Payload},KeyInit,XChaCha20Poly1305,XNonce};
use rand_core::RngCore;
use rusqlite::{Connection,params};
use serde::{Deserialize,Serialize};
use std::{fs,path::{Path,PathBuf}};
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
const BULK_MAX_BYTES:u64=50*1024*1024;
const BACKUP_MAX_RETENTION:usize=50;
const BACKUP_DEFAULT_RETENTION:usize=10;

#[cfg(windows)]
fn set_win_attrs(path:&std::path::Path,protect:bool)->Result<(),String>{
 use std::os::windows::ffi::OsStrExt;
 type DWORD=u32;
 #[link(name="kernel32")]
 extern "system"{fn GetFileAttributesW(name:*const u16)->DWORD;fn SetFileAttributesW(name:*const u16,attrs:DWORD)->i32;}
 const INVALID:DWORD=0xFFFF_FFFF;const READONLY:DWORD=0x1;const HIDDEN:DWORD=0x2;
 let wide:Vec<u16>=path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
 let mut attrs=unsafe{GetFileAttributesW(wide.as_ptr())};
 if attrs==INVALID{return Ok(())}
 if protect{attrs|=READONLY|HIDDEN}else{attrs&=!READONLY;attrs|=HIDDEN;}
 if unsafe{SetFileAttributesW(wide.as_ptr(),attrs)}==0{return Err(format!("无法设置文件属性：{}",path.display()))}
 Ok(())
}

#[cfg(windows)]
fn set_storage_attributes(base:&std::path::Path,protect:bool)->Result<(),String>{
 if !base.exists(){return Ok(())}
 let mut paths=Vec::new();
 fn collect(p:&std::path::Path,out:&mut Vec<std::path::PathBuf>)->std::io::Result<()>{
   out.push(p.to_path_buf());
   if p.is_dir(){for item in std::fs::read_dir(p)?{collect(&item?.path(),out)?;}}
   Ok(())
 }
 collect(base,&mut paths).map_err(|e|e.to_string())?;
 // Files are made read-only; the directory is also marked read-only/hidden as a visual and accidental-edit deterrent.
 // When unprotecting, only READONLY is removed; HIDDEN remains enabled.
 for path in paths.iter().rev(){set_win_attrs(path,protect)?;}
 Ok(())
}

#[cfg(not(windows))]
fn set_storage_attributes(_base:&std::path::Path,_protect:bool)->Result<(),String>{Ok(())}

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
#[serde(rename_all="camelCase")]
pub struct Category{pub name:String,pub icon:String,#[serde(default)] pub parent_name:Option<String>}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct HistoryRecord{pub changed_at:i64,pub fields:Vec<String>}
#[derive(Debug,Serialize,Deserialize)]
pub struct BootstrapStatus{pub exists:bool,pub version:u32,pub recovery_enabled:bool}
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct BackupSettings{pub enabled:bool,pub directory:Option<String>,pub retention:usize,pub last_backup_at:Option<i64>,pub last_error:Option<String>,#[serde(default)] pub last_backup_fingerprint:Option<String>}
impl Default for BackupSettings{fn default()->Self{Self{enabled:false,directory:None,retention:BACKUP_DEFAULT_RETENTION,last_backup_at:None,last_error:None,last_backup_fingerprint:None}}}
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
 fn protect_storage(&self)->Result<(),String>{if let Some(base)=self.path.parent(){set_storage_attributes(base,true)?;}Ok(())}
 fn unprotect_storage(&self)->Result<(),String>{if let Some(base)=self.path.parent(){set_storage_attributes(base,false)?;}Ok(())}
 fn backup_config_path(&self)->PathBuf{self.path.parent().unwrap_or_else(||Path::new(".")).join("backup-settings.json")}
 fn load_backup_settings(&self)->BackupSettings{fs::read(self.backup_config_path()).ok().and_then(|b|serde_json::from_slice::<BackupSettings>(&b).ok()).unwrap_or_default()}
 fn store_backup_settings(&self,settings:&BackupSettings)->Result<(),String>{let path=self.backup_config_path();let tmp=path.with_extension("json.new");let data=serde_json::to_vec_pretty(settings).map_err(|e|e.to_string())?;fs::write(&tmp,data).map_err(|e|e.to_string())?;if path.exists(){let _=fs::remove_file(&path);}fs::rename(&tmp,&path).map_err(|e|e.to_string())?;Ok(())}
 fn path_is_inside(&self,path:&Path)->bool{let base=self.path.parent().unwrap_or_else(||Path::new(".")).canonicalize().unwrap_or_else(|_|self.path.parent().unwrap_or_else(||Path::new(".")).to_path_buf());let target=if path.exists(){path.canonicalize().unwrap_or_else(|_|path.to_path_buf())}else if let Some(parent)=path.parent(){if parent.exists(){parent.canonicalize().unwrap_or_else(|_|parent.to_path_buf()).join(path.file_name().unwrap_or_else(||std::ffi::OsStr::new("")))}else{path.to_path_buf()}}else{path.to_path_buf()};#[cfg(windows)]{let b=base.to_string_lossy().to_ascii_lowercase();let t=target.to_string_lossy().to_ascii_lowercase();return t==b||t.starts_with(&(b+"\\"));}#[cfg(not(windows))]{target==base||target.starts_with(&base)}}
 fn validate_backup_directory(&self,directory:&str)->Result<PathBuf,String>{let p=PathBuf::from(directory.trim());if p.as_os_str().is_empty(){return Err("备份目录不能为空".into())}if !p.exists()||!p.is_dir(){return Err("备份目录不存在或不是文件夹".into())}if self.path_is_inside(&p){return Err("备份目录不能位于 LocalVault 数据目录内部；否则无法抵御同目录文件被一起删除或加密".into())}Ok(p)}
 fn fingerprint_vault(entries:&[Entry],categories:&[Category])->String{
  let mut ordered=entries.to_vec();
  ordered.sort_by(|a,b|a.id.cmp(&b.id));
  let bytes=serde_json::to_vec(&(ordered,categories)).unwrap_or_default();
  let mut h1:u64=0xcbf29ce484222325;
  let mut h2:u64=0x9e3779b185ebca87;
  for b in bytes{
    h1^=b as u64; h1=h1.wrapping_mul(0x100000001b3);
    h2^=(b as u64).wrapping_add(0x9e3779b9); h2=h2.rotate_left(13).wrapping_mul(0xbf58476d1ce4e5b9);
  }
  format!("{:016x}{:016x}",h1,h2)
 }
 fn create_versioned_backup(&self,settings:&mut BackupSettings,force:bool)->Result<Option<PathBuf>,String>{
  if !settings.enabled&&!force{return Ok(None)}
  let directory=match settings.directory.as_deref(){Some(v)=>self.validate_backup_directory(v)?,None=>return Err("尚未设置自动备份目录".into())};
  let dek=self.dek.ok_or("Vault locked")?;
  if !self.path.exists(){return Err("Vault file does not exist".into())}
  fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
  let c=Connection::open(&self.path).map_err(|e|e.to_string())?;
  let entries=self.read_entries(&c,&dek)?;
  drop(c);
  let categories=self.list_categories()?;
  let fingerprint=Self::fingerprint_vault(&entries,&categories);
  if !force&&settings.last_backup_fingerprint.as_deref()==Some(fingerprint.as_str()){
    return Ok(None);
  }
  let stamp=now_ms();
  let token=hex::encode(Self::random::<4>());
  let destination=directory.join(format!("LocalVault-{}-{}.vault",stamp,token));
  let tmp=directory.join(format!(".LocalVault-{}-{}.vault.tmp",stamp,token));
  if tmp.exists(){let _=fs::remove_file(&tmp);}
  fs::copy(&self.path,&tmp).map_err(|e|e.to_string())?;
  let c=match Connection::open(&tmp){Ok(v)=>v,Err(e)=>{let _=fs::remove_file(&tmp);return Err(e.to_string())}};
  if let Err(e)=self.init_schema(&c){drop(c);let _=fs::remove_file(&tmp);return Err(format!("自动备份验证失败：{}",e))}
  let verified=self.read_entries(&c,&dek);
  if let Err(e)=verified{drop(c);let _=fs::remove_file(&tmp);return Err(format!("自动备份验证失败：{}",e))}
  drop(c);
  fs::rename(&tmp,&destination).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;
  settings.last_backup_at=Some(stamp);
  settings.last_error=None;
  settings.last_backup_fingerprint=Some(fingerprint);
  let keep=settings.retention.max(1).min(BACKUP_MAX_RETENTION);
  let mut files=fs::read_dir(&directory).map_err(|e|e.to_string())?
    .filter_map(|x|x.ok())
    .filter(|x|x.path().extension().and_then(|e|e.to_str())==Some("vault")&&x.file_name().to_string_lossy().starts_with("LocalVault-"))
    .collect::<Vec<_>>();
  files.sort_by_key(|x|x.metadata().and_then(|m|m.modified()).ok());
  while files.len()>keep{if let Some(old)=files.first(){let _=fs::remove_file(old.path());}files.remove(0);}
  Ok(Some(destination))
 }
 fn record_backup_error(&self,settings:&mut BackupSettings,error:String){settings.last_error=Some(error);let _=self.store_backup_settings(settings);}
 pub fn backup_settings(&self)->Result<BackupSettings,String>{Ok(self.load_backup_settings())}
 pub fn set_backup_settings(&self,enabled:bool,directory:Option<String>,retention:usize)->Result<BackupSettings,String>{let _=self.dek.ok_or("Vault locked")?;let mut settings=BackupSettings{enabled,directory:directory.map(|x|x.trim().to_string()).filter(|x|!x.is_empty()),retention:retention.max(1).min(BACKUP_MAX_RETENTION),last_backup_at:None,last_error:None,last_backup_fingerprint:None};if settings.enabled{let dir=settings.directory.clone().ok_or("启用自动备份前请选择备份目录")?;let _=self.validate_backup_directory(&dir)?;}let old=self.load_backup_settings();settings.last_backup_at=old.last_backup_at;if old.enabled&&old.directory==settings.directory{settings.last_backup_fingerprint=old.last_backup_fingerprint;}self.store_backup_settings(&settings)?;Ok(settings)}
 pub fn backup_now(&self)->Result<BackupSettings,String>{let mut settings=self.load_backup_settings();match self.create_versioned_backup(&mut settings,true){Ok(_)=>{self.store_backup_settings(&settings)?;Ok(settings)},Err(e)=>{self.record_backup_error(&mut settings,e.clone());Err(e)}}}
 fn auto_backup(&self){let mut settings=self.load_backup_settings();if !settings.enabled{return}match self.create_versioned_backup(&mut settings,false){Ok(_)=>{let _=self.store_backup_settings(&settings);},Err(e)=>{self.record_backup_error(&mut settings,e);}}}
 fn validate_external_backup_destination(&self,destination:&Path)->Result<(),String>{if self.path_is_inside(destination){return Err("备份文件不能保存到 LocalVault 数据目录内部".into())}if let Some(parent)=destination.parent(){if parent.exists()&&parent.is_dir(){Ok(())}else{Err("备份目标文件夹不存在".into())}}else{Err("备份目标无效".into())}}
 pub fn status(&self)->Result<BootstrapStatus,String>{
   if !self.path.exists(){
     let bak=self.path.with_extension("bak");
     if bak.exists(){let _=fs::rename(&bak,&self.path);}
     if !self.path.exists(){return Ok(BootstrapStatus{exists:false,version:FORMAT_VERSION,recovery_enabled:false})}
   }
   // Every application start is treated as locked: restore the Windows hidden/read-only protection first.
   let _=self.protect_storage();
   let c=Connection::open(&self.path).map_err(|e|e.to_string())?;
   let v=self.meta(&c,"format_version").ok().and_then(|b|if b.len()==4{Some(u32::from_be_bytes(b.try_into().ok()?))}else{None}).unwrap_or(1);
   Ok(BootstrapStatus{exists:true,version:v,recovery_enabled:self.meta(&c,"recovery_wrapped").is_ok()})
 }
 fn init_schema(&self,c:&Connection)->Result<(),String>{
   c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value BLOB NOT NULL); CREATE TABLE IF NOT EXISTS entries(id TEXT PRIMARY KEY,cipher BLOB NOT NULL,updated_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS categories(name TEXT PRIMARY KEY,icon TEXT NOT NULL DEFAULT '📁',parent_name TEXT,created_at INTEGER NOT NULL,sort_order INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS history(id INTEGER PRIMARY KEY AUTOINCREMENT,entry_id TEXT NOT NULL,changed_at INTEGER NOT NULL,cipher BLOB NOT NULL); CREATE INDEX IF NOT EXISTS idx_history_entry_time ON history(entry_id,changed_at DESC); CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY,cipher BLOB NOT NULL,deleted_at INTEGER NOT NULL,original_seq INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_trash_deleted_at ON trash(deleted_at);").map_err(|e|e.to_string())?;
   let mut st=c.prepare("PRAGMA table_info(categories)").map_err(|e|e.to_string())?;let mut rows=st.query([]).map_err(|e|e.to_string())?;let mut icon=false;while let Some(r)=rows.next().map_err(|e|e.to_string())?{let n:String=r.get(1).map_err(|e|e.to_string())?;if n=="icon"{icon=true;break}}drop(rows);drop(st);if !icon{c.execute("ALTER TABLE categories ADD COLUMN icon TEXT NOT NULL DEFAULT '📁'",[]).map_err(|e|e.to_string())?;}
   let mut st=c.prepare("PRAGMA table_info(categories)").map_err(|e|e.to_string())?;let mut rows=st.query([]).map_err(|e|e.to_string())?;let mut parent=false;while let Some(r)=rows.next().map_err(|e|e.to_string())?{let n:String=r.get(1).map_err(|e|e.to_string())?;if n=="parent_name"{parent=true;break}}drop(rows);drop(st);if !parent{c.execute("ALTER TABLE categories ADD COLUMN parent_name TEXT",[]).map_err(|e|e.to_string())?;}
   Ok(())
 }
 fn db(&self)->Result<Connection,String>{if let Some(p)=self.path.parent(){fs::create_dir_all(p).map_err(|e|e.to_string())?}let c=Connection::open(&self.path).map_err(|e|e.to_string())?;self.init_schema(&c)?;let defaults=[("默认","▦",0i64),("工作","💼",1),("财务","💰",2),("社交","👥",3),("开发","💻",4),("其他","📁",5)];for (n,i,o) in defaults{c.execute("INSERT OR IGNORE INTO categories(name,icon,created_at,sort_order) VALUES(?1,?2,strftime('%s','now'),?3)",params![n,i,o]).map_err(|e|e.to_string())?;}Ok(c)}
 fn random<const N:usize>()->[u8;N]{let mut x=[0u8;N];OsRng.fill_bytes(&mut x);x}
 fn derive(password:&str,salt:&[u8;SALT_LEN])->Result<[u8;32],String>{let p=Params::new(ARGON_MEM_KIB,ARGON_ITERS,ARGON_LANES,Some(32)).map_err(|e|e.to_string())?;let a=Argon2::new(Algorithm::Argon2id,Version::V0x13,p);let mut out=[0u8;32];a.hash_password_into(password.as_bytes(),salt,&mut out).map_err(|e|e.to_string())?;Ok(out)}
 fn enc(key:&[u8;32],plain:&[u8],aad:&[u8])->Result<Vec<u8>,String>{let c=XChaCha20Poly1305::new(key.into());let n=Self::random::<24>();let ct=c.encrypt(XNonce::from_slice(&n),Payload{msg:plain,aad}).map_err(|e|e.to_string())?;let mut out=n.to_vec();out.extend_from_slice(&ct);Ok(out)}
 fn dec(key:&[u8;32],data:&[u8],aad:&[u8])->Result<Vec<u8>,String>{if data.len()<NONCE_LEN+16{return Err("invalid ciphertext".into())}let c=XChaCha20Poly1305::new(key.into());c.decrypt(XNonce::from_slice(&data[..NONCE_LEN]),Payload{msg:&data[NONCE_LEN..],aad}).map_err(|_|"authentication failed".into())}
 fn meta(&self,c:&Connection,k:&str)->Result<Vec<u8>,String>{c.query_row("SELECT value FROM meta WHERE key=?1",params![k],|r|r.get(0)).map_err(|e|e.to_string())}
 fn salt(&self,c:&Connection)->Result<[u8;16],String>{let b=self.meta(c,"salt")?;if b.len()!=16{return Err("invalid salt".into())}let mut s=[0u8;16];s.copy_from_slice(&b);Ok(s)}
 pub fn create(&mut self,pass:&str,confirm:&str)->Result<(),String>{validate_new_master_password(pass)?;if pass!=confirm{return Err("两次输入的主密码不一致".into())}if self.path.exists(){return Err("Vault 已存在".into())}let c=self.db()?;let salt=Self::random::<16>();let dek=Self::random::<32>();let kek=Self::derive(pass,&salt)?;let wrapped=Self::enc(&kek,&dek,b"LocalVault|wrapped_dek|v1")?;c.execute("INSERT INTO meta(key,value)VALUES('format_version',?),('salt',?),('wrapped_dek',?)",params![FORMAT_VERSION.to_be_bytes().to_vec(),salt.to_vec(),wrapped]).map_err(|e|e.to_string())?;self.dek=Some(dek);Ok(())}
 pub fn unlock(&mut self,pass:&str)->Result<Vec<Entry>,String>{
   self.unprotect_storage()?;
   let result=(||{
     let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"wrapped_dek")?;let kek=Self::derive(pass,&salt)?;
     let raw=Self::dec(&kek,&w,b"LocalVault|wrapped_dek|v1").map_err(|_|"bad credentials".to_string())?;
     if raw.len()!=32{return Err("invalid DEK".into())}
     let mut d=[0u8;32];d.copy_from_slice(&raw);self.dek=Some(d);self.read_entries(&c,&d)
   })();
   if result.is_err(){let _=self.protect_storage();}
   result
 }
 fn read_entries(&self,c:&Connection,dek:&[u8;32])->Result<Vec<Entry>,String>{let mut st=c.prepare("SELECT id,cipher FROM entries ORDER BY updated_at DESC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for row in rows{let(id,cipher)=row.map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",id);let plain=Self::dec(dek,&cipher,aad.as_bytes())?;let mut e:Entry=serde_json::from_slice(&plain).map_err(|_|"entry decode failed".to_string())?;e.id=id;out.push(e)}let has_seq=out.iter().any(|e|e.seq>0);if has_seq{out.sort_by(|a,b|{let sa=if a.seq>0{a.seq}else{i64::MAX};let sb=if b.seq>0{b.seq}else{i64::MAX};sa.cmp(&sb).then_with(||b.updated_at.cmp(&a.updated_at))});}Ok(out)}
 fn changed_fields(a:&Entry,b:&Entry)->Vec<String>{let mut f=Vec::new();if a.entry_type!=b.entry_type{f.push("类型".into())}if a.name!=b.name{f.push("密码名称".into())}if a.username!=b.username{f.push("账号/用户名".into())}if a.email!=b.email{f.push("邮箱".into())}if a.phone!=b.phone{f.push("手机号".into())}if a.password!=b.password{f.push("密码".into())}if a.nickname!=b.nickname{f.push("平台昵称".into())}if a.url!=b.url{f.push("网址/IP/APP".into())}if a.notes!=b.notes{f.push("备注".into())}if a.category!=b.category{f.push("分类".into())}if a.tags!=b.tags{f.push("标签".into())}if a.favorite!=b.favorite{f.push("收藏状态".into())}if a.expires_at!=b.expires_at{f.push("密码过期设置".into())}f}
 pub fn save(&mut self,entries:&[Entry])->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;let old_path=self.path.clone();let tmp=old_path.with_extension("new");if tmp.exists(){let _=fs::remove_file(&tmp);}let c=Connection::open(&tmp).map_err(|e|e.to_string())?;self.init_schema(&c)?;let old=Connection::open(&old_path).map_err(|e|e.to_string())?;self.init_schema(&old)?;let salt=self.salt(&old)?;let w=self.meta(&old,"wrapped_dek")?;let recovery=self.meta(&old,"recovery_wrapped").ok();let questions=self.meta(&old,"recovery_questions").ok();c.execute("INSERT INTO meta VALUES('format_version',?),('salt',?),('wrapped_dek',?)",params![FORMAT_VERSION.to_be_bytes().to_vec(),salt.to_vec(),w]).map_err(|e|e.to_string())?;if let Some(x)=recovery{c.execute("INSERT INTO meta VALUES('recovery_wrapped',?)",params![x]).map_err(|e|e.to_string())?;}if let Some(x)=questions{c.execute("INSERT INTO meta VALUES('recovery_questions',?)",params![x]).map_err(|e|e.to_string())?;}
   {let mut st=old.prepare("SELECT name,icon,parent_name,created_at,sort_order FROM categories ORDER BY sort_order").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?))).map_err(|e|e.to_string())?;for r in rows{let(n,i,pn,ca,so)=r.map_err(|e|e.to_string())?;c.execute("INSERT OR REPLACE INTO categories(name,icon,parent_name,created_at,sort_order)VALUES(?,?,?,?,?)",params![n,i,pn,ca,so]).map_err(|e|e.to_string())?;} }
   {let mut st=old.prepare("SELECT entry_id,changed_at,cipher FROM history").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,Vec<u8>>(2)?))).map_err(|e|e.to_string())?;for r in rows{let(e,t,ct)=r.map_err(|e|e.to_string())?;c.execute("INSERT INTO history(entry_id,changed_at,cipher)VALUES(?,?,?)",params![e,t,ct]).map_err(|e|e.to_string())?;} }
   {let mut st=old.prepare("SELECT id,cipher,deleted_at,original_seq FROM trash").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?))).map_err(|e|e.to_string())?;for r in rows{let(id,ct,t,seq)=r.map_err(|e|e.to_string())?;c.execute("INSERT INTO trash(id,cipher,deleted_at,original_seq)VALUES(?,?,?,?)",params![id,ct,t,seq]).map_err(|e|e.to_string())?;} }
   let old_entries=self.read_entries(&old,&dek)?;let old_map=old_entries.iter().map(|e|(e.id.clone(),e)).collect::<std::collections::HashMap<_,_>>();let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;for e in entries{if let Some(prev)=old_map.get(&e.id){let fields=Self::changed_fields(prev,e);if !fields.is_empty(){let p=serde_json::to_vec(&fields).map_err(|e|e.to_string())?;let aad=format!("LocalVault|history|{}|v1",e.id);let ct=Self::enc(&dek,&p,aad.as_bytes())?;tx.execute("INSERT INTO history(entry_id,changed_at,cipher)VALUES(?,?,?)",params![e.id,now_ms(),ct]).map_err(|e|e.to_string())?;tx.execute("DELETE FROM history WHERE entry_id=?1 AND id NOT IN (SELECT id FROM history WHERE entry_id=?1 ORDER BY changed_at DESC,id DESC LIMIT 3)",params![e.id]).map_err(|e|e.to_string())?;}}
     let p=serde_json::to_vec(e).map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",e.id);let ct=Self::enc(&dek,&p,aad.as_bytes())?;tx.execute("INSERT INTO entries(id,cipher,updated_at)VALUES(?,?,?)",params![e.id,ct,e.updated_at]).map_err(|e|e.to_string())?;}tx.commit().map_err(|e|e.to_string())?;drop(old);drop(c);let bak=old_path.with_extension("bak");if old_path.exists(){let _=fs::copy(&old_path,&bak);}if old_path.exists(){fs::remove_file(&old_path).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;}if let Err(e)=fs::rename(&tmp,&old_path){if bak.exists(){let _=fs::rename(&bak,&old_path);}return Err(e.to_string())}let _=fs::remove_file(&bak);self.auto_backup();Ok(())}
 pub fn list_categories(&self)->Result<Vec<Category>,String>{let c=self.db()?;let mut st=c.prepare("SELECT name,icon,parent_name FROM categories ORDER BY CASE WHEN parent_name IS NULL THEN 0 ELSE 1 END, COALESCE(parent_name,'') ASC, sort_order ASC, created_at ASC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok(Category{name:r.get(0)?,icon:r.get(1)?,parent_name:r.get(2)?})).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{out.push(r.map_err(|e|e.to_string())?)}Ok(out)}
 pub fn create_category(&self,name:&str,icon:&str,parent_name:Option<&str>)->Result<(),String>{let n=name.trim();if n.is_empty(){return Err("分类名称不能为空".into())}if n.chars().count()>40{return Err("分类名称过长".into())}let parent=parent_name.map(str::trim).filter(|x|!x.is_empty()).map(str::to_string);if parent.as_deref()==Some(n){return Err("分类不能以自己作为父分类".into())}let c=self.db()?;if c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![n],|r|r.get(0)).is_ok(){return Err("分类已经存在".into())}if let Some(ref pn)=parent{if c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![pn],|r|r.get(0)).is_err(){return Err("父分类不存在".into())}}let order:i64=c.query_row::<i64,_,_>("SELECT COALESCE(MAX(sort_order),-1)+1 FROM categories WHERE parent_name IS ?1",params![parent.as_deref()],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?;let icon_value=if icon.trim().is_empty(){"📁".to_string()}else{icon.trim().to_string()};c.execute("INSERT INTO categories(name,icon,parent_name,created_at,sort_order)VALUES(?1,?2,?3,strftime('%s','now'),?4)",params![n,icon_value,parent.as_deref(),order]).map_err(|e|e.to_string())?;Ok(())}
 pub fn reorder_categories(&self,parent_name:Option<&str>,names:&[String])->Result<(),String>{let c=self.db()?;let parent=parent_name.map(str::to_string);let current=self.list_categories()?.into_iter().filter(|x|x.parent_name==parent).collect::<Vec<_>>();if names.len()!=current.len(){return Err("分类排序数据不完整".into())}let expected=current.iter().map(|x|x.name.as_str()).collect::<std::collections::HashSet<_>>();let provided=names.iter().map(|x|x.as_str()).collect::<std::collections::HashSet<_>>();if expected!=provided{return Err("分类排序数据无效".into())}let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;for (i,name) in names.iter().enumerate(){tx.execute("UPDATE categories SET sort_order=?1 WHERE name=?2 AND parent_name IS ?3",params![i as i64,name,parent]).map_err(|e|e.to_string())?;}tx.commit().map_err(|e|e.to_string())?;drop(c);self.auto_backup();Ok(())}
 pub fn move_to_trash(&mut self,entry_id:&str)->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let row=c.query_row("SELECT cipher FROM entries WHERE id=?1",params![entry_id],|r|r.get::<_,Vec<u8>>(0)).map_err(|_|"密码条目不存在".to_string())?;let aad=format!("LocalVault|entry|{}|v1",entry_id);let plain=Self::dec(&dek,&row,&aad.as_bytes())?;let e:Entry=serde_json::from_slice(&plain).map_err(|_|"entry decode failed".to_string())?;let deleted_at=now_ms();c.execute("INSERT OR REPLACE INTO trash(id,cipher,deleted_at,original_seq)VALUES(?1,?2,?3,?4)",params![entry_id,row,deleted_at,e.seq]).map_err(|e|e.to_string())?;Ok(())}
 pub fn list_trash(&self,retention_days:i64)->Result<Vec<Entry>,String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let days=retention_days.max(7).min(30);let cutoff=now_ms()-days*86400000;c.execute("DELETE FROM trash WHERE deleted_at<?1",params![cutoff]).map_err(|e|e.to_string())?;let mut st=c.prepare("SELECT id,cipher FROM trash ORDER BY deleted_at DESC").map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{let(id,cipher)=r.map_err(|e|e.to_string())?;let aad=format!("LocalVault|entry|{}|v1",id);let p=Self::dec(&dek,&cipher,aad.as_bytes())?;let mut e:Entry=serde_json::from_slice(&p).map_err(|_|"trash decode failed".to_string())?;e.id=id;out.push(e)}Ok(out)}
 pub fn restore_trash(&mut self,entry_id:&str)->Result<Vec<Entry>,String>{
   let dek=self.dek.ok_or("Vault locked")?;
   let c=self.db()?;
   let row=c.query_row("SELECT cipher FROM trash WHERE id=?1",params![entry_id],|r|r.get::<_,Vec<u8>>(0)).map_err(|_|"回收站中不存在该条目".to_string())?;
   let aad=format!("LocalVault|entry|{}|v1",entry_id);
   let p=Self::dec(&dek,&row,aad.as_bytes())?;
   let mut e:Entry=serde_json::from_slice(&p).map_err(|_|"trash decode failed".to_string())?;
   if c.query_row::<String,_,_>("SELECT id FROM entries WHERE id=?1",params![entry_id],|r|r.get(0)).is_ok(){return Err("当前 Vault 已存在同一条目".into())}
   let mut entries=self.read_entries(&c,&dek)?;
   e.seq=entries.iter().map(|x|x.seq).max().unwrap_or(0)+1;
   e.updated_at=now_ms();
   let payload=serde_json::to_vec(&e).map_err(|x|x.to_string())?;
   let new_cipher=Self::enc(&dek,&payload,aad.as_bytes())?;
   let tx=c.unchecked_transaction().map_err(|x|x.to_string())?;
   tx.execute("INSERT INTO entries(id,cipher,updated_at)VALUES(?1,?2,?3)",params![entry_id,new_cipher,e.updated_at]).map_err(|x|x.to_string())?;
   tx.execute("DELETE FROM trash WHERE id=?1",params![entry_id]).map_err(|x|x.to_string())?;
   tx.commit().map_err(|x|x.to_string())?;
   entries.push(e);
   entries.sort_by_key(|x|x.seq);
   drop(c);
   self.auto_backup();
   Ok(entries)
 }
 pub fn purge_trash(&mut self,entry_id:Option<String>)->Result<(),String>{let _=self.dek.ok_or("Vault locked")?;let c=self.db()?;match entry_id{Some(id)=>{c.execute("DELETE FROM trash WHERE id=?1",params![id]).map_err(|e|e.to_string())?;},None=>{c.execute("DELETE FROM trash",[]).map_err(|e|e.to_string())?;}}Ok(())}
 pub fn update_category(&self,old_name:&str,name:&str,icon:&str,parent_name:Option<&str>)->Result<(),String>{let old=old_name.trim();let n=name.trim();if old=="默认"&&n!="默认"{return Err("默认分类不能重命名".into())}if n.is_empty(){return Err("分类名称不能为空".into())}let parent=parent_name.map(str::trim).filter(|x|!x.is_empty()).map(str::to_string);if n=="默认"&&parent.is_some(){return Err("默认分类不能设置父分类".into())}if parent.as_deref()==Some(n){return Err("分类不能以自己作为父分类".into())}let c=self.db()?;if old!=n&&c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![n],|r|r.get(0)).is_ok(){return Err("分类已经存在".into())}if let Some(ref pn)=parent{if c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![pn],|r|r.get(0)).is_err(){return Err("父分类不存在".into())};let mut cur=Some(pn.clone());while let Some(x)=cur{if x==old{return Err("不能把分类移动到自己的子分类下".into())}cur=c.query_row::<Option<String>,_,_>("SELECT parent_name FROM categories WHERE name=?1",params![x],|r|r.get(0)).ok().flatten();}}let old_parent:Option<String>=c.query_row::<Option<String>,_,_>("SELECT parent_name FROM categories WHERE name=?1",params![old],|r|r.get::<_,Option<String>>(0)).map_err(|e|e.to_string())?;let target_parent_changed=old_parent!=parent;let order:i64=if target_parent_changed{c.query_row::<i64,_,_>("SELECT COALESCE(MAX(sort_order),-1)+1 FROM categories WHERE parent_name IS ?1",params![parent.as_deref()],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?}else{c.query_row::<i64,_,_>("SELECT sort_order FROM categories WHERE name=?1",params![old],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?};let icon_value=if icon.trim().is_empty(){"📁".to_string()}else{icon.trim().to_string()};c.execute("UPDATE categories SET name=?1,icon=?2,parent_name=?3,sort_order=?4 WHERE name=?5",params![n,icon_value,parent.as_deref(),order,old]).map_err(|e|e.to_string())?;if old!=n{c.execute("UPDATE categories SET parent_name=?1 WHERE parent_name=?2",params![n,old]).map_err(|e|e.to_string())?;}Ok(())}
 pub fn delete_category(&self,name:&str)->Result<(),String>{let n=name.trim();if n=="默认"{return Err("默认分类不能删除".into())}let c=self.db()?;let parent:Option<String>=c.query_row("SELECT parent_name FROM categories WHERE name=?1",params![n],|r|r.get(0)).map_err(|_|"分类不存在".to_string())?;c.execute("UPDATE categories SET parent_name=?1 WHERE parent_name=?2",params![parent,n]).map_err(|e|e.to_string())?;c.execute("DELETE FROM categories WHERE name=?1",params![n]).map_err(|e|e.to_string())?;Ok(())}
 pub fn history_list(&self,entry_id:&str)->Result<Vec<HistoryRecord>,String>{let dek=self.dek.ok_or("Vault locked")?;let c=self.db()?;let mut st=c.prepare("SELECT changed_at,cipher FROM history WHERE entry_id=?1 ORDER BY changed_at DESC,id DESC LIMIT 3").map_err(|e|e.to_string())?;let rows=st.query_map(params![entry_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Vec<u8>>(1)?))).map_err(|e|e.to_string())?;let mut out=Vec::new();for r in rows{let(t,ct)=r.map_err(|e|e.to_string())?;let aad=format!("LocalVault|history|{}|v1",entry_id);let p=Self::dec(&dek,&ct,aad.as_bytes())?;let fields:Vec<String>=serde_json::from_slice(&p).map_err(|_|"history decode failed".to_string())?;out.push(HistoryRecord{changed_at:t,fields})}Ok(out)}
 pub fn export_entries(&self,entries:&[Entry],destination:&str,master_password:&str)->Result<(),String>{
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

 pub fn write_bulk_import_template(&self,destination:&str)->Result<(),String>{let p=PathBuf::from(destination);if p.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("csv"))!=Some(true){return Err("批量导入模板必须使用 .csv 文件".into())}let data="\u{FEFF}名称,账号/用户名,密码,网址/IP/APP名称,账号类型,分类,邮箱,手机号,平台昵称,标签,收藏,密码有效期,备注\r\n示例：GitHub,demo@example.com,Demo-GitHub-2026!,https://github.com,网站,开发,demo@example.com,,LocalVault演示账号,LocalVault示例,否,30天,【LocalVault模板示例，不会导入】\r\n示例：企业邮箱,demo@example.com,Demo-Mail-2026!,mail.example.com,邮箱,工作,demo@example.com,13800000000,演示账号,LocalVault示例,否,90天,【LocalVault模板示例，不会导入】\r\n";fs::write(p,data.as_bytes()).map_err(|e|e.to_string())}
 pub fn read_bulk_import_file(&self,source:&str)->Result<String,String>{let p=PathBuf::from(source);if p.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("csv"))!=Some(true){return Err("只允许导入 .csv 模板文件".into())}let meta=fs::metadata(&p).map_err(|e|e.to_string())?;if !meta.is_file(){return Err("导入源不是文件".into())}if meta.len()>BULK_MAX_BYTES{return Err("CSV 文件过大，最大允许 50 MB".into())}let b=fs::read(&p).map_err(|e|e.to_string())?;if b.starts_with(&[0xEF,0xBB,0xBF]){return String::from_utf8(b[3..].to_vec()).map_err(|_|"CSV 编码无法识别，请使用 UTF-8、UTF-16 或 GBK/GB18030 编码".to_string())}if b.starts_with(&[0xFF,0xFE]){if (b.len()-2)%2!=0{return Err("UTF-16LE CSV 编码不完整".into())}let u:Vec<u16>=b[2..].chunks_exact(2).map(|x|u16::from_le_bytes([x[0],x[1]])).collect();return String::from_utf16(&u).map_err(|_|"UTF-16LE CSV 编码无效".to_string())}if b.starts_with(&[0xFE,0xFF]){if (b.len()-2)%2!=0{return Err("UTF-16BE CSV 编码不完整".into())}let u:Vec<u16>=b[2..].chunks_exact(2).map(|x|u16::from_be_bytes([x[0],x[1]])).collect();return String::from_utf16(&u).map_err(|_|"UTF-16BE CSV 编码无效".to_string())}match String::from_utf8(b.clone()){Ok(s)=>Ok(s),Err(_)=>{let (decoded,_,had_errors)=encoding_rs::GB18030.decode(&b);if had_errors{return Err("CSV 编码无法识别，请使用 UTF-8、UTF-16 或 GBK/GB18030 编码".into())}Ok(decoded.into_owned())}}}
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
 pub fn lock(&mut self)->Result<(),String>{if let Some(mut d)=self.dek.take(){d.zeroize()}if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}self.protect_storage()?;Ok(())}
 pub fn backup(&self,destination:&str)->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;if !self.path.exists(){return Err("Vault file does not exist".into())}let dest=PathBuf::from(destination);if dest.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("vault"))!=Some(true){return Err("完整备份文件必须使用 .vault 扩展名".into())}self.validate_external_backup_destination(&dest)?;let tmp=dest.with_extension("vault.new");if tmp.exists(){let _=fs::remove_file(&tmp);}fs::copy(&self.path,&tmp).map_err(|e|e.to_string())?;let c=Connection::open(&tmp).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;self.init_schema(&c)?;self.read_entries(&c,&dek).map_err(|e|{let _=fs::remove_file(&tmp);format!("备份验证失败：{}",e)})?;drop(c);if dest.exists(){let _=fs::remove_file(&dest);}fs::rename(&tmp,&dest).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;Ok(())}
 fn table_has_column(c:&Connection,table:&str,column:&str)->Result<bool,String>{
   let mut st=c.prepare(&format!("PRAGMA table_info({})",table)).map_err(|e|e.to_string())?;
   let mut rows=st.query([]).map_err(|e|e.to_string())?;
   while let Some(r)=rows.next().map_err(|e|e.to_string())?{
     let name:String=r.get(1).map_err(|e|e.to_string())?;
     if name==column{return Ok(true)}
   }
   Ok(false)
 }
 fn read_backup_categories(&self,c:&Connection)->Result<Vec<Category>,String>{
   let has_icon=Self::table_has_column(c,"categories","icon")?;
   let has_parent=Self::table_has_column(c,"categories","parent_name")?;
   let mut categories=Vec::new();
   if has_parent{
     let sql=if has_icon{"SELECT name,icon,parent_name FROM categories ORDER BY CASE WHEN parent_name IS NULL THEN 0 ELSE 1 END, COALESCE(parent_name,'') ASC, sort_order ASC, created_at ASC"}else{"SELECT name,'📁',parent_name FROM categories ORDER BY CASE WHEN parent_name IS NULL THEN 0 ELSE 1 END, COALESCE(parent_name,'') ASC, sort_order ASC, created_at ASC"};
     let mut st=c.prepare(sql).map_err(|e|e.to_string())?;
     let rows=st.query_map([],|r|Ok(Category{name:r.get(0)?,icon:r.get(1)?,parent_name:r.get(2)?})).map_err(|e|e.to_string())?;
     for r in rows{categories.push(r.map_err(|e|e.to_string())?);}
   }else{
     let sql=if has_icon{"SELECT name,icon FROM categories ORDER BY sort_order ASC,created_at ASC"}else{"SELECT name,'📁' FROM categories ORDER BY sort_order ASC,created_at ASC"};
     let mut st=c.prepare(sql).map_err(|e|e.to_string())?;
     let rows=st.query_map([],|r|Ok(Category{name:r.get(0)?,icon:r.get(1)?,parent_name:None})).map_err(|e|e.to_string())?;
     for r in rows{categories.push(r.map_err(|e|e.to_string())?);}
   }
   Ok(categories)
 }
 fn read_backup_file(&self,backup_path:&str,pass:&str)->Result<BackupData,String>{
   let bp=PathBuf::from(backup_path);
   if bp.extension().and_then(|x|x.to_str()).map(|x|x.eq_ignore_ascii_case("vault"))!=Some(true){return Err("只允许导入 .vault 加密备份".into())}
   if !bp.exists(){return Err("备份不存在".into())}
   let c=Connection::open(&bp).map_err(|e|e.to_string())?;
   let salt=self.salt(&c)?;
   let wrapped=self.meta(&c,"wrapped_dek")?;
   let kek=Self::derive(pass,&salt)?;
   let raw=Self::dec(&kek,&wrapped,b"LocalVault|wrapped_dek|v1").map_err(|_|"备份验证失败：主密码不正确".to_string())?;
   if raw.len()!=DEK_LEN{return Err("备份中的 Vault 密钥无效".into())}
   let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);
   let entries=self.read_entries(&c,&d).map_err(|e|format!("备份验证失败：{}",e))?;
   let categories=self.read_backup_categories(&c).map_err(|e|format!("备份分类读取失败：{}",e))?;
   Ok(BackupData{entries,categories})
 }
 pub fn backup_preview(&self,backup_path:&str,pass:&str)->Result<BackupData,String>{
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   let data=self.read_backup_file(backup_path,pass)?;
   Ok(data)
 }
 pub fn restore(&mut self,backup_path:&str,pass:&str)->Result<Vec<Entry>,String>{
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   self.unprotect_storage()?;
   let result=(||{
     let bp=PathBuf::from(backup_path);
     if !bp.exists(){return Err("备份不存在".into())}
     let tmp=self.path.with_extension("restore");
     if tmp.exists(){let _=fs::remove_file(&tmp);}
     fs::copy(&bp,&tmp).map_err(|e|e.to_string())?;
     let c=match Connection::open(&tmp){Ok(v)=>v,Err(e)=>{let _=fs::remove_file(&tmp);return Err(e.to_string())}};
     if let Err(e)=self.init_schema(&c){drop(c);let _=fs::remove_file(&tmp);return Err(format!("备份结构验证失败：{}；原 Vault 未被替换",e))}
     let salt=self.salt(&c)?;
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
     if self.path.exists(){self.auto_backup();}
     let old_backup=self.path.with_extension("pre_restore.bak");
     if old_backup.exists(){let _=fs::remove_file(&old_backup);}
     if self.path.exists(){fs::copy(&self.path,&old_backup).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;fs::remove_file(&self.path).map_err(|e|{let _=fs::remove_file(&tmp);e.to_string()})?;}
     if let Err(e)=fs::rename(&tmp,&self.path){
       if old_backup.exists(){let _=fs::rename(&old_backup,&self.path);}
       let _=fs::remove_file(&tmp);
       return Err(format!("恢复文件替换失败：{}；原 Vault 已保留",e));
     }
     if let Some(mut old)=self.dek.take(){old.zeroize()}
     self.dek=Some(d);
     Ok(entries)
   })();
   if result.is_err(){let _=self.protect_storage();}
   result
 }
 pub fn merge_backup(&mut self,backup_path:&str,pass:&str)->Result<Vec<Entry>,String>{
   let _=self.dek.ok_or("Vault locked")?;
   if pass.is_empty(){return Err("请输入备份对应的 Vault 主密码".into())}
   let data=self.read_backup_file(backup_path,pass)?;
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
   let mut pending=data.categories;
   while !pending.is_empty(){
     let mut next=Vec::new();let mut progressed=false;
     for cat in pending{
       if cat.name.trim().is_empty(){continue;}
       let c=self.db()?;let exists=c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![cat.name],|r|r.get(0)).is_ok();let parent_exists=match cat.parent_name.as_deref(){None=>true,Some(pn)=>c.query_row::<String,_,_>("SELECT name FROM categories WHERE name=?1",params![pn],|r|r.get(0)).is_ok()};drop(c);
       if exists||parent_exists{if !exists{self.create_category(&cat.name,&cat.icon,cat.parent_name.as_deref())?;}progressed=true}else{next.push(cat)}
     }
     if !progressed{return Err("备份中的分类层级无效：找不到父分类".into())}
     pending=next;
   }
   self.save(&current)?;
   Ok(current)
 }
 pub fn generate_recovery_code(&self)->Result<String,String>{let alphabet=b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";let mut r=[0u8;24];OsRng.fill_bytes(&mut r);let mut out=String::with_capacity(29);for(i,n)in r.iter().enumerate(){if i>0&&i%6==0{out.push('-')}out.push(alphabet[(*n as usize)%alphabet.len()]as char)}Ok(out)}
 pub fn recovery_questions(&self)->Result<Vec<String>,String>{
   if !self.path.exists(){return Ok(vec!["我最喜欢的一本童年读物是什么？".into(),"我自己定义的长期不变短语是什么？".into(),"我记得的第一个特别地点是什么？".into()])}
   let c=Connection::open(&self.path).map_err(|e|e.to_string())?;
   match self.meta(&c,"recovery_questions"){Ok(raw)=>serde_json::from_slice(&raw).map_err(|_|"恢复问题数据损坏".into()),Err(_)=>Ok(vec!["我最喜欢的一本童年读物是什么？".into(),"我自己定义的长期不变短语是什么？".into(),"我记得的第一个特别地点是什么？".into()])}
 }
 fn verify_master_dek(&self,pass:&str)->Result<[u8;DEK_LEN],String>{if pass.chars().count()<MIN_MASTER_PASSWORD_LEN{return Err("主密码至少 8 位".into())}let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"wrapped_dek")?;let kek=Self::derive(pass,&salt)?;let raw=Self::dec(&kek,&w,b"LocalVault|wrapped_dek|v1").map_err(|_|"主密码错误".to_string())?;if raw.len()!=DEK_LEN{return Err("Vault 密钥无效".into())}let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);if let Some(current)=self.dek{if current!=d{return Err("当前 Vault 密钥校验失败".into())}}Ok(d)}
 pub fn verify_master(&self,pass:&str)->Result<(),String>{let _=self.verify_master_dek(pass)?;Ok(())}
 pub fn enable_recovery(&self,code:&str,questions:&[String],answers:&[String])->Result<(),String>{let dek=self.dek.ok_or("Vault locked")?;if code.trim().chars().count()<20{return Err("恢复码太短".into())}if questions.len()!=3||answers.len()!=3{return Err("必须有 3 组问题/答案".into())}if answers.iter().any(|x|x.trim().is_empty()){return Err("每个密保答案不能为空".into())}let c=self.db()?;let salt=self.salt(&c)?;let combo=format!("{}\0{}\0{}\0{}",code.trim(),answers[0].trim(),answers[1].trim(),answers[2].trim());let rk=Self::derive(&combo,&salt)?;let wrapped=Self::enc(&rk,&dek,b"LocalVault|recovery|v1")?;let qs=serde_json::to_vec(questions).map_err(|e|e.to_string())?;c.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('recovery_wrapped',?),('recovery_questions',?)",params![wrapped,qs]).map_err(|e|e.to_string())?;Ok(())}
 pub fn recovery_verify(&mut self,code:&str,answers:&[String])->Result<(),String>{
   if answers.len()!=3{return Err("恢复答案数量错误".into())}
   self.unprotect_storage()?;
   let result=(||{
     let c=self.db()?;let salt=self.salt(&c)?;let w=self.meta(&c,"recovery_wrapped")?;let combo=format!("{}\0{}\0{}\0{}",code.trim(),answers[0].trim(),answers[1].trim(),answers[2].trim());
     let rk=Self::derive(&combo,&salt)?;let raw=Self::dec(&rk,&w,b"LocalVault|recovery|v1").map_err(|_|"recovery auth failed".to_string())?;
     if raw.len()!=DEK_LEN{return Err("invalid recovery key".into())}
     let mut d=[0u8;DEK_LEN];d.copy_from_slice(&raw);if let Some(mut old)=self.recovery_pending_dek.take(){old.zeroize()}self.recovery_pending_dek=Some(d);Ok(())
   })();
   let _=self.protect_storage();
   result
 }
 pub fn recovery_cancel(&mut self)->Result<(),String>{if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}Ok(())}
 pub fn recovery_set_master(&mut self,new_password:&str,confirm:&str)->Result<(),String>{
   validate_new_master_password(new_password)?;if new_password!=confirm{return Err("两次输入的新主密码不一致".into())}
   let d=*self.recovery_pending_dek.as_ref().ok_or("请先完成恢复验证")?;
   self.unprotect_storage()?;
   let result=(||{
     let c=self.db()?;let salt=self.salt(&c)?;let old_wrapped=self.meta(&c,"wrapped_dek")?;let new_kek=Self::derive(new_password,&salt)?;
     if Self::dec(&new_kek,&old_wrapped,b"LocalVault|wrapped_dek|v1").is_ok(){return Err("新主密码不能与旧主密码相同".into())}
     let wrapped=Self::enc(&new_kek,&d,b"LocalVault|wrapped_dek|v1")?;c.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('wrapped_dek',?)",params![wrapped]).map_err(|e|e.to_string())?;
     Ok(())
   })();
   if result.is_ok(){let _=self.recovery_pending_dek.take();if let Some(mut old)=self.dek.take(){old.zeroize();}}
   let _=self.protect_storage();
   result
 }

 pub fn update_security_settings(&mut self,current_password:&str,new_password:Option<&str>,new_confirm:Option<&str>,questions:Option<&[String]>,answers:Option<&[String]>)->Result<Option<String>,String>{let d=self.verify_master_dek(current_password)?;if new_password.is_none()&&questions.is_none(){return Err("至少选择一项修改".into())}let c=self.db()?;let salt=self.salt(&c)?;let mut new_code=None;let tx=c.unchecked_transaction().map_err(|e|e.to_string())?;if let Some(p)=new_password{let confirm=new_confirm.unwrap_or("");validate_new_master_password(p)?;if p==current_password{return Err("新主密码不能与旧主密码相同".into())}if p!=confirm{return Err("两次输入的新主密码不一致".into())}let kek=Self::derive(p,&salt)?;let wrapped=Self::enc(&kek,&d,b"LocalVault|wrapped_dek|v1")?;tx.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('wrapped_dek',?)",params![wrapped]).map_err(|e|e.to_string())?;}if let Some(qs)=questions{let ans=answers.ok_or("缺少密保答案")?;if qs.len()!=3||ans.len()!=3{return Err("必须有 3 组问题/答案".into())}if qs.iter().any(|q|q.trim().is_empty())||ans.iter().any(|a|a.trim().is_empty()){return Err("密保问题和答案不能为空".into())}let code=self.generate_recovery_code()?;let combo=format!("{}\0{}\0{}\0{}",code,ans[0].trim(),ans[1].trim(),ans[2].trim());let rk=Self::derive(&combo,&salt)?;let wrapped=Self::enc(&rk,&d,b"LocalVault|recovery|v1")?;let raw_qs=serde_json::to_vec(qs).map_err(|e|e.to_string())?;tx.execute("INSERT OR REPLACE INTO meta(key,value)VALUES('recovery_wrapped',?),('recovery_questions',?)",params![wrapped,raw_qs]).map_err(|e|e.to_string())?;new_code=Some(code)}tx.commit().map_err(|e|e.to_string())?;self.dek=Some(d);Ok(new_code)}
}
fn now_ms()->i64{use std::time::{SystemTime,UNIX_EPOCH};SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()as i64}
impl Drop for VaultManager{fn drop(&mut self){if let Some(mut d)=self.dek.take(){d.zeroize()}if let Some(mut d)=self.recovery_pending_dek.take(){d.zeroize()}let _=self.protect_storage();}}
