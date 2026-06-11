//! 给示例二进制补 FFmpeg 动态库 RUNPATH。
//!
//! `compose-material/build.rs` 的 `rustc-link-arg` 只作用于自身包的目标，
//! 不会传播到 `examples` 包。链接到的 `libav*.so` 因此只在 `cargo test -p
//! compose-material` 里能找到，直接运行 `target/release/examples/story`
//! 会报 `libavformat.so.63: cannot open shared object file`。这里按同样的
//! vcpkg / pkg-config 探测给本包的可执行文件补 `-Wl,-rpath`。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn has_ffmpeg_lib(dir: &PathBuf) -> bool {
    dir.join("libavformat.so").is_file()
        || fs::read_dir(dir)
            .map(|it| {
                it.filter_map(Result::ok).any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("libavformat.so")
                })
            })
            .unwrap_or(false)
}

fn candidate_lib_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push_root = |root: PathBuf| {
        out.push(root.join("lib"));
        out.push(root.join("lib64"));
    };
    if let Ok(dir) = env::var("FFMPEG_DIR") {
        push_root(PathBuf::from(dir));
    }
    if let Ok(dir) = env::var("VCPKG_INSTALLED_DIR") {
        let p = PathBuf::from(dir);
        push_root(p.clone());
        if let Ok(entries) = fs::read_dir(&p) {
            for e in entries.flatten() {
                if e.path().is_dir() {
                    push_root(e.path());
                }
            }
        }
    }
    if let Ok(root) = env::var("VCPKG_ROOT") {
        if let Ok(entries) = fs::read_dir(PathBuf::from(root).join("installed")) {
            for e in entries.flatten() {
                if e.path().is_dir() {
                    push_root(e.path());
                }
            }
        }
    }
    if let Ok(out_libs) = Command::new("pkg-config")
        .args(["--libs-only-L", "libavformat", "libavcodec", "libavutil"])
        .output()
    {
        if out_libs.status.success() {
            if let Ok(s) = String::from_utf8(out_libs.stdout) {
                for token in s.split_whitespace() {
                    if let Some(dir) = token.strip_prefix("-L") {
                        out.push(PathBuf::from(dir));
                    }
                }
            }
        }
    }
    out
}

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKG_INSTALLED_DIR");
    println!("cargo:rerun-if-env-changed=VCPKG_TARGET_TRIPLET");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !matches!(target_os.as_str(), "linux" | "macos") {
        return;
    }
    for dir in candidate_lib_dirs() {
        if has_ffmpeg_lib(&dir) {
            if target_os == "linux" {
                // DT_RPATH 也覆盖间接依赖；macOS 不支持该开关。
                println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
            }
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
            break;
        }
    }
}
