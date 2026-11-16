#!/usr/bin/env bash
# Assemble and sign Fable Flight Mobile's APK from this folder.
# Needs: the Rust library built for Android (see the README), the Android
# SDK's build-tools (aapt2, zipalign, apksigner) and platform android-35.
set -euo pipefail

# Adjust these two if your SDK lives elsewhere.
BT=${BT:-/home/z/android-sdk/build-tools/35.0.0}
PLATFORM=${PLATFORM:-/home/z/android-sdk/platforms/android-35/android.jar}

cd "$(dirname "$0")"
OUT=${OUT:-.}

"$BT/aapt2" compile --dir res -o resources.zip
"$BT/aapt2" link -o base.apk \
    -I "$PLATFORM" \
    --manifest AndroidManifest.xml \
    --min-sdk-version 26 --target-sdk-version 35 \
    --version-code 1 --version-name "1.0.0" \
    -R resources.zip \
    --auto-add-overlay

# The native library goes under lib/<abi>/ (NativeActivity loads
# libfablegame.so from there).
mkdir -p staged/lib/arm64-v8a
cp jniLibs/arm64-v8a/libfablegame.so staged/lib/arm64-v8a/
(cd staged && zip -q ../base.apk lib/arm64-v8a/libfablegame.so)

"$BT/zipalign" -f 4 base.apk aligned.apk

# Sign (a debug-style self-signed keystore; make your own for releases):
#   keytool -genkeypair -keystore fableflight.keystore -alias fableflight \
#     -keyalg RSA -keysize 2048 -validity 10000
"$BT/apksigner" sign --ks fableflight.keystore \
    --ks-pass pass:fableflight2026 --key-pass pass:fableflight2026 \
    --ks-key-alias fableflight --out "$OUT/FableFlightMobile.apk" aligned.apk

"$BT/apksigner" verify "$OUT/FableFlightMobile.apk"
echo "APK built: $OUT/FableFlightMobile.apk"
