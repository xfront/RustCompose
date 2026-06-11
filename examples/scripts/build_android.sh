#!/bin/bash
# build_android.sh — 构建 story 示例的 Android 目标（android-activity NativeActivity）。
#
# 前置条件：
#   1. 安装 Android NDK r26d+（https://developer.android.com/ndk/downloads）
#      会自动从 $ANDROID_HOME/$ANDROID_SDK_ROOT 的 ndk/ 目录探测；也可用 ANDROID_NDK 显式指定。
#   2. rustup target add aarch64-linux-android
#   3. (可选) cargo install cargo-ndk
#
# 用法：
#   ./scripts/build_android.sh [target]
#   默认 target: aarch64-linux-android

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EXAMPLE="story"
TARGET="${1:-aarch64-linux-android}"
API_LEVEL="${API_LEVEL:-26}"

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [ -z "${ANDROID_NDK:-}" ] && [ -n "$SDK" ]; then
    ANDROID_NDK="$(ls -1d "$SDK"/ndk/*/ 2>/dev/null | sort -V | tail -1)"
    ANDROID_NDK="${ANDROID_NDK%/}"
fi
if [ -z "${ANDROID_NDK:-}" ]; then
    echo "ERROR: ANDROID_NDK 环境变量未设置，且未从 SDK 探测到 NDK。"
    echo "  export ANDROID_NDK=/path/to/android-ndk-r26d"
    exit 1
fi

if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "==> 安装 Rust 目标: $TARGET"
    rustup target add "$TARGET"
fi

HOST_TAG="linux-x86_64"
case "$(uname -s)" in
    Darwin)  HOST_TAG="darwin-x86_64" ;;
    Linux)   HOST_TAG="linux-x86_64" ;;
    MINGW*|MSYS*|CYGWIN*) HOST_TAG="windows-x86_64" ;;
esac

TOOLCHAIN="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG"
case "$TARGET" in
    aarch64-linux-android) CLANG_PREFIX="aarch64-linux-android${API_LEVEL}" ;;
    x86_64-linux-android)  CLANG_PREFIX="x86_64-linux-android${API_LEVEL}" ;;
    *) echo "ERROR: 不支持的目标: $TARGET（可用 aarch64-linux-android / x86_64-linux-android）"; exit 1 ;;
esac
[ -d "$TOOLCHAIN" ] || { echo "ERROR: 未找到 NDK 工具链: $TOOLCHAIN"; exit 1; }

TARGET_US="${TARGET//-/_}"
export "CC_${TARGET_US}=${CLANG_PREFIX}-clang"
export "CXX_${TARGET_US}=${CLANG_PREFIX}-clang++"
export "AR_${TARGET_US}=llvm-ar"
export "CARGO_TARGET_${TARGET_US^^}_LINKER=$TOOLCHAIN/bin/${CLANG_PREFIX}-clang"
export PATH="$TOOLCHAIN/bin:$PATH"

echo "构建 Android 目标:"
echo "  Example:  $EXAMPLE"
echo "  Target:   $TARGET"
echo "  NDK:      $ANDROID_NDK  (API $API_LEVEL)"

cd "$WS_ROOT"
# 同时构建 bin（story）与 cdylib（story_android → libstory_android.so，供 APK 使用）。
if command -v cargo-ndk >/dev/null 2>&1; then
    cargo ndk --target "$TARGET" --api-level "$API_LEVEL" build -p examples --example "$EXAMPLE" --lib --release
else
    cargo build --target "$TARGET" -p examples --example "$EXAMPLE" --lib --release
fi

OUTPUT="target/$TARGET/release/examples/$EXAMPLE"
SO="target/$TARGET/release/libstory_android.so"
if [ -f "$SO" ] && [ -f "$OUTPUT" ]; then
    echo ""
    echo "构建成功！"
    echo "  bin:   $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
    echo "  cdylib: $SO ($(du -h "$SO" | cut -f1))"
    echo "打包 APK: ./scripts/build_apk.sh"
else
    [ -f "$SO" ] || echo "ERROR: 未找到 cdylib 产物: $SO"
    [ -f "$OUTPUT" ] || echo "ERROR: 未找到 bin 产物: $OUTPUT"
    exit 1
fi
