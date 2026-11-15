
<div align="center">

# FableFlight

### An Open-World Flight Simulator Built in Pure Rust

Explore the skies with real-world weather, live air traffic, and an immersive open-world flying experience.

*Created with Claude Fable 5.2 & Opus 5.5*

<br>

[![Discord](https://img.shields.io/badge/Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/9akYzMRBc6)
[![Rust](https://img.shields.io/badge/Rust-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
<br>
[Demo](https://github.com/user-attachments/assets/4f8833da-13c0-4d20-a50b-c0acf2d88df1)
</div>

# Compiling
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

# Live air traffic
Real aircraft around you, as reported by ADS-B receivers (data from [adsb.lol](https://adsb.lol), ODbL), fly in the sky with tags (callsign, type, altitude, speed, distance); the flight panel (Tab) lists the nearest. It can be switched off in the pause menu's Settings.

# Real weather
The weather follows the real conditions where you fly: the nearest METAR within 60 km ([aviationweather.gov](https://aviationweather.gov)) sets the wind, gusts, visibility, cloud layers and thunderstorms; with no airport near, [Open-Meteo](https://open-meteo.com)'s model stands in. It is looked up again every 10 minutes or 30 km, and the sky eases into each report. Switch it off in the weather settings (start screen, flight panel or F3) to set the weather by hand.

The clock can follow real time too ("Real time" above the day sliders): the sun and the date are those where you fly, now (solar time, so the local hour can differ from the legal one by up to an hour or so). Pressing T / Y to set the time by hand leaves real time.

Settings (language, sound, camera, HUD, anti-aliasing) are kept in `settings.json` in the config folder (`~/.config/fablegame` on Linux).

# Contributions
Please contribute anything if you want to! Improvements to flight sim realism, graphics, optimization, UI, and especially plane models are needed! (((=
