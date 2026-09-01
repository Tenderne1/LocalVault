# Compile fix — current Windows step

本版本针对 Windows 上一次编译日志修复了：
- `build.rs` 使用 `tauri_build::build()` 所需的 `[build-dependencies] tauri-build`
- `if let Some(...)` 中 SQLite `execute()` 返回 `usize` 的分号问题
- `if old_path.exists()` / 临时恢复文件删除中的 Rust 表达式类型问题
- Tauri command 中 `.into()` 类型推断歧义，统一为明确的 `String::from(...)`

运行：
```powershell
cargo clean --manifest-path .\src-tauri\Cargo.toml
npm.cmd run tauri:dev
```
