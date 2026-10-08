// LocalVault 窗口防截屏（Windows SetWindowDisplayAffinity）。
//
// 用途：解锁后的主界面在截图 / 录屏 / 远程桌面 / Win+Shift+S 中
// 显示为空白（WDA_EXCLUDEFROMCAPTURE，Win10 2004+）或黑色块
// （WDA_MONITOR 回退，Win8+），内容被遮挡，无法被捕获。
//
// 注意：
//   - 这是"遮挡"而非"防破解"：操作系统层不保证绝对不可捕获，
//     极少数第三方录屏可能仍能看到窗口。它与主密码加密互为补充，
//     不替代加密本身。
//   - 开启后用户自己的截图工具同样截不到该窗口，属预期行为。
#![allow(non_snake_case)]

use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, GetWindowDisplayAffinity,
    WDA_EXCLUDEFROMCAPTURE, WDA_MONITOR, WDA_NONE,
};
use windows::Win32::Foundation::HWND;

/// 对窗口应用 / 解除防截屏。
/// enabled=true 时优先 WDA_EXCLUDEFROMCAPTURE（最彻底），失败回退 WDA_MONITOR。
pub fn set_window_protect(hwnd: isize, enabled: bool) -> Result<(), String> {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    if !enabled {
        unsafe { SetWindowDisplayAffinity(h, WDA_NONE) }
            .map_err(|e| format!("解除防截屏失败：{e}"))?;
        return Ok(());
    }
    // 先尝试最彻底的排除模式
    if unsafe { SetWindowDisplayAffinity(h, WDA_EXCLUDEFROMCAPTURE) }.is_ok() {
        return Ok(());
    }
    // 系统较旧或驱动不支持时回退到"截图显示为黑色块"
    unsafe { SetWindowDisplayAffinity(h, WDA_MONITOR) }
        .map_err(|e| format!("设置防截屏失败：{e}"))
}

/// 当前窗口是否处于防截屏状态（用于状态一致性检查）。
pub fn window_protect_active(hwnd: isize) -> bool {
    let h = HWND(hwnd as *mut core::ffi::c_void);
    let mut affinity: u32 = 0;
    if unsafe { GetWindowDisplayAffinity(h, &mut affinity) }.is_err() {
        return false;
    }
    affinity != 0
}
