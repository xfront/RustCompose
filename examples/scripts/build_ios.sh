#!/usr/bin/env bash
# build_ios.sh — 构建 compose 的 iOS 静态库（aarch64-apple-ios），供 Xcode 链接。
#
# compose 已合并原 compose-platform，走「宿主驱动」模型：Rust 侧导出
# `compose_ios_*` C ABI 符号，由 UIKit 宿主（CADisplayLink 驱动 Kernel::frame）调用。
# 本脚本编译 compose 为 iOS 静态库，产出 libcompose.a。
#
# 用法：
#   bash examples/scripts/build_ios.sh [debug|release]
#
# 依赖：
#   - rustup + aarch64-apple-ios 目标
#   - Xcode（最终链接 Metal/UIKit/Foundation；本脚本不调用 Xcode）

set -euo pipefail

PROFILE="${1:-release}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

TARGET="aarch64-apple-ios"
echo "==> 添加 iOS 目标（若未安装）"
rustup target add "$TARGET" 2>/dev/null || true

if [ "$PROFILE" = "debug" ]; then
    CARGO_PROFILE=""
    OUT_DIR="target/$TARGET/debug"
else
    CARGO_PROFILE="--release"
    OUT_DIR="target/$TARGET/release"
fi

echo "==> 构建 iOS 静态库 ($PROFILE)"
# compose 已声明 crate-type = ["rlib","cdylib","staticlib"]，
# -p compose --lib 直接产出 libcompose.a。
cargo build $CARGO_PROFILE --target "$TARGET" -p compose --lib

LIB="$OUT_DIR/libcompose.a"
if [ ! -f "$LIB" ]; then
    echo "!! 未找到 $LIB" >&2
    exit 1
fi

echo "==> 产物：$LIB"
echo "==> 导出符号（compose_ios_*）："
strings "$LIB" | grep -E '^compose_ios_' | sort -u || true

cat <<'EOF'

下一步（Xcode 集成）：
  1. 把 libcompose.a 与 crates/compose/src/platform/ios 的 Swift/头文件加入 Xcode 工程；
  2. 链接 Metal / UIKit / Foundation / CoreFoundation；
  3. Other Linker Flags 加 -all_load。
EOF
