#!/bin/bash
# build_apk.sh — 把 `gallery` 示例打包为可安装、可运行的 Android APK。
#
# 走 NativeActivity 集成路线（android-activity 直驱模型）：
#   - cargo 交叉编译示例 → 重命名为 libgallery.so（导出 android_main）
#   - aapt2 link 依据 AndroidManifest.xml 生成 APK 骨架（activity = android.app.NativeActivity）
#   - 把 libgallery.so 塞进 APK，zipalign 对齐 + apksigner 用调试证书签名
#
# 前置条件：
#   - $ANDROID_HOME / $ANDROID_SDK_ROOT 指向 Android SDK（含 build-tools / platforms）
#   - $ANDROID_NDK 指向 NDK（与 build_android.sh 同）
#   - rustup target add aarch64-linux-android

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/tools/dev-cache/android-sdk}}"
[ -d "$SDK" ] || { echo "ERROR: 未找到 Android SDK ($SDK)"; exit 1; }

if [ -z "${ANDROID_NDK:-}" ]; then
    ANDROID_NDK="$(ls -1d "$SDK"/ndk/*/ 2>/dev/null | sort -V | tail -1)"
fi
ANDROID_NDK="${ANDROID_NDK%/}"
[ -d "$ANDROID_NDK" ] || { echo "ERROR: 未找到 NDK ($SDK/ndk)"; exit 1; }

BUILD_TOOLS="$(ls -1d "$SDK"/build-tools/*/ 2>/dev/null | sort -V | tail -1)"
AAPT2="${BUILD_TOOLS}aapt2"; ZIPALIGN="${BUILD_TOOLS}zipalign"; APKSIGNER="${BUILD_TOOLS}apksigner"
[ -x "$AAPT2" ] && [ -x "$ZIPALIGN" ] && [ -x "$APKSIGNER" ] || { echo "ERROR: build-tools 缺少 aapt2/zipalign/apksigner ($BUILD_TOOLS)"; exit 1; }

PLATFORM="$(ls -1d "$SDK"/platforms/android-*/ 2>/dev/null | sort -V | tail -1)"
ANDROID_JAR="${PLATFORM}android.jar"
[ -f "$ANDROID_JAR" ] || { echo "ERROR: 未找到 android.jar ($PLATFORM)"; exit 1; }

API_LEVEL="${API_LEVEL:-26}"
ABI="${ABI:-arm64-v8a}"
case "$ABI" in
    arm64-v8a)    RUST_TARGET="aarch64-linux-android"; TRIPLE="aarch64-linux-android" ;;
    x86_64)       RUST_TARGET="x86_64-linux-android";  TRIPLE="x86_64-linux-android" ;;
    *) echo "ERROR: 未知 ABI: $ABI（请用 arm64-v8a / x86_64）"; exit 1 ;;
esac

UNAME="$(uname -s)"
case "$UNAME" in
    Linux)  HOST_TAG="linux-x86_64" ;;
    Darwin) HOST_TAG="darwin-x86_64" ;;
    *)      HOST_TAG="linux-x86_64" ;;
esac
TOOLCHAIN="$ANDROID_NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
TRIPLE_US="${TRIPLE//-/_}"; RUST_TARGET_US="${RUST_TARGET//-/_}"; RUST_TARGET_UC="${RUST_TARGET_US^^}"

export ANDROID_NDK_HOME="$ANDROID_NDK"
export "CC_${TRIPLE_US}=$TOOLCHAIN/${TRIPLE}${API_LEVEL}-clang"
export "CXX_${TRIPLE_US}=$TOOLCHAIN/${TRIPLE}${API_LEVEL}-clang++"
export "AR_${TRIPLE_US}=$TOOLCHAIN/llvm-ar"
export "CARGO_TARGET_${RUST_TARGET_UC}_LINKER=$TOOLCHAIN/${TRIPLE}${API_LEVEL}-clang"
export "CARGO_TARGET_${RUST_TARGET_UC}_AR=$TOOLCHAIN/llvm-ar"
export PATH="$TOOLCHAIN:$PATH"

