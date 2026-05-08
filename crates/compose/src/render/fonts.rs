//! 字体集合加载 — 按平台提供默认 `valo::FontCollection`。
//!
//! - 桌面：扫描系统字体目录，CJK / 等宽 / UI 字体加 alias（简化版）。
//! - Web / Android / iOS / OHOS：暂用启发式默认集合（TODO：嵌入 CJK 字体子集）。

use std::path::Path;
use std::sync::Arc;

use valo::FontCollection;

/// 返回平台默认 `FontCollection`。
///
/// - 桌面：扫描系统字体目录，按优先级加入 CJK / 等宽 / UI 别名（简化版）。
/// - Web / Android / iOS / OHOS：暂用启发式默认集合（TODO：嵌入 CJK 字体子集）。
///
/// 用单一函数 + 内部 `cfg!` 分支（而非 cfg 切分两份同名定义），保证跨 crate
/// 的 `pub use` 重导出始终可见（详见 compose-render lib.rs 的 `pub use fonts::default_font_collection`）。
pub fn default_font_collection() -> FontCollection {
    if cfg!(any(
        target_arch = "wasm32",
        target_os = "android",
        target_env = "ohos",
        target_os = "ios"
    )) {
        let tag = if cfg!(target_os = "android") {
            "android"
        } else if cfg!(target_arch = "wasm32") {
            "web"
        } else if cfg!(target_os = "ios") {
            "ios"
        } else {
            "ohos"
        };
        eprintln!("[compose] {tag}: default font collection (heuristic metrics)");
        FontCollection::default()
    } else {
        let mut fc = FontCollection::default();
        let mut dirs: Vec<std::path::PathBuf> = vec![
            "/usr/share/fonts".into(),
            "/usr/local/share/fonts".into(),
            "/usr/share/X11/fonts".into(),
        ];
        if cfg!(target_os = "windows") {
            // Windows：字体不在 Unix 路径下，只扫 Unix 目录会一个都加载不到（文本整片空白）。
            let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
            dirs.push(std::path::Path::new(&windir).join("Fonts"));
        } else if cfg!(target_os = "macos") {
            dirs.push("/System/Library/Fonts".into());
            dirs.push("/Library/Fonts".into());
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(std::path::Path::new(&home).join("Library/Fonts"));
            }
        }
        let mut files: Vec<(u8, std::path::PathBuf)> = Vec::new();
        for dir in &dirs {
            scan_font_dir(dir, &mut files);
        }
        files.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        files.dedup_by(|a, b| a.1 == b.1);
        let mut count = 0;
        for (_prio, path) in files {
            if let Ok(bytes) = std::fs::read(&path) {
                let fonts = valo::Font::instances_from_data(Arc::new(bytes), 0);
                for mut font in fonts {
                    let fam = font.family().to_ascii_lowercase();
                    if (fam.starts_with("roboto") || fam.contains("arial") || fam.contains("segoe"))
                        && !fam.contains("mono")
                    {
                        font.add_alias("sans-serif");
                    } else if fam.contains("mono") {
                        font.add_alias("monospace");
                    }
                    let id = fc.add(font);
                    fc.add_fallback(id);
                }
                count += 1;
                if count >= 64 {
                    break;
                }
            }
        }
        eprintln!("[compose] Loaded {count} font files");
        fc
    }
}

fn scan_font_dir(dir: &Path, out: &mut Vec<(u8, std::path::PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("ttf") | Some("otf") | Some("ttc")
        ) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let n = name.to_ascii_lowercase();
            let mono = n.contains("mono");
            let latin_ui = (n.contains("roboto") && !mono)
                || (n.contains("dejavusans") && !mono)
                || n.contains("liberationsans")
                || n.contains("arial")
                || n.contains("segoeui")
                || n.contains("helvetica");
            let prio = if latin_ui {
                0
            } else if n.contains("cjk")
                || n.contains("noto")
                || n.contains("wqy")
                || n.contains("yahei")
                || n.contains("msyh")
                || n.contains("simsun")
                || n.contains("simhei")
            {
                1
            } else if mono {
                3
            } else {
                2
            };
            out.push((prio, path));
        }
    }
}
