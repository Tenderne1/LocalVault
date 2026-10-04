// LocalVault 手机端桌面调试入口（Windows 上可跑 tauri dev 预览 UI）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    localvault_mobile_lib::run()
}