EXAMPLE="gallery"
LIB_NAME="gallery"
PKG_LABEL="Compose Gallery"
PKG_NAME="dev.compose.gallery"

STAGE="$WS_ROOT/target/apk-stage-$ABI"
OUT="$WS_ROOT/target"
rm -rf "$STAGE"; mkdir -p "$STAGE/lib/$ABI"

echo "==> 环境"
echo "  SDK: $SDK | NDK: $ANDROID_NDK | ABI: $ABI / $RUST_TARGET (API $API_LEVEL)"

echo "==> 交叉编译 $LIB_NAME (release, $RUST_TARGET)"
cd "$WS_ROOT"
cargo build --example "$EXAMPLE" --target "$RUST_TARGET" --release
SO_SRC="target/$RUST_TARGET/release/examples/$EXAMPLE"
[ -f "$SO_SRC" ] || { echo "ERROR: 未找到产物 $SO_SRC"; exit 1; }
# NativeActivity 加载 lib<lib_name>.so
cp "$SO_SRC" "$STAGE/lib/$ABI/lib$LIB_NAME.so"
echo "  lib$LIB_NAME.so: $(du -h "$STAGE/lib/$ABI/lib$LIB_NAME.so" | cut -f1)"

echo "==> 生成 AndroidManifest.xml"
MANIFEST="$STAGE/AndroidManifest.xml"
cat > "$MANIFEST" <<MANIFEST_EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="${PKG_NAME}"
    android:versionCode="1"
    android:versionName="1.0">
    <uses-sdk android:minSdkVersion="${API_LEVEL}" android:targetSdkVersion="34" />
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <application android:label="${PKG_LABEL}"
        android:icon="@android:drawable/sym_def_app_icon"
        android:extractNativeLibs="true"
        android:debuggable="false">
        <activity android:name="android.app.NativeActivity"
            android:label="${PKG_LABEL}"
            android:exported="true"
            android:configChanges="orientation|keyboardHidden|keyboard|screenSize|screenLayout|density|uiMode|navigation|fontScale|locale|layoutDirection">
            <meta-data android:name="android.app.lib_name" android:value="${LIB_NAME}" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
MANIFEST_EOF

echo "==> aapt2 link"
"$AAPT2" link -o "$STAGE/base.apk" --manifest "$MANIFEST" -I "$ANDROID_JAR" \
    --min-sdk-version "$API_LEVEL" --target-sdk-version 34 --version-code 1 --version-name 1.0

echo "==> 打入 lib$LIB_NAME.so"
cd "$STAGE"
zip -X -q base.apk "lib/$ABI/lib$LIB_NAME.so"
cd "$WS_ROOT"

echo "==> zipalign"
"$ZIPALIGN" -f 4 "$STAGE/base.apk" "$STAGE/aligned.apk"

KEYSTORE="${KEYSTORE:-$HOME/.android/debug.keystore}"
KS_ALIAS="${KS_ALIAS:-androiddebugkey}"
KS_PASS="${KS_PASS:-android}"
if [ ! -f "$KEYSTORE" ]; then
    echo "==> 生成调试 keystore: $KEYSTORE"
    mkdir -p "$(dirname "$KEYSTORE")"
    keytool -genkeypair -v -keystore "$KEYSTORE" -alias "$KS_ALIAS" -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass "$KS_PASS" -keypass "$KS_PASS" \
        -dname "CN=Android Debug, OU=Android, O=Android, L=NA, S=NA, C=NA" >/dev/null 2>&1
fi

FINAL_APK="$OUT/$EXAMPLE-$ABI.apk"
echo "==> apksigner sign (alias=$KS_ALIAS)"
"$APKSIGNER" sign --ks "$KEYSTORE" --ks-key-alias "$KS_ALIAS" \
    --ks-pass pass:"$KS_PASS" --key-pass pass:"$KS_PASS" --out "$FINAL_APK" "$STAGE/aligned.apk"

echo "==> 校验"
"$APKSIGNER" verify --print-certs "$FINAL_APK" | head -3 || true

echo ""
echo "构建成功！APK: $FINAL_APK ($(du -h "$FINAL_APK" | cut -f1))"
echo "安装到设备：adb install -r $FINAL_APK"
