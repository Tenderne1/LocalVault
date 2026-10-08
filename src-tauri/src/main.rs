#![cfg_attr(not(debug_assertions),windows_subsystem="windows")]
mod autofill;
mod sync;
mod vault;
mod hello;
mod screen;
use std::sync::{Arc,Mutex};
use std::process::Command;
use tauri::{Manager,RunEvent,State};
use tauri::Emitter;
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri_plugin_autostart::MacosLauncher;
use autofill::AutofillBridge;
use sync::SyncBridge;
use vault::{BackupData,BackupSettings,BootstrapStatus,Category,Entry,HistoryRecord,SecurityPolicy,UnlockSecurityState,VaultManager};
#[tauri::command] fn diagnostics_settings_get(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::DiagnosticsSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.diagnostics_settings()}
#[tauri::command] fn diagnostics_settings_set(enabled:bool,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::DiagnosticsSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.set_diagnostics_enabled(enabled)}
#[tauri::command] fn diagnostics_log_clear(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.clear_diagnostics_log()}
#[tauri::command] fn diagnostics_log_path(state:State<'_,Arc<Mutex<VaultManager>>>)->String{state.lock().map(|v|v.diagnostics_log_path_string()).unwrap_or_default()}
#[tauri::command] fn diagnostics_log_read(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<vault::DiagnosticsLogLine>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.read_diagnostics_log(None)}
#[tauri::command] fn hello_status(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::HelloStatus,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.hello_status()}

/// 取主窗口 HWND（isize），供 Windows Hello Desktop Interop 使用。
fn main_hwnd(app:&tauri::AppHandle)->Result<isize,String>{
  let w=app.get_webview_window("main").ok_or_else(||"找不到主窗口".to_string())?;
  #[cfg(windows)]{
    let hwnd=w.hwnd().map_err(|e|format!("获取窗口句柄失败：{e}"))?;
    return Ok(hwnd.0 as isize)
  }
  #[cfg(not(windows))]{let _=w;Err("仅支持 Windows".into())}
}
#[tauri::command] async fn hello_enable(app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::HelloStatus,String>{let hwnd=main_hwnd(&app)?;let vm=state.inner().clone();tauri::async_runtime::spawn_blocking(move||{vm.lock().map_err(|_|"state lock poisoned".to_string())?.enable_hello(hwnd)?;vm.lock().map_err(|_|"state lock poisoned".to_string())?.hello_status()}).await.map_err(|e|e.to_string())?}
#[tauri::command] async fn hello_disable(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::HelloStatus,String>{let vm=state.inner().clone();tauri::async_runtime::spawn_blocking(move||{vm.lock().map_err(|_|"state lock poisoned".to_string())?.disable_hello()?;vm.lock().map_err(|_|"state lock poisoned".to_string())?.hello_status()}).await.map_err(|e|e.to_string())?}
#[tauri::command] async fn hello_unlock(app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<vault::Entry>,String>{let hwnd=main_hwnd(&app)?;let vm=state.inner().clone();let r=tauri::async_runtime::spawn_blocking(move||vm.lock().map_err(|_|"state lock poisoned".to_string())?.unlock_with_hello(hwnd)).await.map_err(|e|e.to_string())?;if r.is_ok(){sync_screen_protect(&app,&state);}r}
#[tauri::command] fn clipboard_settings_get(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::ClipboardSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.clipboard_settings()}
#[tauri::command] fn clipboard_settings_set(clear_seconds:u32,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::ClipboardSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.set_clipboard_clear_seconds(clear_seconds)}
#[tauri::command] fn screen_protect_get(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::ScreenProtectSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.screen_protect_settings()}

/// 对主窗口应用防截屏状态。enabled 仅当设置开启时才调用；失败静默（不影响主流程）。
fn apply_screen_protect(app:&tauri::AppHandle,enabled:bool){
  if let Ok(hwnd)=main_hwnd(app){
    let _=screen::set_window_protect(hwnd,enabled);
  }
}

/// 根据「设置是否开启 + Vault 是否解锁」决定窗口防截屏状态。
fn sync_screen_protect(app:&tauri::AppHandle,state:&State<'_,Arc<Mutex<VaultManager>>>){
  let enabled=state.lock().ok().and_then(|v|v.screen_protect_settings().ok()).map(|s|s.enabled).unwrap_or(false);
  let unlocked=state.lock().map(|v|v.is_unlocked()).unwrap_or(false);
  apply_screen_protect(app,enabled&&unlocked);
}

