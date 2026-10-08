// LocalVault Windows Hello 解锁（DPAPI + Desktop HWND Interop）。
//
// 密钥职责（严格区分，不把 DPAPI 描述成"Hello 直接加密绑定"）：
//
//   DPAPI（CryptProtectData/CryptUnprotectData）
//       → 保护 Windows Hello 解锁路径的 wrapping key（32 字节随机数）
//   XChaCha20-Poly1305
//       → 用 wrapping key 保护 Vault DEK
//   DEK
//       → 实际解密 Vault 数据
//
// Windows Hello 在这里承担"应用解锁门禁"（用户存在/同意验证），
// 不是 TPM 硬件强制的 cryptographic authorization。DPAPI 的具体保护链
// （TPM 是否参与底层保护）由 Windows 环境和配置决定，应用层不作绝对承诺。
//
// 调用链：
//   用户点击「Windows Hello 解锁」
//       ▼
//   IUserConsentVerifierInterop::RequestVerificationForWindowAsync(HWND)
//       ▼ Verified
//   DPAPI Unprotect（hello blob → wrapping key）
//       ▼
//   XChaCha20-Poly1305 解包 DEK
//       ▼
//   vault.db
//
// 存储：
//   - meta 表      : hello_wrapped_dek（wrapping key 加密后的 DEK）
//   - hello.bin    : 版本头 + DPAPI blob（DPAPI 密文绑定当前 Windows 用户/机器）
//
// 安全边界（写进注释，防止审计混淆）：
//   攻击者无法仅凭复制 vault.db 获得 Hello wrapping key（DPAPI 默认绑定
//   用户/机器上下文）；但同一用户上下文中能调用 DPAPI 的恶意代码理论上
//   仍可解出该 blob——本方案的 Windows Hello 验证是应用层门禁，
//   不是 TPM 硬件的 cryptographic authorization。
#![allow(non_snake_case)]

use windows::core::HSTRING;
use windows::Security::Credentials::UI::{UserConsentVerifier, UserConsentVerifierAvailability, UserConsentVerificationResult};
use windows::Win32::System::WinRT::{IUserConsentVerifierInterop, RoGetActivationFactory};
use windows::Win32::Foundation::{HWND as WinRtHWND, HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// hello.bin 文件版本头。
const BLOB_MAGIC: &[u8; 8] = b"LVHELLO2";

/// 设备是否支持 Windows Hello（指纹/人脸/PIN 任一可用）。
pub fn hello_supported() -> bool {
    match UserConsentVerifier::CheckAvailabilityAsync()
        .and_then(|op| op.get())
    {
        Ok(avail) => avail == UserConsentVerifierAvailability::Available,
        Err(_) => false,
    }
}

/// 启用前置条件：Windows Hello 可用即可（DPAPI 无需前置检查，总是可用）。
pub fn can_enable() -> bool {
    hello_supported()
}

/// 全局单次 Hello 验证锁：任何时候只允许一个 Windows Hello 验证流程，
/// 防止用户重复点击导致多个未完成的 IAsyncOperation 并存。
static HELLO_IN_PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RAII 锁守卫：离开作用域（成功/失败/超时）必定释放锁。
struct HelloGuard;
impl HelloGuard {
    fn try_acquire() -> Option<Self> {
        HELLO_IN_PROGRESS
            .compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst)
            .ok()
            .map(|_| HelloGuard)
    }
}
impl Drop for HelloGuard {
    fn drop(&mut self) {
        HELLO_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 弹系统生物验证框（Desktop 正确用法：绑定主窗口 HWND），返回是否验证通过。
/// 带 60 秒超时护栏 + 取消回收：超时后调用 IAsyncInfo::Cancel() 取消异步操作，
/// 等待其进入终止状态，再 Close() 释放，绝不让挂起的线程/操作泄漏。
pub fn request_verification_with_window(message: &str, hwnd: isize) -> Result<bool, String> {
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_future::IAsyncOperation;

    let _guard = HelloGuard::try_acquire().ok_or("Windows Hello 验证正在进行，请稍候…")?;

    // Desktop App 官方路径：通过 RoGetActivationFactory 取得
    // IUserConsentVerifierInterop，用 RequestVerificationForWindowAsync(HWND, ...)
    // 把验证框绑定到 Tauri 主窗口（而非无主窗口的后台 WinRT 操作）。
    let interop: IUserConsentVerifierInterop =
        unsafe { RoGetActivationFactory(&HSTRING::from("Windows.Security.Credentials.UI.UserConsentVerifier")) }
            .map_err(|e| format!("无法获取 Windows Hello Interop 接口：{e}"))?;

    let hwnd = WinRtHWND(hwnd as *mut core::ffi::c_void);
    let op: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        interop.RequestVerificationForWindowAsync(hwnd, &HSTRING::from(message))
    }
    .map_err(|e| format!("Windows Hello 不可用：{e}"))?;

    let op_for_thread = op.clone();
    let (tx, rx) = mpsc::channel::<Option<bool>>();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(
            op_for_thread
                .get()
                .map(|r| r == UserConsentVerificationResult::Verified)
                .ok()
        );
    });

    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(Some(true)) => Ok(true),
        Ok(Some(false)) => Ok(false),
        Ok(None) => Err("Windows Hello 验证失败".into()),
        Err(_) => {
            // 超时：取消异步操作，等 worker 线程结束（get() 会随 Cancel 返回），再释放资源
            let _ = op.Cancel();
            let _ = worker.join();
            // 操作已进入终止状态，可安全 Close 释放
            let _ = op.Close();
            Err("Windows Hello 验证超时（60 秒未响应）".into())
        }
    }
}

