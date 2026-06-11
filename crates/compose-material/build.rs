//! 自动探测 FFmpeg（vcpkg 优先，其次 pkg-config），找到则编译 `csrc/ffmpeg_shim.c`
//! 并启用 `cfg(compose_ffmpeg)`；否则视频模块只保留 Mock 后端供离线测试。
//!
//! 参考 `/work/study/cui/rui_packages/build.rs`。非桌面目标直接跳过。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const HEADERS: [&str; 3] = [
    "libavformat/avformat.h",
    "libavcodec/avcodec.h",
    "libswscale/swscale.h",
];
const LIBS: [&str; 5] = ["avformat", "avcodec", "avutil", "swresample", "swscale"];

fn has_headers(include: &Path) -> bool {
    HEADERS.iter().all(|h| include.join(h).is_file())
}

fn has_libs(lib: &Path) -> bool {
    fs::read_dir(lib)
        .map(|it| {
            it.filter_map(Result::ok).any(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.starts_with("libavformat.so")
                    || n == "libavformat.a"
                    || n == "avformat.lib"
            })
        })
        .unwrap_or(false)
}

fn candidate_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = env::var("FFMPEG_DIR") {
        out.push(PathBuf::from(dir));
    }
    if let Ok(installed) = env::var("VCPKG_INSTALLED_DIR") {
        out.push(PathBuf::from(installed));
    }
    if let Ok(root) = env::var("VCPKG_ROOT") {
        let installed = PathBuf::from(root).join("installed");
        if let Ok(triplet) = env::var("VCPKG_TARGET_TRIPLET") {
            out.push(installed.join(triplet));
        }
        if let Ok(entries) = fs::read_dir(&installed) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by(|a, b| {
        let rank = |p: &Path| {
            let s = p.to_string_lossy();
            if s.contains("x64-linux-dynamic") || s.contains("x64-windows-dynamic") {
                0
            } else if s.contains(env::var("VCPKG_TARGET_TRIPLET").unwrap_or_default().as_str()) {
                1
            } else {
                2
            }
        };
        rank(a).cmp(&rank(b))
    });
    out
}

fn pkg_config_paths() -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    let run = |flag: &str| -> Vec<PathBuf> {
        Command::new("pkg-config")
            .arg(flag)
            .args(LIBS)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                s.split_whitespace()
                    .filter_map(|t| t.strip_prefix("-I").or_else(|| t.strip_prefix("-L")))
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    let includes = run("--cflags-only-I");
    let mut libs = run("--libs-only-L");
    if libs.is_empty() {
        libs = vec![PathBuf::from("/usr/lib"), PathBuf::from("/usr/lib/x86_64-linux-gnu")];
    }
    if includes.iter().any(|p| has_headers(p)) {
        Some((includes, libs))
    } else {
        None
    }
}

fn build_with(includes: &[PathBuf], lib_dirs: &[PathBuf]) -> bool {
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out.join("ffmpeg_shim.o");
    let lib = out.join("libcompose_ffmpeg_shim.a");
    let cc = env::var("CC").unwrap_or_else(|_| "cc".into());
    let mut cmd = Command::new(&cc);
    cmd.arg("-O2").arg("-fPIC").arg("-c").arg("csrc/ffmpeg_shim.c");
    for inc in includes {
        cmd.arg(format!("-I{}", inc.display()));
    }
    if !cmd.arg("-o").arg(&obj).status().map(|s| s.success()).unwrap_or(false) {
        return false;
    }
    let ar = env::var("AR").unwrap_or_else(|_| "ar".into());
    if !Command::new(ar)
        .arg("rcs")
        .arg(&lib)
        .arg(&obj)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return false;
    }

    println!("cargo:rustc-cfg=compose_ffmpeg");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=compose_ffmpeg_shim");
    for dir in lib_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
        if env::consts::OS == "linux" || env::consts::OS == "macos" {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
        }
    }
    for name in LIBS {
        let kind = if lib_dirs.iter().any(|d| {
            d.join(format!("lib{name}.a")).is_file() || d.join(format!("{name}.lib")).is_file()
        }) {
            "static"
        } else {
            "dylib"
        };
        println!("cargo:rustc-link-lib={kind}={name}");
    }
    true
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(compose_ffmpeg)");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    println!("cargo:rerun-if-changed=csrc/ffmpeg_shim.c");
    for k in ["FFMPEG_DIR", "VCPKG_ROOT", "VCPKG_INSTALLED_DIR", "VCPKG_TARGET_TRIPLET"] {
        println!("cargo:rerun-if-env-changed={k}");
    }
    if !matches!(target_os.as_str(), "linux" | "macos" | "windows") {
        return;
    }

    for root in candidate_roots() {
        let inc = root.join("include");
        let lib = root.join("lib");
        if has_headers(&inc) && has_libs(&lib) {
            if build_with(&[inc], &[lib]) {
                return;
            }
        }
    }
    if let Some((includes, libs)) = pkg_config_paths() {
        if build_with(&includes, &libs) {
            return;
        }
    }
    println!("cargo:warning=FFmpeg not found; compose_material::video falls back to Mock backend");
}
