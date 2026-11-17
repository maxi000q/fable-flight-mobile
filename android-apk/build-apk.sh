#!/usr/bin/env bash
# Build Fable Flight Mobile's APKs from this folder — a debug one and a
# release one. Needs: the Rust library built for Android (see the README),
# the Android SDK's build-tools (aapt2, zipalign, apksigner) and platform
# android-35.
#
# The packaging details that matter for a pure-native (no Java) app:
#   * hasCode=false and extractNativeLibs=false in the manifest, otherwise
#     the installer rejects the package ("appears to be invalid") for
#     lacking a classes.dex,
#   * the .so is stored UNCOMPRESSED and 16 KB page-aligned, so the loader
#     maps it in place — required for 16 KB-page devices (Android 15+),
#   * signed with v1+v2+v3 so anything from Android 8 to 15 installs it.
set -euo pipefail

# Adjust these if your SDK lives elsewhere.
BT=${BT:-/home/z/android-sdk/build-tools/35.0.0}
PLATFORM=${PLATFORM:-/home/z/android-sdk/platforms/android-35/android.jar}

cd "$(dirname "$0")"
OUT=${OUT:-..}
mkdir -p "$OUT"

# ---------------------------------------------------------------------------
# One variant: manifest-in, signed APK out.
#   $1 debug|release   $2 package suffix ("" for release)
# ---------------------------------------------------------------------------
build_variant() {
    local variant=$1 suffix=$2
    local pkg="com.maxi.fableflightmobile${suffix}"
    local vcode vname ks kspass label
    if [ "$variant" = debug ]; then
        vcode=116; vname="1.0.16-debug"
        # Sign debug with the same committed key as release: a fresh CI
        # checkout has no debug.keystore, so generating one per build gave
        # every APK a different signature and forced an uninstall (losing
        # settings and cached tiles) on every update. One stable key lets
        # updates install straight over the previous build.
        ks=fableflight.keystore; kspass=fableflight2026
        label="Fable Flight Dbg"
    else
        vcode=116; vname="1.0.16"
        ks=fableflight.keystore; kspass=fableflight2026
        label="Fable Flight Mobile"
    fi
    local apk="FableFlightMobile-$variant.apk"

    # 1. Link the base APK with this variant's package, version and label
    #    (the debug one is labelled apart so both sit side by side on the
    #    launcher without confusing anyone).
    sed "s/android:label=\"Fable Flight Mobile\"/android:label=\"$label\"/" AndroidManifest.xml > "manifest-$variant.xml"
    "$BT/aapt2" link -o "base-$variant.apk" \
        -I "$PLATFORM" \
        --manifest "manifest-$variant.xml" \
        --rename-manifest-package "$pkg" \
        --min-sdk-version 26 --target-sdk-version 35 \
        --version-code "$vcode" --version-name "$vname" \
        -R resources.zip \
        --auto-add-overlay

    # 2. Add the native library UNCOMPRESSED and 16 KB page-aligned (the
    #    packer below does what zipalign -P 16 would; this build-tools'
    #    zipalign rejects -P). With extractNativeLibs=false the loader
    #    maps the .so straight from the APK, which 16 KB-page devices
    #    (Android 15+) require.
    python3 scripts/pack_apk.py "base-$variant.apk" \
        jniLibs/arm64-v8a/libfablegame.so "packed-$variant.apk"

    # 3. Sign.
    "$BT/apksigner" sign \
        --ks "$ks" --ks-pass "pass:$kspass" --key-pass "pass:$kspass" \
        --ks-key-alias fableflight \
        --v1-signing-enabled true --v2-signing-enabled true --v3-signing-enabled true \
        --min-sdk-version 26 \
        --out "$apk" "packed-$variant.apk"

    # 4. Verify: signature schemes and install-relevant badging.
    "$BT/apksigner" verify --verbose "$apk" | grep -E "Verifies|scheme"
    "$BT/aapt2" dump badging "$apk" | grep -E "^package|sdkVersion|native-code"
    unzip -lv "$apk" | grep "\.so" || true
    [ "$(realpath "$apk")" != "$(realpath "$OUT/$apk")" ] && mv "$apk" "$OUT/" || true
    rm -f "base-$variant.apk" "packed-$variant.apk" "manifest-$variant.xml"
}

# The icon resources, compiled once.
"$BT/aapt2" compile --dir res -o resources.zip

build_variant debug .debug
build_variant release ""
echo "Done: $OUT/FableFlightMobile-debug.apk, $OUT/FableFlightMobile-release.apk"
