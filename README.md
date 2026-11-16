<div align="center">

# Fable Flight Mobile

### FableFlight for Android — full touch controls, tuned for phones and tablets

An open-world flight simulator built in pure Rust, now flying on Android. Explore the skies with real-world weather, live air traffic, and an immersive open-world flying experience — with your thumbs.

*A mobile port of [Rolandjg/fableflight](https://github.com/Rolandjg/fableflight), kept in sync with the desktop game.*

<br>

[![Rust](https://img.shields.io/badge/Rust-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Android](https://img.shields.io/badge/Android-3DDC84?style=for-the-badge&logo=android&logoColor=white)](https://developer.android.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](LICENSE)

</div>

## What this fork adds

**Full mobile controls** — a complete touch flight deck, drawn over the 3D view:

- A **virtual stick** that grows wherever your left thumb lands (pitch and roll), with a generous grab zone so it is always in reach
- A **rudder slider** along the bottom right, with a centre notch so it springs back to neutral
- A **throttle lever** on the left edge — set the power outright, like a real quadrant
- **Action buttons** under the right thumb: gear, flaps up/down, camera, flight panel, parking brake, pause
- **Brake and starter tabs** (hold to apply), for short-field landings and cold starts
- **Look anywhere**: a second finger on the upper screen turns the view; a **pinch zooms** the camera (or the cockpit zoom)
- Every finger is tracked separately (true multi-touch), so you can fly the stick and work the throttle at the same time
- The globe and all menus work by touch: drag to spin the planet, tap to pick a spot, double-tap to start

**Mobile optimizations** — built to run well on a phone:

- A lighter default graphics profile (Low preset, 0.8 render scale, vsync on) until you raise it in Settings
- Fullscreen immersive gameplay; landscape orientation follows the sensor
- The flight-sim's tile streaming, audio and networking all run on device — nothing is cut

**Everything else is the desktop game**: the whole world, the aircraft (Cessna 172, Boeing 747, Concorde, F-22, F/A-18, SR-71, Starship…), live traffic, real weather, fuel and failures, damage — untouched.

## Install

Grab `FableFlightMobile.apk` from the [Releases](../../releases) page and sideload it (allow "install from unknown sources" if asked). Requires Android 8.0+ (API 26) on a 64-bit (arm64-v8a) phone.

## Building from source

You need the Rust toolchain and an Android NDK:

```sh
# once: add the Android target
rustup target add aarch64-linux-android

# build the Rust library (set your NDK path; API 26+)
export ANDROID_NDK_HOME=$HOME/Android/Sdk/ndk/<version>
cargo build --release --lib --target aarch64-linux-android

# assemble and sign the APK (aapt2 + apksigner from the build-tools)
cp target/aarch64-linux-android/release/libfablegame.so android-apk/jniLibs/arm64-v8a/
cd android-apk && ./build-apk.sh
```

The `android-apk/` folder in this fork holds the manifest, the icon and the packaging script; the game itself is the same code as the desktop build (`cargo run --release` still works).

## Touch layout

```
┌──────────────────────────────────────┐
│  (drag: look around)      [II] pause │
│                          [CAM] camera│
│ THR                        [MENU]    │
│ ┬┬                          [F+]     │
│ ││   stick grows where      [F-]     │
│ ││   the thumb lands        [GEAR]   │
│ ┴┴        ◉                 [PARK]   │
│      rudder ◀●▶    [BRAKE][START]    │
└──────────────────────────────────────┘
```

# Compiling (desktop)

* Install rust
```
cargo build --release
```

running
```
cargo run --release
```

# Language

The interface is in English or French. It follows the system language, can be changed in Settings (start screen or pause menu), and is remembered. For one run: `cargo run --release -- --lang fr`.

# Credits

- The original [FableFlight](https://github.com/Rolandjg/fableflight) by Rolandjg — an open-world flight sim in pure Rust, MIT licensed
- Mobile port: touch controls, Android packaging and performance tuning

# License

MIT — same as the original. See [LICENSE](LICENSE).