#[tauri::command] fn screen_protect_set(enabled:bool,app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<vault::ScreenProtectSettings,String>{
  let s=state.lock().map_err(|_|"state lock poisoned".to_string())?.set_screen_protect_enabled(enabled)?;
  sync_screen_protect(&app,&state);
  Ok(s)
}


#[derive(serde::Serialize)]
struct BrowserInfo{path:String,name:String,icon:Option<String>}

fn browser_display_name(path:&str)->&str{
    let file=path.rsplit(['\\','/']).next().unwrap_or(path);
    let low=file.to_ascii_lowercase();
    match low.as_str(){
        "msedge.exe"=>"Microsoft Edge",
        "chrome.exe"=>"Google Chrome",
        "firefox.exe"=>"Mozilla Firefox",
        "brave.exe"=>"Brave",
        "launcher.exe"=>"Opera",
        "vivaldi.exe"=>"Vivaldi",
        "360se.exe"=>"360 安全浏览器",
        "qqbrowser.exe"=>"QQ 浏览器",
        "sogouexplorer.exe"=>"搜狗高速浏览器",
        "liebao.exe"=>"猎豹浏览器",
        _=>file,
    }
}

#[cfg(target_os="windows")]
fn extract_browser_icon(path:&str)->Option<String>{
    use base64::Engine;
    use windows::core::{BOOL,GUID,PCWSTR};
    use windows::Win32::Graphics::GdiPlus::*;
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon,HICON};
    let wide:Vec<u16>=path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut large=HICON(std::ptr::null_mut());
    let got=unsafe{ExtractIconExW(PCWSTR(wide.as_ptr()),0,Some(&mut large),None,1)};
    if got==0||large.is_invalid(){return None;}
    let mut token:usize=0;
    let input=GdiplusStartupInput{GdiplusVersion:1,DebugEventCallback:0,SuppressBackgroundThread:BOOL(0),SuppressExternalCodecs:BOOL(0)};
    let mut output=GdiplusStartupOutput{NotificationHook:0,NotificationUnhook:0};
    if unsafe{GdiplusStartup(&mut token,&input,&mut output)}.0!=0{let _=unsafe{DestroyIcon(large)};return None;}
    let mut bitmap=std::ptr::null_mut();
    let status=unsafe{GdipCreateBitmapFromHICON(large,&mut bitmap)};
    let _=unsafe{DestroyIcon(large)};
    if status.0!=0||bitmap.is_null(){unsafe{GdiplusShutdown(token)};return None;}
    let png_clsid=GUID{data1:0x557CF406,data2:0x1A04,data3:0x11D3,data4:[0x9A,0x73,0x00,0x00,0xF8,0x1E,0xF3,0x2E]};
    let tmp=std::env::temp_dir().join(format!("lvicon-{}.png",std::process::id()));
    let tmp_wide:Vec<u16>=tmp.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let saved=unsafe{GdipSaveImageToFile(bitmap as *mut GpImage,PCWSTR(tmp_wide.as_ptr()),&png_clsid,std::ptr::null())};
    unsafe{GdipDisposeImage(bitmap as *mut GpImage);GdiplusShutdown(token)};
    if saved.0!=0{return None;}
    let data=std::fs::read(&tmp).ok()?;
    let _=std::fs::remove_file(&tmp);
    Some(base64::engine::general_purpose::STANDARD.encode(&data))
}
#[tauri::command]
fn detect_browsers()->Vec<BrowserInfo>{
    #[cfg(target_os="windows")]
    {
        let home=std::env::var("LOCALAPPDATA").unwrap_or_default();
        let pf=std::env::var("ProgramFiles").unwrap_or_default();
        let pf86=std::env::var("ProgramFiles(x86)").unwrap_or_default();
        let mut candidates=vec![
            format!(r"{pf}\Microsoft\Edge\Application\msedge.exe"),
            format!(r"{pf86}\Microsoft\Edge\Application\msedge.exe"),
            format!(r"{home}\Microsoft\Edge\Application\msedge.exe"),
            format!(r"{pf}\Google\Chrome\Application\chrome.exe"),
            format!(r"{pf86}\Google\Chrome\Application\chrome.exe"),
            format!(r"{home}\Google\Chrome\Application\chrome.exe"),
            format!(r"{pf}\Mozilla Firefox\firefox.exe"),
            format!(r"{home}\Mozilla Firefox\firefox.exe"),
            format!(r"{pf}\BraveSoftware\Brave-Browser\Application\brave.exe"),
            format!(r"{home}\BraveSoftware\Brave-Browser\Application\brave.exe"),
            format!(r"{pf}\Opera\launcher.exe"),
            format!(r"{home}\Programs\Opera\launcher.exe"),
            format!(r"{home}\Programs\Opera GX\launcher.exe"),
            format!(r"{pf}\Vivaldi\Application\vivaldi.exe"),
            format!(r"{pf86}\360\360se6\Application\360se.exe"),
            format!(r"{pf86}\Tencent\QQBrowser\QQBrowser.exe"),
            format!(r"{home}\Tencent\QQBrowser\QQBrowser.exe"),
            format!(r"{pf86}\SogouExplorer\SogouExplorer.exe"),
            format!(r"{pf}\liebao\liebao.exe"),
        ];
        candidates.dedup();
        let mut out=Vec::new();
        for p in candidates.iter().filter(|p|std::path::Path::new(p).exists()){
            out.push(BrowserInfo{path:p.clone(),name:browser_display_name(p).to_string(),icon:extract_browser_icon(p)});
        }
        out
    }
    #[cfg(not(target_os="windows"))]
    { Vec::new() }
}

