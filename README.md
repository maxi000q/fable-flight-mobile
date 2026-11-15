
<div align="center">

# FableFlight

### An Open-World Flight Simulator Built in Pure Rust

Explore the skies with real-world weather, live air traffic, and an immersive open-world flying experience.

*Created with Claude Fable 5.2 & Opus 5.5*

<br>

[![Discord](https://img.shields.io/badge/Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/9akYzMRBc6)
[![Rust](https://img.shields.io/badge/Rust-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
<br>
</div>
<div align="center">
  <video
    src="https://github.com/user-attachments/assets/4f8833da-13c0-4d20-a50b-c0acf2d88df1"
    width="800"
    controls>
  </video>
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

# Joysticks, pedals and key bindings
Any joystick, throttle, rudder pedals or wheel can be used: in Bindings (pause menu, or Settings on the start screen), bind each axis by moving it the way that means "more" (nose up, right roll, right rudder, full throttle, brake pressed) and each button to any action. Each axis has an Invert switch and a dead zone. Presets set a device up in one click; the Turtle Beach VelocityOne Flightstick and rudder pedals get theirs on their own the first time they are plugged in. Keyboard keys can be moved too. Everything is kept in `bindings.json` in the config folder.

# Fuel and engine start
The engines burn fuel as they work (the Cessna some 30 l/h at 75% power, the 747 about 10 t/h in the cruise) and the fuel weighs: full tanks fly heavier than empty ones. Choose how much to take on the start screen (Flight); the HUD, the flight panel (fuel, flow, endurance) and the cockpit gauges show what is left, with LOW FUEL / NO FUEL warnings, and a dry engine stops. Start on a runway with the engine off ("cold start", or `--cold`) and start it by hand (I magnetos, Page Up mixture, hold O for the starter; on the jets I is the fuel controls and O engages the starters) or with F4, which also shuts it down. A windmilling engine with fuel and ignition catches again in the air; shut down on the ground, compression stops it within seconds.

# Mass and balance
The fuel and the payload (crew, passengers, baggage: set on the start screen) make the aircraft heavier or lighter and move its centre of gravity, shown against its limits in % of the mean aerodynamic chord (start screen, flight panel; CG LIMIT on the HUD). Loaded aft, the nose is lighter and quicker to answer and a gust takes longer to settle; forward, it is stable but nose-heavy and the elevator runs out sooner (a harder flare). A Cessna with four aboard and full baggage is out of its aft limit, as the real one is.

# Electrics and failures
A battery behind the master switch (Ins) and an alternator (the jets' generators) that charges it once the engine turns fast enough: the starter, the radios, the electric gauges and the lights need power, the engine itself runs on its magnetos. The pause menu's Failures page sets failures by hand or at random (about every N minutes): engine, alternator, flat battery, vacuum pump (the attitude and heading gyros run down), blocked pitot (the airspeed indicator turns into an altimeter) or static port (altimeter and VSI freeze), fuel leak. The flight panel shows the volts, battery and current, and the HUD warns LOW VOLTS and VACUUM.

# Radio navigation
VORs and NDBs from [OurAirports](https://ourairports.com/data/) (public domain), and an ILS on every paved runway end of 1500 m or more at medium and large airports. Tune NAV 1 (VOR or ILS) and the ADF on the flight panel's Radio page (Tab); J shows the instruments: a course deviation indicator with TO/FROM, the glide slope on an ILS and DME, and an ADF needle. [ and ] turn the OBS. Starting on a final approach tunes the runway's ILS. The altimeter is set to the QNH (from the METAR when the weather is real) unless you set it yourself; X sets it back. Courses are true: the world has no magnetic variation. `assets/mknav.py` rebuilds `assets/navaids.tsv.gz` from OurAirports' `navaids.csv`.

# Comparing with a JSBSim flight model
`cargo run --release -- --jsbsim-compare path/to/c172p.xml [--aircraft 747]` reads a JSBSim aircraft file (the XML flight models JSBSim and FlightGear use; none ship with the game) and prints how its aerodynamics compare with one of ours: geometry, mass and inertia, the lift and drag curves clean and with flaps, stall, best glide, the roll rate at full aileron, and the `Spec` values that would match its curves, as a start for tuning by hand.

# Live air traffic
Real aircraft around you, as reported by ADS-B receivers (data from [adsb.lol](https://adsb.lol), ODbL), fly in the sky with tags (callsign, type, altitude, speed, distance); the flight panel (Tab) lists the nearest. It can be switched off in the pause menu's Settings.

# Real weather
The weather follows the real conditions where you fly: the nearest METAR within 60 km ([aviationweather.gov](https://aviationweather.gov)) sets the wind, gusts, visibility, cloud layers and thunderstorms; with no airport near, [Open-Meteo](https://open-meteo.com)'s model stands in. It is looked up again every 10 minutes or 30 km, and the sky eases into each report. Switch it off in the weather settings (start screen, flight panel or F3) to set the weather by hand.

The clock can follow real time too ("Real time" above the day sliders): the sun and the date are those where you fly, now (solar time, so the local hour can differ from the legal one by up to an hour or so). Pressing T / Y to set the time by hand leaves real time.

Settings (language, sound, camera, HUD, anti-aliasing) are kept in `settings.json` in the config folder (`~/.config/fablegame` on Linux).

# Contributions
Please contribute anything if you want to! Improvements to flight sim realism, graphics, optimization, UI, and especially plane models are needed! (((=
