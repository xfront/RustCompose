//! 应用根注入点（cdylib 目标 `story_android`）——对应 cui 的 `examples/app_root.cpp`。
//!
//! 平台宿主入口 `android_main` 与完整事件循环归引擎层
//! `compose::platform::android`（对应 cui 的 `cui_core/src/platform/android/cpp/main.cpp`）；
//! 本文件只实现引擎声明的 `compose_create_app_root` 链接期符号，装配具体应用 Widget 树。
//! 依赖方向严格单向：examples（应用）→ compose（引擎）；引擎不认识 story。
//! 最终 .so 由本 lib target 产出：链接引擎 rlib 带入 `android_main`，
//! 并由本 crate 提供 `compose_create_app_root` 解析其链接期引用。
//! 非 Android 目标仅产出空 rlib，不参与宿主入口。

#[cfg(target_os = "android")]
#[path = "common.rs"]
mod common;
#[cfg(target_os = "android")]
#[path = "prelude.rs"]
mod prelude;
#[cfg(target_os = "android")]
#[path = "shell.rs"]
mod shell;
#[cfg(target_os = "android")]
#[path = "pages/mod.rs"]
mod pages;

/// 引擎宿主按链接期符号查找此函数，取得要挂到 Kernel 的应用根。
///
/// `#[unsafe(no_mangle)]` 保证符号名不被改名，与
/// `compose::platform::android` 中的 `extern "Rust" { fn compose_create_app_root() }`
/// 对齐；只链引擎不链本实现会在链接期报未解析符号（刻意的装配错误）。
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "Rust" fn compose_create_app_root() -> Box<dyn compose::App + 'static> {
    compose_material::install_router();
    Box::new(shell::StoryApp { dark: false, palette: 0 })
}