#[tauri::command]
fn open_url_in_browser(url:String,browser:Option<String>)->Result<(),String>{
    if !url.starts_with("http://") && !url.starts_with("https://"){return Err("只允许打开 http/https 网址".into());}
    if let Some(path)=browser.filter(|x|!x.trim().is_empty()){
        Command::new(path).arg(&url).spawn().map_err(|e|format!("无法启动所选浏览器：{}",e))?;
        Ok(())
    }else{
        #[cfg(target_os="windows")] {
            tauri_plugin_opener::open_url(&url,None::<&str>).map_err(|e|format!("无法用系统默认浏览器打开：{}",e))?;
            Ok(())
        }
        #[cfg(not(target_os="windows"))] {
            Err("当前平台暂未实现系统默认浏览器打开".into())
        }
    }
}

// 剪贴板安全：写入密码时在剪贴板上注册并设置 ExcludeClipboardContentFromMonitorProcessing 格式，
// 系统因此不会把本次内容收录进 Win+V 剪贴板历史，也不会同步到云剪贴板。
// 只使用这一个格式：CanIncludeInClipboardHistory / CanUploadToCloudClipboard 是"允许进历史/上云"的
// 语义（存在即生效），与排除格式同时存在时语义冲突，可能让密码仍被收录，故不设置。
#[cfg(target_os="windows")]
fn register_secure_clipboard_formats()->Result<u32,String>{
    use windows::core::PCWSTR;
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;

    let name="ExcludeClipboardContentFromMonitorProcessing";
    let wide:Vec<u16>=name.encode_utf16().chain(std::iter::once(0)).collect();
    let id=unsafe{RegisterClipboardFormatW(PCWSTR(wide.as_ptr()))};
    if id==0{
        return Err(format!("注册 Windows 安全剪贴板格式失败：{}",name));
    }
    Ok(id)
}

#[cfg(target_os="windows")]
unsafe fn alloc_clipboard_hglobal(data:&[u8])->Result<windows::Win32::Foundation::HGLOBAL,String>{
    use windows::Win32::Foundation::{GlobalFree,HGLOBAL};
    use windows::Win32::System::Memory::{GlobalAlloc,GlobalLock,GlobalUnlock,GMEM_MOVEABLE};

    let hmem=GlobalAlloc(GMEM_MOVEABLE,data.len())
        .map_err(|_|"分配剪贴板内存失败".to_string())?;
    let p=GlobalLock(hmem);
    if p.is_null(){
        let _=GlobalFree(Some(hmem));
        return Err("锁定剪贴板内存失败".into());
    }
    std::ptr::copy_nonoverlapping(data.as_ptr(),p as *mut u8,data.len());
    let _=GlobalUnlock(hmem);
    Ok(HGLOBAL(hmem.0))
}

