// Android JNI 桥：指纹/面部生物识别 + 系统分享面板
// 通过静态方法调用 MainActivity（持有 Activity 实例），Rust 侧轮询结果

#[cfg(target_os = "android")]
pub fn biometric_available() -> bool {
    call_static_bool("bioAvailable").unwrap_or(false)
}

/// 弹出系统生物识别，等待用户操作（最多 30 秒）
#[cfg(target_os = "android")]
pub fn biometric_authenticate() -> Result<bool, String> {
    call_static_void("startBiometricAuth")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let r = read_static_int("bioResult")?;
        if r == 1 {
            return Ok(true);
        }
        if r == 0 {
            return Ok(false);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[cfg(target_os = "android")]
pub fn share_text(text: &str, subject: &str) {
    let _ = with_env(|env, cls| {
        let t = env.new_string(text).map_err(|e| e.to_string())?;
        let s = env.new_string(subject).map_err(|e| e.to_string())?;
        let _ = env.call_static_method(
            cls,
            "shareText",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[jni::objects::JValue::Object(&t), jni::objects::JValue::Object(&s)],
        );
        Ok(())
    });
}

/// 调用系统安装器安装自动更新下载的 APK（FileProvider + ACTION_VIEW）
#[cfg(target_os = "android")]
pub fn install_apk(path: &str) -> Result<String, String> {
    with_env(|env, cls| {
        let t = env.new_string(path).map_err(|e| e.to_string())?;
        let v = env
            .call_static_method(
                cls,
                "installApk",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[jni::objects::JValue::Object(&t)],
            )
            .map_err(|e| e.to_string())?;
        let obj = v.l().map_err(|e| e.to_string())?;
        if obj.is_null() {
            return Ok("failed".into());
        }
        let s: String = env
            .get_string(&obj.into())
            .map_err(|e| e.to_string())?
            .into();
        Ok(s)
    })
}

/// 打开系统浏览器访问外部 URL（蓝奏云下载页等）
#[cfg(target_os = "android")]
pub fn open_url(url: &str) -> Result<String, String> {
    with_env(|env, cls| {
        let t = env.new_string(url).map_err(|e| e.to_string())?;
        let v = env
            .call_static_method(
                cls,
                "openUrl",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[jni::objects::JValue::Object(&t)],
            )
            .map_err(|e| e.to_string())?;
        let obj = v.l().map_err(|e| e.to_string())?;
        if obj.is_null() {
            return Ok("failed".into());
        }
        let s: String = env
            .get_string(&obj.into())
            .map_err(|e| e.to_string())?
            .into();
        Ok(s)
    })
}

/// 跳转系统指纹录入设置（多级回退），返回已打开的页面 action（"opened:..." / "failed"）
#[cfg(target_os = "android")]
pub fn open_biometric_settings() -> Result<String, String> {
    with_env(|env, cls| {
        let v = env
            .call_static_method(cls, "openBiometricSettings", "()Ljava/lang/String;", &[])
            .map_err(|e| e.to_string())?;
        let obj = v.l().map_err(|e| e.to_string())?;
        if obj.is_null() {
            return Ok("failed".into());
        }
        let s: String = env
            .get_string(&obj.into())
            .map_err(|e| e.to_string())?
            .into();
        Ok(if s.is_empty() { "failed".into() } else { format!("opened:{s}") })
    })
}

// ---------- 指纹会话密钥持久化（Keystore 加密落盘，冷启动后指纹仍可用） ----------

/// 持久化 bio_key（base64 字符串交给 Java 侧 Keystore 加密存储）
#[cfg(target_os = "android")]
pub fn save_bio_key(b64: &str) -> bool {
    with_env(|env, cls| {
        let t = env.new_string(b64).map_err(|e| e.to_string())?;
        let v = env
            .call_static_method(cls, "saveBioKey", "(Ljava/lang/String;)Z", &[jni::objects::JValue::Object(&t)])
            .map_err(|e| e.to_string())?;
        Ok(v.z().unwrap_or(false))
    })
    .unwrap_or(false)
}

/// 读取持久化的 bio_key（base64 字符串，无则 None）
#[cfg(target_os = "android")]
pub fn load_bio_key() -> Option<String> {
    with_env(|env, cls| {
        let v = env
            .call_static_method(cls, "loadBioKey", "()Ljava/lang/String;", &[])
            .map_err(|e| e.to_string())?;
        let obj = v.l().map_err(|e| e.to_string())?;
        if obj.is_null() {
            return Ok(None);
        }
        let s: String = env
            .get_string(&obj.into())
            .map_err(|e| e.to_string())?
            .into();
        Ok(if s.is_empty() { None } else { Some(s) })
    })
    .ok()
    .flatten()
}

/// 删除持久化的 bio_key
#[cfg(target_os = "android")]
pub fn delete_bio_key() {
    let _ = with_env(|env, cls| {
        let _ = env.call_static_method(cls, "deleteBioKey", "()V", &[]);
        Ok(())
    });
}

/// 前端确认退出：结束 MainActivity（两次返回确认后调用）
#[cfg(target_os = "android")]
pub fn finish_activity() {
    let _ = with_env(|env, cls| {
        let _ = env.call_static_method(cls, "finishActivity", "()V", &[]);
        Ok(())
    });
}

#[cfg(target_os = "android")]
fn call_static_bool(method: &str) -> Result<bool, String> {
    with_env(|env, cls| {
        let v = env
            .call_static_method(cls, method, "()Z", &[])
            .map_err(|e| e.to_string())?;
        Ok(v.z().unwrap_or(false))
    })
}

#[cfg(target_os = "android")]
fn call_static_void(method: &str) -> Result<(), String> {
    with_env(|env, cls| {
        env.call_static_method(cls, method, "()V", &[])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

#[cfg(target_os = "android")]
fn read_static_int(field: &str) -> Result<i32, String> {
    with_env(|env, cls| {
        let f = env
            .get_static_field(cls, field, "I")
            .map_err(|e| e.to_string())?;
        Ok(f.i().unwrap_or(0))
    })
}

#[cfg(target_os = "android")]
fn with_env<T>(f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JClass) -> Result<T, String>) -> Result<T, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
    match vm.get_env() {
        // 当前线程已 attach：直接复用
        Ok(mut env) => run_with_env(&mut env, &ctx, f),
        // 未 attach：attach（AttachGuard 是 RAII，作用域结束自动 detach，避免线程泄漏）
        Err(_) => {
            let mut guard = vm.attach_current_thread().map_err(|e| e.to_string())?;
            run_with_env(&mut guard, &ctx, f)
        }
    }
}

#[cfg(target_os = "android")]
fn run_with_env<T>(
    env: &mut jni::JNIEnv,
    ctx: &ndk_context::AndroidContext,
    f: impl FnOnce(&mut jni::JNIEnv, jni::objects::JClass) -> Result<T, String>,
) -> Result<T, String> {
    // 关键：native 线程的 FindClass 使用系统 classloader，找不到 app 类（ClassNotFoundException）。
    // 必须用 App Context 的 classLoader.loadClass 加载 MainActivity。
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let loader = env
        .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let name = env
        .new_string("com.localvault.mobile.MainActivity")
        .map_err(|e| e.to_string())?;
    let cls = env
        .call_method(
            &loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[jni::objects::JValue::Object(&name)],
        )
        .map_err(|e| e.to_string())?
        .l()
        .map_err(|e| e.to_string())?;
    let jcls = jni::objects::JClass::from(cls);
    let r = f(env, jcls);
    // 关键：JNI 调用失败时 Java 侧会挂起异常，若不清理，Tauri 的 postMessage IPC 桥会
    // 检测到挂起异常并报 "Java exception was raised during method invocation"（整个命令失败）。
    let _ = env.exception_clear();
    r
}

// ---------- 非 Android（桌面调试） ----------

#[cfg(not(target_os = "android"))]
pub fn biometric_available() -> bool {
    false
}

#[cfg(not(target_os = "android"))]
pub fn biometric_authenticate() -> Result<bool, String> {
    Err("仅支持 Android 设备".into())
}

#[cfg(not(target_os = "android"))]
pub fn share_text(_text: &str, _subject: &str) {}

#[cfg(not(target_os = "android"))]
pub fn install_apk(_path: &str) -> Result<String, String> {
    Ok("failed".into())
}

#[cfg(not(target_os = "android"))]
pub fn open_url(_url: &str) -> Result<String, String> {
    Ok("failed".into())
}

#[cfg(not(target_os = "android"))]
pub fn open_biometric_settings() -> Result<String, String> {
    Ok("failed".into())
}

#[cfg(not(target_os = "android"))]
pub fn save_bio_key(_b64: &str) -> bool {
    false
}

#[cfg(not(target_os = "android"))]
pub fn load_bio_key() -> Option<String> {
    None
}

#[cfg(not(target_os = "android"))]
pub fn delete_bio_key() {}

#[cfg(not(target_os = "android"))]
pub fn finish_activity() {}
