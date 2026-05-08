#!/bin/bash
# build_android.sh — 构建 compose（已合并原 compose-platform）示例的 Android 目标 (aarch64-linux-android)
#
# 前置条件：
#   1. 安装 Android NDK r26d+（https://developer.android.com/ndk/downloads）
#   2. rustup target add aarch64-linux-android
#   3. (可选) cargo install cargo-ndk
#
# 用法：
#   ./scripts/build_android.sh [example_name] [target]
#   默认：gallery / aarch64-linux-android

set -euo pipefail

EXAMPLE="${1:-gallery}"
TARGET="${2:-aarch64-linux-android}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -z "${ANDROID_NDK:-}" ]; then
    echo "ERROR: ANDROID_NDK 环境变量未设置。"
    echo "  export ANDROID_NDK=/path/to/android-ndk-r26d"
    exit 1
fi

if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "安装 Rust 目标: $TARGET"
    rustup target add "$TARGET"
fi

HOST_TAG="linux-x86_64"
case "$(uname -s)" in
    Darwin)  HOST_TAG="darwin-x86_64" ;;
    Linux)   HOST_TAG="linux-x86_64" ;;
    MINGW*|MSYS*|CYGWIN*) HOST_TAG="windows-x86_64" ;;
esac

TOOLCHAIN="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG"
API_LEVEL=26

case "$TARGET" in
    aarch64-linux-android) CLANG_PREFIX="aarch64-linux-android${API_LEVEL}" ;;
    x86_64-linux-android)  CLANG_PREFIX="x86_64-linux-android${API_LEVEL}" ;;
    *) echo "ERROR: 不支持的目标: $TARGET"; exit 1 ;;
esac

export "CC_${TARGET//-/_}=${CLANG_PREFIX}-clang"
export "CXX_${TARGET//-/_}=${CLANG_PREFIX}-clang++"
export "AR_${TARGET//-/_}=llvm-ar"
export "CARGO_TARGET_${TARGET//-/_}_LINKER=$TOOLCHAIN/bin/${CLANG_PREFIX}-clang"
export PATH="$TOOLCHAIN/bin:$PATH"

echo "构建 Android 目标:"
echo "  Example:  $EXAMPLE"
echo "  Target:   $TARGET"
echo "  NDK:      $ANDROID_NDK  (API $API_LEVEL)"

cd "$WS_ROOT"
if command -v cargo-ndk >/dev/null 2>&1; then
    cargo ndk --target "$TARGET" --api-level "$API_LEVEL" build --example "$EXAMPLE" --release
else
    cargo build --target "$TARGET" --example "$EXAMPLE" --release
fi

OUTPUT="target/$TARGET/release/examples/$EXAMPLE"
if [ -f "$OUTPUT" ]; then
    echo ""
    echo "构建成功！产物: $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
else
    echo "ERROR: 构建产物未找到: $OUTPUT"
    exit 1
fi