#[cfg(target_os="windows")]
fn is_localvault_secure_clipboard()->bool{
    let Ok(exclude_monitor)=register_secure_clipboard_formats() else {return false};
    use windows::Win32::System::DataExchange::{CloseClipboard,IsClipboardFormatAvailable,OpenClipboard};

    unsafe{
        if OpenClipboard(None).is_err(){return false;}
        let guarded=IsClipboardFormatAvailable(exclude_monitor).is_ok();
        let _=CloseClipboard();
        guarded
    }
}

#[tauri::command]
fn clipboard_clear()->Result<(),String>{
    #[cfg(target_os="windows")]{
        // 只清除 LocalVault 自己写入的安全剪贴板，避免误删用户随后复制的普通内容。
        if !is_localvault_secure_clipboard(){return Ok(())}
        use windows::Win32::System::DataExchange::{OpenClipboard,EmptyClipboard,CloseClipboard};
        for _ in 0..3{
            unsafe{
                if OpenClipboard(None).is_err(){std::thread::sleep(std::time::Duration::from_millis(60));continue}
                // 在真正清空前再次确认安全标记仍属于当前剪贴板。
                let guarded=register_secure_clipboard_formats().map(|exclude_monitor|{
                    windows::Win32::System::DataExchange::IsClipboardFormatAvailable(exclude_monitor).is_ok()
                }).unwrap_or(false);
                if !guarded{
                    let _=CloseClipboard();
                    return Ok(());
                }
                let ok_empty=EmptyClipboard().is_ok();
                let _=CloseClipboard();
                if ok_empty{return Ok(())}
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
        }
        Err("清空 LocalVault 安全剪贴板失败（可能被其他程序占用）".into())
    }
    #[cfg(not(target_os="windows"))]{ Err("当前平台暂未实现剪贴板清空".into()) }
}

#[cfg(target_os="windows")]
fn write_clipboard_text_win(text:&str)->Result<(),String>{
    use windows::Win32::System::DataExchange::{OpenClipboard,EmptyClipboard,CloseClipboard,SetClipboardData};
    use windows::Win32::Foundation::{GlobalFree,HANDLE,HGLOBAL};
    const CF_UNICODETEXT:u32=13;

    let exclude_monitor=register_secure_clipboard_formats()?;

    let mut wide:Vec<u16>=text.encode_utf16().collect();
    wide.push(0);
    let text_bytes=unsafe{
        std::slice::from_raw_parts(wide.as_ptr() as *const u8,wide.len()*2)
    };
    let exclude_bytes=[1u8];

    for _ in 0..3{
        unsafe{
            if OpenClipboard(None).is_err(){
                std::thread::sleep(std::time::Duration::from_millis(60));
                continue
            }
            if EmptyClipboard().is_err(){
                let _=CloseClipboard();
                std::thread::sleep(std::time::Duration::from_millis(60));
                continue
            }

            let Ok(h_text)=alloc_clipboard_hglobal(text_bytes) else {
                let _=CloseClipboard();
                return Err("分配剪贴板文本内存失败".into())
            };
            let Ok(h_exclude)=alloc_clipboard_hglobal(&exclude_bytes) else {
                let _=GlobalFree(Some(HGLOBAL(h_text.0)));
                let _=CloseClipboard();
                return Err("分配剪贴板监控保护标记内存失败".into())
            };

            let mut transferred_text=false;
            let mut transferred_exclude=false;

            if SetClipboardData(CF_UNICODETEXT,Some(HANDLE(h_text.0))).is_ok(){
                transferred_text=true;
            }else{
                let _=GlobalFree(Some(HGLOBAL(h_text.0)));
            }

            if transferred_text && SetClipboardData(exclude_monitor,Some(HANDLE(h_exclude.0))).is_ok(){
                transferred_exclude=true;
            }else if !transferred_exclude{
                let _=GlobalFree(Some(HGLOBAL(h_exclude.0)));
            }

            let ok=transferred_text && transferred_exclude;
            if !ok{
                // 不能留下一个只含明文、却没有安全标记的密码剪贴板。
                let _=EmptyClipboard();
                let _=CloseClipboard();
                std::thread::sleep(std::time::Duration::from_millis(60));
                continue
            }

            let _=CloseClipboard();
            return Ok(())
        }
    }
    Err("写入安全剪贴板失败（可能被其他程序占用）".into())
}

