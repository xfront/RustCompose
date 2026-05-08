#!/usr/bin/env bash
# build_ohos.sh — 构建 compose（已合并原 compose-platform）示例的 HarmonyOS Next 目标
#                 (aarch64-unknown-linux-ohos, Tier 3，需 -Z build-std)。
#
# 用法：
#   export OHOS_NDK_HOME=/path/to/ohos-sdk/native
#   ./scripts/build_ohos.sh [example_name]
#   ./scripts/build_ohos.sh gallery
#
# 依赖：OHOS NDK（含 llvm 工具链）+ rustup target add aarch64-unknown-linux-ohos

set -e

EXAMPLE=${1:-gallery}
OHOS_NDK=${OHOS_NDK_HOME:?"OHOS_NDK_HOME not set. Please set it to the OHOS NDK path (e.g., /path/to/ohos-sdk/native)"}

TARGET="aarch64-unknown-linux-ohos"
OHOS_LLVM_BIN="${OHOS_NDK}/llvm/bin"

if [ ! -d "$OHOS_LLVM_BIN" ]; then
    echo "Error: OHOS LLVM toolchain not found at $OHOS_LLVM_BIN"
    exit 1
fi

if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "==> 添加 Tier 3 目标: $TARGET"
    rustup target add "$TARGET"
fi

echo "=== Building HarmonyOS Next example: $EXAMPLE ==="
echo "Target: $TARGET"
echo "NDK: $OHOS_NDK"

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_OHOS_LINKER="${OHOS_LLVM_BIN}/clang"
export CC_aarch64_unknown_linux_ohos="${OHOS_LLVM_BIN}/clang"
export CXX_aarch64_unknown_linux_ohos="${OHOS_LLVM_BIN}/clang++"
export AR_aarch64_unknown_linux_ohos="${OHOS_LLVM_BIN}/llvm-ar"
export RANLIB_aarch64_unknown_linux_ohos="${OHOS_LLVM_BIN}/llvm-ranlib"

# Tier 3 target 需要 -Z build-std
cargo build --release \
    --target "$TARGET" \
    -Z build-std=std,panic_abort \
    --example "$EXAMPLE"

OUTPUT="target/$TARGET/release/examples/$EXAMPLE"
echo ""
echo "=== Build complete ==="
echo "Binary: $OUTPUT"
echo ""
echo "To deploy to a HarmonyOS device:"
echo "  1. Package as an HAP (HarmonyOS Ability Package)"
echo "  2. Use DevEco Studio or hdc command to install"
echo "     hdc install your_app.hap"