/// DPAPI 保护数据（当前用户+机器上下文，TPM 是否参与由 Windows 决定）。
pub fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(|e| format!("DPAPI 加密失败：{e}"))?;
    if out.pbData.is_null() || out.cbData == 0 {
        return Err("DPAPI 加密失败：返回空数据".into());
    }
    let result = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void))); }
    Ok(result)
}

/// DPAPI 解保护数据。
pub fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            0,
            &mut out,
        )
    }
    .map_err(|e| format!("DPAPI 解密失败：{e}"))?;
    if out.pbData.is_null() || out.cbData == 0 {
        return Err("DPAPI 解密失败：返回空数据".into());
    }
    let result = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void))); }
    Ok(result)
}

/// hello.bin：版本头 + 数据长度 + DPAPI blob。
pub fn encode_blob(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + 4 + payload.len());
    out.extend_from_slice(BLOB_MAGIC);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// 解析 hello.bin。
pub fn decode_blob(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.len() < 12 || &raw[..8] != BLOB_MAGIC {
        return Err("hello.bin 版本不匹配或已损坏".into());
    }
    let len = u32::from_le_bytes(raw[8..12].try_into().unwrap_or_default()) as usize;
    if len == 0 || 12 + len > raw.len() {
        return Err("hello.bin 长度无效".into());
    }
    Ok(raw[12..12 + len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DPAPI 全链路验证：保护 → 解保护 → 内容一致。
    #[test]
    fn dpapi_roundtrip() {
        let secret = b"hello-wrapping-key-test-0123456789abcdef";
        let protected = dpapi_protect(secret).expect("DPAPI 保护失败");
        eprintln!("DPAPI_PROTECT_LEN={}", protected.len());
        let plain = dpapi_unprotect(&protected).expect("DPAPI 解保护失败");
        assert_eq!(plain.as_slice(), secret, "解保护结果应与原文一致");
        eprintln!("DPAPI_ROUNDTRIP_OK");
    }

    /// blob 编解码往返。
    #[test]
    fn blob_roundtrip() {
        let payload = b"some-dpapi-blob-bytes";
        let encoded = encode_blob(payload);
        assert!(encoded.starts_with(BLOB_MAGIC));
        let decoded = decode_blob(&encoded).expect("blob 解析失败");
        assert_eq!(decoded.as_slice(), payload);
        eprintln!("BLOB_ROUNDTRIP_OK");
    }
}
