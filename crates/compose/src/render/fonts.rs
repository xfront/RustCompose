//! 字体集合加载 — 按平台提供默认 `valo::FontCollection`。
//!
//! 桌面策略对齐 rui `make_font_collection`：按类别（拉丁 / CJK / 等宽）各取
//! **一个**字体文件，避免把整个字体目录的 TTC（单个 18–26MB）全部常驻内存。
//! 早期版本最多加载 64 个文件（含 4 个 CJK TTC + Emoji ≈ 100MB），是 story
//! 相对 rui_gallery RSS 翻倍的主因。
//!
//! - Web：无文件系统访问，用启发式默认集合（TODO：嵌入 CJK 字体子集）。
//! - Android：扫描 `/system/fonts`（Roboto / DroidSansFallback），真机可按名加载真实字形。
//! - iOS / OHOS：按各自系统字体目录扫描；不可读时回退启发式度量。

use std::path::{Path, PathBuf};
use valo::FontCollection;

/// 字体候选类别（按加入 fallback 链的顺序排列：拉丁优先，中文缺字后回落 CJK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontKind {
    Latin,
    Cjk,
    Mono,
}

/// 返回平台默认 `FontCollection`。
///
/// 用单一函数 + 内部 `cfg!` 分支（而非 cfg 切分两份同名定义），保证跨 crate
/// 的 `pub use` 重导出始终可见。
pub fn default_font_collection() -> FontCollection {
    // Web：wasm32 无本地文件系统，只能用启发式度量（无真实字形）。
    if cfg!(target_arch = "wasm32") {
        eprintln!("[compose] web: default font collection (heuristic metrics)");
        return FontCollection::default();
    }

    let mut fc = FontCollection::default();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "android") {
        // Android 系统字体：Roboto-Regular.ttf（拉丁）/ DroidSansFallback.ttf（CJK）。
        dirs.push("/system/fonts".into());
    } else if cfg!(target_os = "ios") {
        dirs.push("/System/Library/Fonts".into());
        dirs.push("/System/Library/Fonts/CoreAddition.framework/Resources".into());
    } else if cfg!(target_env = "ohos") {
        dirs.push("/system/fonts".into());
    }
    dirs.extend([
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        PathBuf::from("/usr/share/X11/fonts"),
    ]);
    if cfg!(target_os = "windows") {
        let windir = std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into());
        dirs.push(Path::new(&windir).join("Fonts"));
    } else if cfg!(target_os = "macos") {
        dirs.push("/System/Library/Fonts".into());
        dirs.push("/Library/Fonts".into());
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(Path::new(&home).join("Library/Fonts"));
        }
    }

    // 只扫描路径（stat，不读文件内容），按类别挑最优候选后只读 3 个文件。
    let mut found: Vec<(FontKind, u8, PathBuf)> = Vec::new();
    for dir in &dirs {
        scan_font_dir(dir, &mut found);
    }
    found.sort_by(|a, b| a.0.cmp_kind(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut loaded = 0usize;
    for kind in [FontKind::Latin, FontKind::Cjk, FontKind::Mono] {
        let Some((_, _, path)) = found.iter().find(|(k, _, _)| *k == kind) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        // from_bytes 只解析 face 0：TTC 的其它脸不会被加载（亦避免逐 face 常驻）。
        let Some(mut font) = valo::Font::from_bytes(bytes) else {
            continue;
        };
        match kind {
            FontKind::Latin | FontKind::Cjk => font.add_alias("sans-serif"),
            FontKind::Mono => font.add_alias("monospace"),
        }
        let id = fc.add(font);
        fc.add_fallback(id);
        loaded += 1;
    }
    eprintln!("[compose] Loaded {loaded} font files (latin/cjk/mono)");
    fc
}

impl FontKind {
    /// 固定优先级：拉丁 → CJK → 等宽（fallback 链顺序）。
    fn cmp_kind(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(k: FontKind) -> u8 {
            match k {
                FontKind::Latin => 0,
                FontKind::Cjk => 1,
                FontKind::Mono => 2,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

fn scan_font_dir(dir: &Path, out: &mut Vec<(FontKind, u8, PathBuf)>) {
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
            if let Some((kind, score)) = classify_font(&n) {
                out.push((kind, score, path));
            }
        }
    }
}

/// 分类 + 打分（分数越小越优先），对齐 rui 候选择优思路：
/// - 拉丁：DejaVuSans / LiberationSans / Roboto（排除 Bold/Oblique 变体）；
/// - CJK：**单脸 TTF 优先**（DroidSansFallbackFull 3.8MB），再考虑 NotoSansCJK
///   等 TTC（18MB+，放末位）；排除 Serif/Bold/Emoji；
/// - 等宽：名字含 mono 且不是 CJK。
fn classify_font(n: &str) -> Option<(FontKind, u8)> {
    let mono = n.contains("mono");
    let is_cjk = n.contains("cjk")
        || n.contains("wqy")
        || n.contains("yahei")
        || n.contains("msyh")
        || n.contains("simsun")
        || n.contains("simhei")
        || n.contains("droidsansfallback")
        || n.contains("pingfang");
    let skip_variant = n.contains("bold")
        || n.contains("italic")
        || n.contains("oblique")
        || n.contains("emoji")
        || n.contains("serif");

    if is_cjk || (n.contains("noto") && !mono && !skip_variant) {
        if skip_variant {
            return None;
        }
        let score = if n.contains("droidsansfallback") && n.ends_with(".ttf") {
            0
        } else if n.ends_with(".ttf") || n.ends_with(".otf") {
            1
        } else {
            2 // .ttc 最后
        };
        return Some((FontKind::Cjk, score));
    }
    if mono {
        return Some((FontKind::Mono, if n.contains("dejavu") { 0 } else { 1 }));
    }
    let latin = n.contains("dejavusans")
        || n.contains("liberationsans")
        || n.contains("roboto")
        || n.contains("arial")
        || n.contains("segoeui")
        || n.contains("helvetica");
    if latin && !skip_variant {
        return Some((FontKind::Latin, 0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_classification_picks_one_per_kind() {
        // CJK：单脸 TTF 优先，TTC 在后面；粗体/衬线/Emoji 不入池。
        assert_eq!(
            classify_font("droidsansfallbackfull.ttf"),
            Some((FontKind::Cjk, 0))
        );
        assert_eq!(classify_font("notosanscjk-regular.ttc"), Some((FontKind::Cjk, 2)));
        assert_eq!(classify_font("notosanscjk-bold.ttc"), None);
        assert_eq!(classify_font("notocoloremoji.ttf"), None);
        // 拉丁与等宽分流。
        assert_eq!(classify_font("dejavusans.ttf"), Some((FontKind::Latin, 0)));
        assert_eq!(classify_font("dejavusansmono.ttf"), Some((FontKind::Mono, 0)));
        // 无关字体不加载。
        assert_eq!(classify_font("some-random-font.ttf"), None);
    }

    #[test]
    fn android_system_fonts_are_classified() {
        // Android /system/fonts 的实际命名须能分类，否则真机仍无字形。
        assert_eq!(
            classify_font("roboto-regular.ttf"),
            Some((FontKind::Latin, 0))
        );
        assert_eq!(
            classify_font("droidsansfallback.ttf"),
            Some((FontKind::Cjk, 0))
        );
        assert_eq!(
            classify_font("robotomono-regular.ttf"),
            Some((FontKind::Mono, 1))
        );
    }
}