fn secure_copy_text(text:&str)->Result<(),String>{
    #[cfg(target_os="windows")]{
        // Windows 原生 Win+V/Cloud Clipboard 保护在写入剪贴板的同一时刻完成。
        // 不再调用 Clipboard::ClearHistory()，避免删除用户原有的剪贴板历史。
        write_clipboard_text_win(text)?;
        Ok(())
    }
    #[cfg(not(target_os="windows"))]{ Err("当前平台暂未实现剪贴板写入".into()) }
}

#[tauri::command]
fn copy_secure(text:String)->Result<(),String>{secure_copy_text(&text)}
#[tauri::command] fn vault_status(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BootstrapStatus,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.status()}
#[tauri::command] fn vault_list(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.list()}
#[tauri::command] fn vault_create(master_password:String,confirm_password:String,app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>,bridge:State<'_,Mutex<AutofillBridge>>,sync_bridge:State<'_,Mutex<SyncBridge>>)->Result<(),String>{
    let r=state.lock().map_err(|_|"state lock poisoned".to_string())?.create(&master_password,&confirm_password);
    if r.is_ok(){
        // 创建后 Vault 即处于解锁状态：与解锁一致地启动浏览器填充（若已启用）与局域网同步服务
        if let Ok(mut b)=bridge.lock(){b.on_vault_unlocked(&*state);}
        if let Ok(mut b)=sync_bridge.lock(){b.on_vault_unlocked(&*state);}
        // 解锁后的主界面：若已开启防截屏则应用窗口遮挡
        sync_screen_protect(&app,&state);
    }
    r
}
#[tauri::command] fn vault_unlock(master_password:String,captcha:Option<String>,recovery_code:Option<String>,recovery_answers:Option<Vec<String>>,app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>,bridge:State<'_,Mutex<AutofillBridge>>,sync_bridge:State<'_,Mutex<SyncBridge>>)->Result<Vec<Entry>,String>{
    let r=state.lock().map_err(|_|"state lock poisoned".to_string())?.unlock(&master_password,captcha.as_deref(),recovery_code.as_deref(),recovery_answers.as_deref());
    if r.is_ok(){
        // 浏览器填充：仅当总开关已启用时自动启动（用户手动开关驱动）
        if let Ok(mut b)=bridge.lock(){b.on_vault_unlocked(&*state);}
        // 局域网同步：解锁后自动启动服务（供手机客户端连接）
        if let Ok(mut b)=sync_bridge.lock(){b.on_vault_unlocked(&*state);}
        // 解锁后的主界面：若已开启防截屏则应用窗口遮挡
        sync_screen_protect(&app,&state);
    }
    r
}
#[tauri::command] fn vault_unlock_security_state(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<UnlockSecurityState,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.security_state()}
#[tauri::command] fn vault_refresh_captcha(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<String,String>{let mut v=state.lock().map_err(|_|"state lock poisoned".to_string())?;let s=v.security_state()?;if !s.captcha_required{return Err("当前无需验证码".into())}Ok(v.new_captcha())}
#[tauri::command] fn security_policy_get(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<SecurityPolicy,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.security_policy()}
#[tauri::command] fn security_policy_set(policy:SecurityPolicy,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<SecurityPolicy,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.set_security_policy(&policy)}

