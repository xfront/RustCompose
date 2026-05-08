#!/bin/bash
# build_web.sh — 构建 compose（已合并原 compose-platform）示例的 WASM 浏览器版本。
#
# 说明：
#   采用 winit 0.30 web 后端（wasm32-unknown-unknown + wasm-bindgen + wgpu WebGPU/WebGL）。
#   Web 路线：先 `cargo build --target wasm32-unknown-unknown` 产出
#   `gallery.wasm`，再经 `wasm-bindgen` CLI 生成 JS 胶水；浏览器侧由 winit 的
#   `spawn_app` 接管事件循环与 Canvas。
#
# 前置条件：
#   1. rustup target add wasm32-unknown-unknown
#   2. (可选) cargo install wasm-bindgen-cli   # 生成 web_dist JS 胶水
#   3. (可选) python3                          # 本地静态服务器预览
#
# 用法：
#   ./scripts/build_web.sh              # release 构建 + 生成 web_dist（若有 wasm-bindgen）
#   ./scripts/build_web.sh debug        # 调试构建
#   ./scripts/build_web.sh serve        # 启动 python 静态服务器预览 web_dist

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TARGET="wasm32-unknown-unknown"
EXAMPLE="gallery"

if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "ERROR: 未安装 Rust 目标 $TARGET"
    echo "  rustup target add $TARGET"
    exit 1
fi

cd "$WS_ROOT"

PROFILE="--release"
case "${1:-release}" in
    serve)  PROFILE="--release" ;;
    debug)  PROFILE="" ;;
    release|*) PROFILE="--release" ;;
esac

echo "==> 构建 $EXAMPLE ($TARGET ${PROFILE:+release})"
cargo build $PROFILE --target "$TARGET" --example "$EXAMPLE"

WASM="target/$TARGET/${PROFILE:+release/}examples/$EXAMPLE.wasm"
if [ ! -f "$WASM" ]; then
    WASM="target/$TARGET/release/examples/$EXAMPLE.wasm"
fi
if [ ! -f "$WASM" ]; then
    echo "ERROR: 未找到产物 $WASM"
    exit 1
fi
echo "  wasm: $WASM ($(du -h "$WASM" | cut -f1))"

# 生成 JS 胶水（wasm-bindgen 可选）
DIST_DIR="$WS_ROOT/web_dist"
if command -v wasm-bindgen >/dev/null 2>&1; then
    echo "==> wasm-bindgen 生成胶水 -> $DIST_DIR"
    mkdir -p "$DIST_DIR"
    wasm-bindgen "$WASM" --out-dir "$DIST_DIR" --target web --no-typescript
    cat > "$DIST_DIR/index.html" <<HTML
<!doctype html>
<html><head><meta charset="utf-8"/><title>compose gallery (wasm)</title>
<style>html,body{margin:0;height:100%;background:#111}canvas{display:block}</style></head>
<body><script type="module">
import init from './$EXAMPLE.js';
init().catch(e => { document.body.innerHTML = '<pre style="color:#f55">'+e+'</pre>'; console.error(e); });
</script></body></html>
HTML
    echo "  产物: $DIST_DIR/index.html + $EXAMPLE.js + ${EXAMPLE}_bg.wasm "
else
    echo "==> 未安装 wasm-bindgen-cli，仅产出原始 wasm。"
    echo "   如需浏览器运行：cargo install wasm-bindgen-cli 后重跑本脚本。"
fi

if [ "${1:-release}" = "serve" ]; then
    cd "$DIST_DIR"
    echo "==> 启动静态服务器: http://127.0.0.1:7070"
    exec python3 -m http.server 7070
fi

echo ""
echo "构建成功！"
echo "  原始 wasm: $WASM"
[ -d "$DIST_DIR" ] && echo "  web_dist:  $DIST_DIR (python3 -m http.server 7070 --directory $DIST_DIR)"
