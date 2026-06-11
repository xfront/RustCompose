#!/bin/bash
# build_web.sh — 构建 story 示例的 Web/WASM 版本。
#
# 路线：wasm32-unknown-unknown + winit 0.30 web 后端（spawn_app / rAF）+ wgpu WebGPU/WebGL。
#   cargo build --target wasm32-unknown-unknown → story.wasm
#   wasm-bindgen（可选）→ web_dist/story.js + story_bg.wasm + index.html
#
# 前置条件：
#   1. rustup target add wasm32-unknown-unknown
#   2. (可选) cargo install wasm-bindgen-cli   # 生成浏览器 JS 胶水（需与 wasm-bindgen crate 版本一致）
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
EXAMPLE="story"

if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "==> 安装 Rust 目标 $TARGET"
    rustup target add "$TARGET"
fi

cd "$WS_ROOT"

MODE="${1:-release}"
case "$MODE" in
    debug)  PROFILE_ARG=""; PROFILE_DIR="debug" ;;
    serve|release|*) PROFILE_ARG="--release"; PROFILE_DIR="release" ;;
esac

echo "==> 构建 $EXAMPLE ($TARGET / $PROFILE_DIR)"
cargo build $PROFILE_ARG --target "$TARGET" -p examples --example "$EXAMPLE"

WASM="target/$TARGET/$PROFILE_DIR/examples/$EXAMPLE.wasm"
if [ ! -f "$WASM" ]; then
    echo "ERROR: 未找到产物 $WASM"
    exit 1
fi
echo "  wasm: $WASM ($(du -h "$WASM" | cut -f1))"

DIST_DIR="$WS_ROOT/web_dist"
if command -v wasm-bindgen >/dev/null 2>&1; then
    echo "==> wasm-bindgen 生成胶水 -> $DIST_DIR"
    mkdir -p "$DIST_DIR"
    wasm-bindgen "$WASM" --out-dir "$DIST_DIR" --target web --no-typescript
    cat > "$DIST_DIR/index.html" <<HTML
<!doctype html>
<html><head><meta charset="utf-8"/><title>$EXAMPLE — compose (wasm)</title>
<style>html,body{margin:0;height:100%;background:#111;overflow:hidden}canvas{display:block;width:100%;height:100%}</style></head>
<body><script type="module">
import init from './$EXAMPLE.js';
init().catch(e => { document.body.innerHTML = '<pre style="color:#f55;padding:16px">'+e+'</pre>'; console.error(e); });
</script></body></html>
HTML
    echo "  产物: $DIST_DIR/index.html + $EXAMPLE.js + ${EXAMPLE}_bg.wasm"
else
    echo "==> 未安装 wasm-bindgen-cli，仅产出原始 wasm。"
    echo "   如需浏览器运行：cargo install wasm-bindgen-cli 后重跑本脚本。"
fi

if [ "$MODE" = "serve" ]; then
    [ -d "$DIST_DIR" ] || { echo "ERROR: 无 web_dist 可预览，请先安装 wasm-bindgen-cli"; exit 1; }
    cd "$DIST_DIR"
    echo "==> 启动静态服务器: http://127.0.0.1:7070"
    exec python3 -m http.server 7070
fi

echo ""
echo "构建成功！"
echo "  原始 wasm: $WASM"
[ -d "$DIST_DIR" ] && echo "  web_dist:  $DIST_DIR"