#[tauri::command] fn vault_save(entries:Vec<Entry>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.save(&entries)}
#[tauri::command] fn vault_lock(app:tauri::AppHandle,state:State<'_,Arc<Mutex<VaultManager>>>,bridge:State<'_,Mutex<AutofillBridge>>,sync_bridge:State<'_,Mutex<SyncBridge>>)->Result<(),String>{
    // 先关闭浏览器填充服务与局域网同步服务，再锁 Vault，避免锁定时出现服务线程/数据库锁顺序问题。
    // bridge 锁失败时也必须继续锁 Vault（安全核心），并向上报告服务未停止的异常。
    let mut problems=Vec::new();
    match bridge.lock() {
        Ok(mut b) => { b.vault_locked(); }
        Err(_) => problems.push("浏览器填充服务状态异常未停止"),
    };
    match sync_bridge.lock() {
        Ok(mut b) => { b.vault_locked(); }
        Err(_) => problems.push("局域网同步服务状态异常未停止"),
    };
    state.lock().map_err(|_|"state lock poisoned".to_string())?.lock()?;
    // 锁定后解除窗口防截屏（解锁界面本身不遮挡，避免用户看不到自己的操作）
    apply_screen_protect(&app,false);
    if !problems.is_empty() {
        return Err(format!("Vault 已锁定，但{}；请重启程序以确保安全", problems.join("、")));
    }
    Ok(())
}
// 查看单条密码：前端传 id + 会话密文，后端仅解密这一条返回明文；明文不常驻前端
#[tauri::command]
fn vault_entry_password(entry_id:String,password_encrypted:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<String,String>{
    state.lock().map_err(|_|"state lock poisoned".to_string())?.entry_password(&entry_id,&password_encrypted)
}
// 复制密码：后端解密后直接写入安全剪贴板，密码明文不经过前端
#[tauri::command]
fn vault_copy_password(entry_id:String,password_encrypted:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{
    let mut plain=state.lock().map_err(|_|"state lock poisoned".to_string())?.entry_password(&entry_id,&password_encrypted)?;
    let r=secure_copy_text(&plain);
    plain.clear();
    r
}
// 复制条目为副本：后端读源条目，构造新副本返回；不写盘，由前端编辑后保存
#[tauri::command]
fn vault_duplicate_entry(source_id:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Entry,String>{
    state.lock().map_err(|_|"state lock poisoned".to_string())?.duplicate_entry(&source_id)
}
#[tauri::command] fn vault_backup(destination:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.backup(&destination)}
#[tauri::command] fn backup_settings_get(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BackupSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.backup_settings()}
#[tauri::command] fn backup_settings_set(enabled:bool,directory:Option<String>,retention:usize,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BackupSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.set_backup_settings(enabled,directory,retention)}
#[tauri::command] fn backup_now(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BackupSettings,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.backup_now()}
#[tauri::command] fn vault_restore(backup_path:String,master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.restore(&backup_path,&master_password)}
#[tauri::command] fn vault_backup_preview(backup_path:String,master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BackupData,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.backup_preview(&backup_path,&master_password)}
#[tauri::command] fn vault_backup_verify(backup_path:String,master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<BackupData,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.verify_backup(&backup_path,&master_password)}
#[tauri::command] fn vault_backup_merge(backup_path:String,master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.merge_backup(&backup_path,&master_password)}
#[tauri::command] fn vault_export(entry_ids:Vec<String>,destination:String,master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.export_entries(&entry_ids,&destination,&master_password)}
#[tauri::command] fn vault_import(source:String,source_master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.import_entries(&source,&source_master_password)}
#[tauri::command] fn bulk_import_template(destination:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.write_bulk_import_template(&destination)}
#[tauri::command] fn bulk_import_read(source:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<String,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.read_bulk_import_file(&source)}
#[tauri::command] fn trash_move(entry_id:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.move_to_trash(&entry_id)}
#[tauri::command] fn trash_list(retention_days:i64,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.list_trash(retention_days)}
#[tauri::command] fn trash_restore(entry_id:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Entry>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.restore_trash(&entry_id)}
#[tauri::command] fn trash_purge(entry_id:Option<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.purge_trash(entry_id)}
#[tauri::command] fn category_update(old_name:String,name:String,icon:String,parent_name:Option<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.update_category(&old_name,&name,&icon,parent_name.as_deref())}
#[tauri::command] fn category_delete(name:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.delete_category(&name)}
#[tauri::command] fn history_list(entry_id:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<HistoryRecord>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.history_list(&entry_id)}
#[tauri::command] fn recovery_generate_code(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<String,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.generate_recovery_code()}
#[tauri::command] fn recovery_enable(recovery_code:String,recovery_questions:Vec<String>,recovery_answers:Vec<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.enable_recovery(&recovery_code,&recovery_questions,&recovery_answers)}
#[tauri::command] fn recovery_questions(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<String>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.recovery_questions()}
#[tauri::command] fn recovery_verify(recovery_code:String,answers:Vec<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.recovery_verify(&recovery_code,&answers)}
#[tauri::command] fn recovery_cancel(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.recovery_cancel()}
#[tauri::command] fn recovery_set_master(new_password:String,confirm_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.recovery_set_master(&new_password,&confirm_password)}
#[tauri::command] fn vault_verify_master(master_password:String,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.verify_master(&master_password)}
#[tauri::command] fn vault_update_security(current_password:String,new_password:Option<String>,new_confirm:Option<String>,questions:Option<Vec<String>>,answers:Option<Vec<String>>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Option<String>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.update_security_settings(&current_password,new_password.as_deref(),new_confirm.as_deref(),questions.as_deref(),answers.as_deref())}
#[tauri::command] fn category_list(state:State<'_,Arc<Mutex<VaultManager>>>)->Result<Vec<Category>,String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.list_categories()}
#[tauri::command] fn category_create(name:String,icon:String,parent_name:Option<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.create_category(&name,&icon,parent_name.as_deref())}
#[tauri::command] fn category_reorder(parent_name:Option<String>,names:Vec<String>,state:State<'_,Arc<Mutex<VaultManager>>>)->Result<(),String>{state.lock().map_err(|_|"state lock poisoned".to_string())?.reorder_categories(parent_name.as_deref(),&names)}

// ---------- 浏览器填充桥命令 ----------
#[tauri::command]
fn autofill_status(bridge:State<'_,Mutex<AutofillBridge>>,vault:State<'_,Arc<Mutex<VaultManager>>>)->Result<autofill::AutofillStatus,String>{
    let b=bridge.lock().map_err(|_|"state lock poisoned".to_string())?;
    let mut s=b.status();
    s.vault_unlocked=vault.lock().map_err(|_|"state lock poisoned".to_string())?.is_unlocked();
    Ok(s)
}
#[tauri::command]
fn autofill_toggle(enabled:bool,bridge:State<'_,Mutex<AutofillBridge>>,vault:State<'_,Arc<Mutex<VaultManager>>>)->Result<autofill::AutofillStatus,String>{
    let vault_arc=vault.inner().clone();
    let mut b=bridge.lock().map_err(|_|"state lock poisoned".to_string())?;
    b.set_enabled(enabled,&vault_arc)?;
    let mut s=b.status();
    s.vault_unlocked=vault.lock().map_err(|_|"state lock poisoned".to_string())?.is_unlocked();
    Ok(s)
}
#[tauri::command]
fn autofill_begin_pair(bridge:State<'_,Mutex<AutofillBridge>>)->Result<String,String>{bridge.lock().map_err(|_|"state lock poisoned".to_string())?.begin_pair()}
#[tauri::command]
fn autofill_unpair_all(bridge:State<'_,Mutex<AutofillBridge>>)->Result<(),String>{bridge.lock().map_err(|_|"state lock poisoned".to_string())?.unpair_all()}

// ---------- 局域网同步桥命令 ----------
#[tauri::command]
fn sync_status(bridge:State<'_,Mutex<SyncBridge>>,vault:State<'_,Arc<Mutex<VaultManager>>>)->Result<sync::SyncStatus,String>{
    let b=bridge.lock().map_err(|_|"state lock poisoned".to_string())?;
    let mut s=b.status();
    s.vault_unlocked=vault.lock().map_err(|_|"state lock poisoned".to_string())?.is_unlocked();
    Ok(s)
}
#[tauri::command]
fn sync_begin_pair(bridge:State<'_,Mutex<SyncBridge>>)->Result<sync::SyncPairInfo,String>{bridge.lock().map_err(|_|"state lock poisoned".to_string())?.begin_pair()}
#[tauri::command]
fn sync_unpair_all(bridge:State<'_,Mutex<SyncBridge>>)->Result<(),String>{bridge.lock().map_err(|_|"state lock poisoned".to_string())?.unpair_all()}

fn main(){
    tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![])))
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        // 已有实例运行时，聚焦其主窗口而不是启动第二个实例，
        // 避免多实例同时绑定 38527 端口造成"服务未运行但端口被占"的假象。
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }))
    .manage(Arc::new(Mutex::new(VaultManager::new())))
    .manage(Mutex::new(AutofillBridge::new()))
    .manage(Mutex::new(SyncBridge::new()))
    .setup(|app|{
        use tauri::menu::{Menu,MenuItem};
        // 注入 AppHandle 到同步桥：手机推送成功后向前端 emit sync-updated（前端自动刷新，无需锁定再解锁）
        {
            let handle=app.handle().clone();
            let sb=app.state::<Mutex<SyncBridge>>();
            let mut b=sb.lock().map_err(|_|"sync state lock poisoned".to_string())?;
            b.set_app(handle);
        }
        let fill_item=MenuItem::with_id(app,"autofill","🌐 浏览器填充",true,None::<&str>).map_err(|e|e.to_string())?;
        let sync_item=MenuItem::with_id(app,"phonesync","📱 手机同步",true,None::<&str>).map_err(|e|e.to_string())?;
        let menu=Menu::with_items(app,&[&fill_item,&sync_item]).map_err(|e|e.to_string())?;
        app.set_menu(menu).map_err(|e|e.to_string())?;
        app.on_menu_event(move|app,event|{
            if event.id().as_ref()=="autofill"{
                let _=app.emit("open-autofill-dialog",());
            }else if event.id().as_ref()=="phonesync"{
                let _=app.emit("open-phone-sync-dialog",());
            }
        });
        // 系统托盘：点 X 最小化到托盘后可从托盘恢复；托盘菜单可退出程序
        let show_item=MenuItem::with_id(app,"tray-show","显示 LocalVault",true,None::<&str>).map_err(|e|e.to_string())?;
        let quit_item=MenuItem::with_id(app,"tray-quit","退出",true,None::<&str>).map_err(|e|e.to_string())?;
        let tray_menu=Menu::with_items(app,&[&show_item,&quit_item]).map_err(|e|e.to_string())?;
        let default_icon=app.default_window_icon().cloned().ok_or("未找到应用图标")?;
        TrayIconBuilder::with_id("main-tray")
            .icon(default_icon)
            .menu(&tray_menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app,event|{
                match event.id.as_ref(){
                    "tray-show"=>{
                        if let Some(w)=app.get_webview_window("main"){
                            let _=w.show();
                            let _=w.set_focus();
                        }
                    }
                    "tray-quit"=>{
                        app.exit(0);
                    }
                    _=>{}
                }
            })
            .on_tray_icon_event(|tray,event|{
                if let TrayIconEvent::Click{button:tauri::tray::MouseButton::Left,button_state:tauri::tray::MouseButtonState::Up,..}=event{
                    let app=tray.app_handle();
                    if let Some(w)=app.get_webview_window("main"){
                        let _=w.show();
                        let _=w.set_focus();
                    }
                }
            })
            .build(app).map_err(|e|e.to_string())?;
        Ok(())
    })
    .invoke_handler(tauri::generate_handler![diagnostics_settings_get,diagnostics_settings_set,diagnostics_log_clear,diagnostics_log_path,diagnostics_log_read,hello_status,hello_enable,hello_disable,hello_unlock,clipboard_settings_get,clipboard_settings_set,screen_protect_get,screen_protect_set,security_policy_get,security_policy_set,vault_status,vault_list,vault_create,vault_unlock,vault_unlock_security_state,vault_refresh_captcha,detect_browsers,open_url_in_browser,clipboard_clear,copy_secure,vault_save,vault_lock,vault_entry_password,vault_copy_password,vault_duplicate_entry,vault_backup,backup_settings_get,backup_settings_set,backup_now,vault_restore,vault_backup_preview,vault_backup_verify,vault_backup_merge,vault_export,vault_import,bulk_import_template,bulk_import_read,trash_move,trash_list,trash_restore,trash_purge,category_update,category_delete,history_list,recovery_generate_code,recovery_enable,recovery_questions,recovery_verify,recovery_cancel,recovery_set_master,vault_verify_master,vault_update_security,category_list,category_create,category_reorder,autofill_status,autofill_toggle,autofill_begin_pair,autofill_unpair_all,sync_status,sync_begin_pair,sync_unpair_all])
    .build(tauri::generate_context!()).expect("error while building LocalVault")
    .run(|app: &tauri::AppHandle, event: RunEvent|{
        if matches!(event,RunEvent::Exit){
            let state=app.state::<Arc<Mutex<VaultManager>>>();
            let result=state.lock();
            if let Ok(mut vault)=result{let _=vault.lock();}
            let bridge=app.state::<Mutex<AutofillBridge>>();
            let bridge_guard=bridge.lock();
            if let Ok(mut b)=bridge_guard{b.stop();}
            let sync_bridge=app.state::<Mutex<SyncBridge>>();
            let sync_guard=sync_bridge.lock();
            if let Ok(mut b)=sync_guard{b.stop();}
        }
    });
}
