#!/usr/bin/env bash
# build_cross.sh — 一键交叉编译 story 示例到 WASM 与 Android。
#
# 用法：
#   ./scripts/build_cross.sh            # 依次构建 wasm32 + aarch64-linux-android（release）
#   ./scripts/build_cross.sh wasm       # 只构建 Web/WASM
#   ./scripts/build_cross.sh android    # 只构建 Android（.so；用 build_apk.sh 打包 APK）
#
# 前置条件：见 build_web.sh / build_android.sh 的说明。
#   - wasm:  rustup target add wasm32-unknown-unknown；可选 wasm-bindgen-cli
#   - android: ANDROID_NDK 或 $ANDROID_HOME/ndk/*；rustup target add aarch64-linux-android

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$WS_ROOT"

TARGET="${1:-all}"
case "$TARGET" in
    wasm|web)
        bash "$SCRIPT_DIR/build_web.sh" release
        ;;
    android)
        bash "$SCRIPT_DIR/build_android.sh" aarch64-linux-android
        ;;
    all)
        echo "════════════ WASM (wasm32-unknown-unknown) ════════════"
        bash "$SCRIPT_DIR/build_web.sh" release
        echo ""
        echo "════════════ Android (aarch64-linux-android) ════════════"
        bash "$SCRIPT_DIR/build_android.sh" aarch64-linux-android
        ;;
    *)
        echo "用法: $0 [all|wasm|android]"
        exit 2
        ;;
esac

echo ""
echo "════════════ 完成 ════════════"
echo "  WASM:    target/wasm32-unknown-unknown/release/examples/story.wasm"
echo "  Android: target/aarch64-linux-android/release/examples/story"
echo "  APK:     ./scripts/build_apk.sh"
