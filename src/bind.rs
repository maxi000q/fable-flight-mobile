//! Control bindings: joysticks, rudder pedals, throttle quadrants and wheels
//! (any device gilrs can open, laid out or not), and the keyboard's keys
//! moved to suit. Kept in `<config dir>/fablegame/bindings.json`.
//!
//! - An axis is bound by moving it the way that means "more": nose up,
//!   right roll, right rudder, full throttle, brake pressed. Its sign is
//!   learnt from that, so there is nothing to invert by hand.
//! - A joystick button does what a key does (`Action::key`): the game's own
//!   key handling then takes it, held or pressed.
//! - A key moved elsewhere is translated back to the key it stands for as
//!   it comes in (`map_key`), so the rest of the game is none the wiser.

use std::collections::HashMap;
use winit::keyboard::KeyCode;

/// The flight axes a device axis can drive.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum AxisAct {
    Pitch,
    Roll,
    Yaw,
    Throttle,
    BrakeLeft,
    BrakeRight,
}

impl AxisAct {
    pub const ALL: [AxisAct; 6] = [AxisAct::Pitch, AxisAct::Roll, AxisAct::Yaw, AxisAct::Throttle, AxisAct::BrakeLeft, AxisAct::BrakeRight];

    pub fn id(self) -> &'static str {
        match self {
            AxisAct::Pitch => "pitch",
            AxisAct::Roll => "roll",
            AxisAct::Yaw => "rudder",
            AxisAct::Throttle => "throttle",
            AxisAct::BrakeLeft => "brake_left",
            AxisAct::BrakeRight => "brake_right",
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            AxisAct::Pitch => t!("Pitch", "Profondeur"),
            AxisAct::Roll => t!("Roll", "Roulis"),
            AxisAct::Yaw => t!("Rudder", "Palonnier"),
            AxisAct::Throttle => t!("Throttle", "Gaz"),
            AxisAct::BrakeLeft => t!("Left toe brake", "Frein gauche"),
            AxisAct::BrakeRight => t!("Right toe brake", "Frein droit"),
        }
    }

    /// What to do to bind it.
    pub fn how(self) -> &'static str {
        use crate::i18n::t;
        match self {
            AxisAct::Pitch => t!("Pull the stick back, nose up, all the way.", "Tirez le manche à fond vers vous (cabrer)."),
            AxisAct::Roll => t!("Push the stick all the way right.", "Poussez le manche à fond vers la droite."),
            AxisAct::Yaw => t!("Push the right rudder pedal all the way.", "Enfoncez à fond la pédale de droite."),
            AxisAct::Throttle => t!("Push the throttle all the way forward.", "Poussez la manette des gaz à fond."),
            AxisAct::BrakeLeft => t!("Press the left toe brake all the way.", "Appuyez à fond sur le frein gauche."),
            AxisAct::BrakeRight => t!("Press the right toe brake all the way.", "Appuyez à fond sur le frein droit."),
        }
    }

    /// Centred axes (stick, pedals) as against one-ended ones (throttle, brakes).
    pub fn centred(self) -> bool {
        matches!(self, AxisAct::Pitch | AxisAct::Roll | AxisAct::Yaw)
    }

    /// The dead zone a new binding starts with.
    pub fn default_dead(self) -> f32 {
        if self.centred() { 0.05 } else { 0.02 }
    }

    fn parse(s: &str) -> Option<AxisAct> {
        AxisAct::ALL.into_iter().find(|a| a.id() == s)
    }
}

/// Something a key or a joystick button can do. Each stands for the key
/// that does it by default (`key`); `keyboard` says whether the keyboard
/// may move it (Esc and Tab stay put: the menus need them).
#[derive(Copy, Clone, Debug)]
pub struct Action {
    pub id: &'static str,
    pub key: KeyCode,
    pub keyboard: bool,
    en: &'static str,
    fr: &'static str,
}

impl Action {
    pub fn label(&self) -> &'static str {
        if crate::i18n::is_fr() { self.fr } else { self.en }
    }
}

const fn act(id: &'static str, key: KeyCode, en: &'static str, fr: &'static str) -> Action {
    Action { id, key, keyboard: true, en, fr }
}

pub const ACTIONS: &[Action] = &[
    act("pitch_down", KeyCode::KeyW, "Pitch down", "Piquer"),
    act("pitch_up", KeyCode::KeyS, "Pitch up", "Cabrer"),
    act("roll_left", KeyCode::KeyA, "Roll left", "Roulis à gauche"),
    act("roll_right", KeyCode::KeyD, "Roll right", "Roulis à droite"),
    act("rudder_left", KeyCode::KeyQ, "Rudder left", "Palonnier à gauche"),
    act("rudder_right", KeyCode::KeyE, "Rudder right", "Palonnier à droite"),
    act("throttle_up", KeyCode::ShiftLeft, "Throttle up", "Plus de gaz"),
    act("throttle_down", KeyCode::ControlLeft, "Throttle down", "Moins de gaz"),
    act("trim_down", KeyCode::Numpad7, "Trim nose down", "Trim à piquer"),
    act("trim_up", KeyCode::Numpad1, "Trim nose up", "Trim à cabrer"),
    act("brakes", KeyCode::KeyB, "Wheel brakes", "Freins de roues"),
    act("parking_brake", KeyCode::Period, "Parking brake", "Frein de parc"),
    act("flaps_down", KeyCode::KeyF, "Flaps down a notch", "Volets : sortir d'un cran"),
    act("flaps_up", KeyCode::KeyV, "Flaps up a notch", "Volets : rentrer d'un cran"),
    act("gear", KeyCode::KeyL, "Landing gear", "Train d'atterrissage"),
    act("speedbrake", KeyCode::Slash, "Speedbrake", "Aérofreins"),
    act("reversers", KeyCode::KeyZ, "Thrust reversers (hold)", "Inverseurs de poussée (maintenir)"),
    act("boost", KeyCode::Space, "Boost / afterburner", "Boost / postcombustion"),
    act("nav_display", KeyCode::KeyJ, "Navigation instruments", "Instruments de navigation"),
    act("obs_left", KeyCode::BracketLeft, "OBS course left", "OBS : course à gauche"),
    act("obs_right", KeyCode::BracketRight, "OBS course right", "OBS : course à droite"),
    act("baro_qnh", KeyCode::KeyX, "Altimeter to the QNH", "Caler l'altimètre au QNH"),
    act("heading_sync", KeyCode::KeyK, "Heading indicator to compass", "Recaler le conservateur de cap"),
    act("camera", KeyCode::KeyC, "Chase / cockpit camera", "Caméra extérieure / cockpit"),
    act("hud", KeyCode::KeyH, "Hide / show the HUD", "Masquer / afficher le HUD"),
    act("goto", KeyCode::KeyG, "Go to a place", "Aller à un lieu"),
    act("respawn", KeyCode::KeyR, "Respawn in the air", "Réapparaître en vol"),
    act("pause", KeyCode::KeyP, "Pause without the menu", "Pause sans le menu"),
    act("sound", KeyCode::KeyU, "Sound on / off", "Son activé / coupé"),
    act("time_forward", KeyCode::KeyT, "Time of day forward", "Avancer l'heure"),
    act("time_back", KeyCode::KeyY, "Time of day back", "Reculer l'heure"),
    act("clouds_more", KeyCode::KeyN, "More clouds", "Plus de nuages"),
    act("clouds_less", KeyCode::KeyM, "Fewer clouds", "Moins de nuages"),
    act("taa", KeyCode::F2, "Anti-aliasing on / off", "Anticrénelage activé / désactivé"),
    act("screenshot", KeyCode::F12, "Screenshot", "Capture d'écran"),
    Action { id: "pause_menu", key: KeyCode::Escape, keyboard: false, en: "Pause menu", fr: "Menu pause" },
    Action { id: "flight_panel", key: KeyCode::Tab, keyboard: false, en: "Flight panel", fr: "Panneau de vol" },
];

fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// A device: gilrs's UUID (stable across runs and ports) and its name.
pub type Uuid = [u8; 16];

#[derive(Clone, Debug, PartialEq)]
pub struct AxisBind {
    pub dev: Uuid,
    pub name: String,
    /// the device's own axis code (evdev's on Linux)
    pub code: u32,
    /// +1 or -1: the way that means "more"
    pub sign: f32,
    /// dead zone: round the centre of a stick or pedals, at the idle end
    /// of a throttle or brake (fraction of the travel)
    pub dead: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonBind {
    pub dev: Uuid,
    pub name: String,
    pub code: u32,
}

/// What is waiting for an input to be bound to it.
#[derive(Clone, Debug, PartialEq)]
pub enum Capture {
    Axis(AxisAct),
    Button(&'static str),
    Key(&'static str),
}

#[derive(Default)]
pub struct Bindings {
    pub axes: HashMap<AxisAct, AxisBind>,
    /// action id -> button
    pub buttons: HashMap<&'static str, ButtonBind>,
    /// action id -> the key it has moved to (None: no key)
    pub keys: HashMap<&'static str, Option<KeyCode>>,
    /// incoming key -> the key it stands for (None: does nothing now)
    remap: HashMap<KeyCode, Option<KeyCode>>,
    /// the devices' axes and buttons as they are now
    axis_now: HashMap<(Uuid, u32), f32>,
    button_now: HashMap<(Uuid, u32), bool>,
    /// joystick buttons pressed and let go since last asked: (action id, down)
    edges: Vec<(&'static str, bool)>,
    pub capture: Option<Capture>,
    /// the axes when the capture began, to see which one moves
    baseline: HashMap<(Uuid, u32), f32>,
    /// devices a preset has been offered to on its own (once each)
    auto_done: std::collections::HashSet<Uuid>,
}

impl Bindings {
    // ------------------------------------------------------------ the keyboard

    /// The key an action is on now.
    pub fn key_of(&self, a: &Action) -> Option<KeyCode> {
        self.keys.get(a.id).copied().unwrap_or(Some(a.key))
    }

    /// An incoming key as the game should see it: the key it stands for,
    /// itself when it has not been touched, None when it does nothing now.
    pub fn map_key(&self, k: KeyCode) -> Option<KeyCode> {
        self.remap.get(&k).copied().unwrap_or(Some(k))
    }

    /// Moves an action to a key; any other action on that key loses it.
    pub fn set_key(&mut self, id: &'static str, k: KeyCode) {
        for a in ACTIONS.iter().filter(|a| a.keyboard && a.id != id) {
            if self.key_of(a) == Some(k) {
                self.keys.insert(a.id, None);
            }
        }
        self.keys.insert(id, Some(k));
        self.tidy();
    }

    pub fn reset_key(&mut self, id: &str) {
        self.keys.remove(id);
        self.tidy();
    }

    /// Drops what is back to its default, and works the translation out again.
    fn tidy(&mut self) {
        self.keys.retain(|id, k| action(id).is_some_and(|a| *k != Some(a.key)));
        self.remap.clear();
        for a in ACTIONS.iter().filter(|a| a.keyboard) {
            if self.keys.contains_key(a.id) {
                // Its own key no longer does it…
                self.remap.entry(a.key).or_insert(None);
            }
        }
        for a in ACTIONS.iter().filter(|a| a.keyboard) {
            if let Some(Some(k)) = self.keys.get(a.id) {
                // …the new one does.
                self.remap.insert(*k, Some(a.key));
            }
        }
    }

    // ------------------------------------------------------------ the devices

    pub fn uses(&self, dev: &Uuid) -> bool {
        self.axes.values().any(|b| b.dev == *dev) || self.buttons.values().any(|b| b.dev == *dev)
    }

    /// A device axis moved (-1..1).
    pub fn axis_event(&mut self, dev: Uuid, name: &str, code: u32, v: f32) {
        self.axis_now.insert((dev, code), v);
        if let Some(Capture::Axis(a)) = self.capture.clone() {
            let base = *self.baseline.entry((dev, code)).or_insert(v);
            let d = v - base;
            if d.abs() > 0.6 {
                self.axes.retain(|_, b| !(b.dev == dev && b.code == code));
                self.axes.insert(a, AxisBind { dev, name: name.to_string(), code, sign: d.signum(), dead: a.default_dead() });
                self.capture = None;
            }
        }
    }

    /// A device button pressed or let go.
    pub fn button_event(&mut self, dev: Uuid, name: &str, code: u32, down: bool) {
        self.button_now.insert((dev, code), down);
        if down {
            if let Some(Capture::Button(id)) = self.capture.clone() {
                self.buttons.retain(|_, b| !(b.dev == dev && b.code == code));
                self.buttons.insert(id, ButtonBind { dev, name: name.to_string(), code });
                self.capture = None;
                return;
            }
        }
        for (id, b) in &self.buttons {
            if b.dev == dev && b.code == code {
                self.edges.push((id, down));
            }
        }
    }

    pub fn start_capture(&mut self, c: Capture) {
        self.baseline = self.axis_now.clone();
        self.capture = Some(c);
    }

    pub fn take_edges(&mut self) -> Vec<(&'static str, bool)> {
        std::mem::take(&mut self.edges)
    }

    /// An axis's value now, as the action reads it: -1..1 for the stick and
    /// pedals (dead zone taken off), 0..1 for the throttle and brakes.
    pub fn axis(&self, a: AxisAct) -> Option<f64> {
        let b = self.axes.get(&a)?;
        let v = (*self.axis_now.get(&(b.dev, b.code))? * b.sign) as f64;
        // The dead zone, the rest of the travel stretched so there is no step out of it.
        let d = (b.dead as f64).clamp(0.0, 0.5);
        Some(if a.centred() {
            if v.abs() < d { 0.0 } else { v.signum() * (v.abs() - d) / (1.0 - d) }
        } else {
            let f = ((v + 1.0) * 0.5).clamp(0.0, 1.0);
            if f < d { 0.0 } else { (f - d) / (1.0 - d) }
        })
    }

    /// What a binding reads as, for the bindings page.
    pub fn describe_axis(&self, a: AxisAct) -> Option<String> {
        self.axes.get(&a).map(|b| crate::i18n::t!("{} · axis {}", "{} · axe {}", short(&b.name), axis_name(b.code)))
    }

    pub fn describe_button(&self, id: &str) -> Option<String> {
        self.buttons.get(id).map(|b| crate::i18n::t!("{} · button {}", "{} · bouton {}", short(&b.name), button_name(b.code)))
    }

    // ------------------------------------------------------------ the file

    fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("fablegame").join("bindings.json"))
    }

    pub fn load() -> Self {
        let mut b = Self::default();
        if let Some(j) = Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()) {
            b.read(&j);
        }
        b
    }

    fn read(&mut self, j: &serde_json::Value) {
        if let Some(m) = j["keys"].as_object() {
            for (id, k) in m {
                if let Some(a) = action(id).filter(|a| a.keyboard) {
                    self.keys.insert(a.id, k.as_str().and_then(key_from_name));
                }
            }
        }
        let dev = |v: &serde_json::Value| -> Option<(Uuid, String, u32)> {
            Some((uuid_from_hex(v["device"].as_str()?)?, v["name"].as_str().unwrap_or("").to_string(), v["code"].as_u64()? as u32))
        };
        for v in j["axes"].as_array().into_iter().flatten() {
            if let (Some(a), Some((dev, name, code))) = (v["action"].as_str().and_then(AxisAct::parse), dev(v)) {
                let sign = if v["sign"].as_f64().unwrap_or(1.0) < 0.0 { -1.0 } else { 1.0 };
                let dead = v["dead"].as_f64().map(|d| d.clamp(0.0, 0.5) as f32).unwrap_or(a.default_dead());
                self.axes.insert(a, AxisBind { dev, name, code, sign, dead });
            }
        }
        for v in j["buttons"].as_array().into_iter().flatten() {
            if let (Some(a), Some((dev, name, code))) = (v["action"].as_str().and_then(action), dev(v)) {
                self.buttons.insert(a.id, ButtonBind { dev, name, code });
            }
        }
        for v in j["presets_applied"].as_array().into_iter().flatten() {
            if let Some(u) = v.as_str().and_then(uuid_from_hex) {
                self.auto_done.insert(u);
            }
        }
        self.tidy();
    }

    fn json(&self) -> serde_json::Value {
        let hex = |u: &Uuid| u.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let mut keys = serde_json::Map::new();
        for (id, k) in &self.keys {
            keys.insert(id.to_string(), k.map(|k| serde_json::Value::String(format!("{k:?}"))).unwrap_or(serde_json::Value::Null));
        }
        let mut axes: Vec<_> = self.axes.iter().map(|(a, b)| serde_json::json!({ "action": a.id(), "device": hex(&b.dev), "name": b.name, "code": b.code, "sign": b.sign, "dead": b.dead })).collect();
        let mut buttons: Vec<_> = self.buttons.iter().map(|(id, b)| serde_json::json!({ "action": id, "device": hex(&b.dev), "name": b.name, "code": b.code })).collect();
        axes.sort_by_key(|v| v["action"].to_string());
        buttons.sort_by_key(|v| v["action"].to_string());
        let mut done: Vec<String> = self.auto_done.iter().map(hex).collect();
        done.sort();
        serde_json::json!({ "keys": keys, "axes": axes, "buttons": buttons, "presets_applied": done })
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.json()).unwrap_or_default();
        if let Err(e) = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &p)) {
            log::warn!("saving bindings to {}: {e}", p.display());
        }
    }

    /// A fingerprint of what is bound, to see when it needs saving.
    pub fn stamp(&self) -> String {
        self.json().to_string()
    }
}

// ------------------------------------------------------------ presets

/// The device codes as gilrs gives them: the evdev event type in the high
/// half (3 for an axis, 1 for a key), its code in the low half.
const ABS: u32 = 3 << 16;
const BTN: u32 = 1 << 16;
const ABS_X: u32 = ABS;
const ABS_Y: u32 = ABS + 1;
const ABS_Z: u32 = ABS + 2;
const ABS_RZ: u32 = ABS + 5;
const ABS_THROTTLE: u32 = ABS + 6;
const BTN_TRIGGER: u32 = BTN + 0x120;
const BTN_THUMB: u32 = BTN + 0x121;
const BTN_THUMB2: u32 = BTN + 0x122;
const BTN_TOP: u32 = BTN + 0x123;
const BTN_TOP2: u32 = BTN + 0x124;

/// A ready-made set of bindings for a kind of device.
pub struct Preset {
    en: &'static str,
    fr: &'static str,
    /// lower-case pieces of a device name it is put on by itself (none: only by hand)
    names: &'static [&'static str],
    /// (axis, code, sign, dead zone)
    axes: &'static [(AxisAct, u32, f32, f32)],
    /// (action id, button code)
    buttons: &'static [(&'static str, u32)],
}

impl Preset {
    pub fn label(&self) -> &'static str {
        if crate::i18n::is_fr() { self.fr } else { self.en }
    }
}

pub const PRESETS: &[Preset] = &[
    // Measured on the stick itself: Y pitch, X roll (reversed), the twist (Z)
    // rudder, the throttle wheel on the base on Rz.
    Preset {
        en: "Turtle Beach VelocityOne Flightstick",
        fr: "Turtle Beach VelocityOne Flightstick",
        names: &["velocityone flightstick"],
        axes: &[(AxisAct::Pitch, ABS_Y, 1.0, 0.05), (AxisAct::Roll, ABS_X, -1.0, 0.05), (AxisAct::Yaw, ABS_Z, 1.0, 0.08), (AxisAct::Throttle, ABS_RZ, 1.0, 0.02)],
        buttons: &[("brakes", BTN_TRIGGER), ("camera", BTN_THUMB), ("flaps_down", BTN_THUMB2), ("flaps_up", BTN_TOP), ("gear", BTN_TOP2)],
    },
    // The usual HID layout (Logitech Extreme 3D, Thrustmaster T.16000M and
    // the like): X roll, Y pitch, the twist on Rz, a throttle that reads
    // lowest pushed forward.
    Preset {
        en: "Standard joystick (X/Y, twist, throttle)",
        fr: "Joystick standard (X/Y, torsion, manette des gaz)",
        names: &[],
        axes: &[(AxisAct::Pitch, ABS_Y, 1.0, 0.05), (AxisAct::Roll, ABS_X, 1.0, 0.05), (AxisAct::Yaw, ABS_RZ, 1.0, 0.08), (AxisAct::Throttle, ABS_THROTTLE, -1.0, 0.02)],
        buttons: &[("brakes", BTN_TRIGGER), ("camera", BTN_THUMB), ("flaps_down", BTN_THUMB2), ("flaps_up", BTN_TOP), ("gear", BTN_TOP2)],
    },
    // Rudder pedals (Logitech/Saitek, Thrustmaster TFRP…): rudder on Rz,
    // the toe brakes on X and Y.
    Preset {
        en: "Rudder pedals (rudder, toe brakes)",
        fr: "Palonnier (direction, freins aux pieds)",
        names: &["rudder", "pedal"],
        axes: &[(AxisAct::Yaw, ABS_RZ, 1.0, 0.03), (AxisAct::BrakeLeft, ABS_X, 1.0, 0.05), (AxisAct::BrakeRight, ABS_Y, 1.0, 0.05)],
        buttons: &[],
    },
];

/// The preset that goes on a device by its name, if any.
pub fn preset_for(name: &str) -> Option<&'static Preset> {
    let n = name.to_lowercase();
    PRESETS.iter().find(|p| p.names.iter().any(|m| n.contains(m)))
}

impl Bindings {
    /// Puts a preset on a device. `over`: replace what is bound already
    /// (by hand); else only fill in what is free (on its own).
    pub fn apply_preset(&mut self, p: &Preset, dev: Uuid, name: &str, over: bool) {
        for &(a, code, sign, dead) in p.axes {
            if over || !self.axes.contains_key(&a) {
                self.axes.insert(a, AxisBind { dev, name: name.to_string(), code, sign, dead });
            }
        }
        for &(id, code) in p.buttons {
            if let Some(a) = action(id) {
                if over || !self.buttons.contains_key(a.id) {
                    self.buttons.retain(|_, b| !(b.dev == dev && b.code == code));
                    self.buttons.insert(a.id, ButtonBind { dev, name: name.to_string(), code });
                }
            }
        }
        self.auto_done.insert(dev);
    }

    /// A device seen: the first time, if it has nothing bound and a preset
    /// knows it by name, that preset goes on. Returns the preset's name.
    pub fn auto_preset(&mut self, dev: Uuid, name: &str) -> Option<&'static str> {
        if !self.auto_done.insert(dev) || self.uses(&dev) {
            return None;
        }
        let p = preset_for(name)?;
        self.apply_preset(p, dev, name, false);
        Some(p.label())
    }

    /// Takes everything off a device.
    pub fn clear_device(&mut self, dev: &Uuid) {
        self.axes.retain(|_, b| b.dev != *dev);
        self.buttons.retain(|_, b| b.dev != *dev);
    }
}

fn uuid_from_hex(hex: &str) -> Option<Uuid> {
    let mut u = [0u8; 16];
    for (i, b) in u.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(u)
}

/// An axis by its evdev name ("Y", "Rz", "Throttle"), else its number.
fn axis_name(code: u32) -> String {
    const N: [&str; 11] = ["X", "Y", "Z", "Rx", "Ry", "Rz", "Throttle", "Rudder", "Wheel", "Gas", "Brake"];
    match (code >> 16, code & 0xffff) {
        (3, c) if (c as usize) < N.len() => N[c as usize].to_string(),
        (_, c) => c.to_string(),
    }
}

/// A button by its number on a joystick (1 = trigger), or its gamepad name.
fn button_name(code: u32) -> String {
    const PAD: [&str; 15] = ["A", "B", "C", "X", "Y", "Z", "LB", "RB", "LT", "RT", "Select", "Start", "Mode", "L3", "R3"];
    match code & 0xffff {
        c @ 0x120..=0x12f => (c - 0x120 + 1).to_string(),
        c @ 0x130..=0x13e => PAD[(c - 0x130) as usize].to_string(),
        c @ 0x2c0..=0x2ff => (c - 0x2c0 + 17).to_string(),
        c => format!("0x{c:x}"),
    }
}

/// A device's name, cut short for the bindings page.
fn short(name: &str) -> String {
    let n: String = name.chars().take(40).collect();
    if n.is_empty() { "?".into() } else { n }
}

/// The keys a binding can be put on, by the name winit gives them.
const KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal,
        F2, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Home, End, PageUp, PageDown, Insert, Delete,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, Space, Backspace,
        Comma, Period, Slash, Semicolon, Quote, BracketLeft, BracketRight, Backslash, Minus, Equal, Backquote, IntlBackslash,
    ]
};

fn key_from_name(s: &str) -> Option<KeyCode> {
    KEYS.iter().copied().find(|k| format!("{k:?}") == s)
}

/// Whether a key can take a binding (not Esc, Tab, F1, F3, Enter: the menus' own).
pub fn bindable(k: KeyCode) -> bool {
    KEYS.contains(&k)
}

/// A key's name for the bindings page: "W", "Left Shift", "Num 7"…
pub fn key_name(k: KeyCode) -> String {
    let s = format!("{k:?}");
    let fr = crate::i18n::is_fr();
    if let Some(c) = s.strip_prefix("Key").or_else(|| s.strip_prefix("Digit")) {
        return c.to_string();
    }
    if let Some(c) = s.strip_prefix("Numpad") {
        return format!("{} {c}", if fr { "Pavé num." } else { "Num" });
    }
    let named = match k {
        KeyCode::ShiftLeft => if fr { "Maj gauche" } else { "Left Shift" },
        KeyCode::ShiftRight => if fr { "Maj droite" } else { "Right Shift" },
        KeyCode::ControlLeft => if fr { "Ctrl gauche" } else { "Left Ctrl" },
        KeyCode::ControlRight => if fr { "Ctrl droite" } else { "Right Ctrl" },
        KeyCode::AltLeft => "Alt",
        KeyCode::AltRight => "Alt Gr",
        KeyCode::Space => if fr { "Espace" } else { "Space" },
        KeyCode::Period => ".",
        KeyCode::Comma => ",",
        KeyCode::Slash => "/",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::ArrowUp => "↑",
        KeyCode::ArrowDown => "↓",
        KeyCode::ArrowLeft => "←",
        KeyCode::ArrowRight => "→",
        KeyCode::PageUp => if fr { "Page préc." } else { "Page Up" },
        KeyCode::PageDown => if fr { "Page suiv." } else { "Page Down" },
        KeyCode::Escape => if fr { "Échap" } else { "Esc" },
        _ => "",
    };
    if named.is_empty() { s } else { named.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_keys() {
        let mut b = Bindings::default();
        let pitch_down = action("pitch_down").unwrap();
        let pitch_up = action("pitch_up").unwrap();
        assert_eq!(b.map_key(KeyCode::KeyW), Some(KeyCode::KeyW));
        // Pitch down onto I: I now pitches down, W does nothing.
        b.set_key("pitch_down", KeyCode::KeyI);
        assert_eq!(b.map_key(KeyCode::KeyI), Some(KeyCode::KeyW));
        assert_eq!(b.map_key(KeyCode::KeyW), None);
        // Pitch down onto S takes it from pitch up, which is left without a key.
        b.set_key("pitch_down", KeyCode::KeyS);
        assert_eq!(b.map_key(KeyCode::KeyS), Some(KeyCode::KeyW));
        assert_eq!(b.key_of(pitch_up), None);
        assert_eq!(b.map_key(KeyCode::KeyI), Some(KeyCode::KeyI));
        // Back to the defaults.
        b.reset_key("pitch_down");
        b.reset_key("pitch_up");
        assert_eq!((b.key_of(pitch_down), b.key_of(pitch_up)), (Some(KeyCode::KeyW), Some(KeyCode::KeyS)));
        assert_eq!(b.map_key(KeyCode::KeyS), Some(KeyCode::KeyS));
        assert!(b.keys.is_empty());
    }

    #[test]
    fn axes_learn_their_sign() {
        let mut b = Bindings::default();
        let dev = [7u8; 16];
        b.axis_event(dev, "Stick", 1, 0.02);
        b.start_capture(Capture::Axis(AxisAct::Pitch));
        // Another axis drifting a little is not taken; this one pulled back (to -1) is.
        b.axis_event(dev, "Stick", 5, 0.3);
        b.axis_event(dev, "Stick", 1, -0.9);
        assert_eq!(b.capture, None);
        assert_eq!(b.axes[&AxisAct::Pitch].sign, -1.0);
        assert!((b.axis(AxisAct::Pitch).unwrap() - 0.896).abs() < 0.01);
        // A throttle at idle reads 0, full 1.
        b.start_capture(Capture::Axis(AxisAct::Throttle));
        b.axis_event(dev, "Stick", 2, -1.0);
        b.axis_event(dev, "Stick", 2, 1.0);
        assert_eq!(b.axis(AxisAct::Throttle), Some(1.0));
        b.axis_event(dev, "Stick", 2, -1.0);
        assert_eq!(b.axis(AxisAct::Throttle), Some(0.0));
    }

    #[test]
    fn buttons_and_file() {
        let mut b = Bindings::default();
        let dev = [0xab; 16];
        b.start_capture(Capture::Button("gear"));
        b.button_event(dev, "Throttle quadrant", 290, true);
        b.button_event(dev, "Throttle quadrant", 290, false);
        b.button_event(dev, "Throttle quadrant", 290, true);
        assert_eq!(b.take_edges(), vec![("gear", false), ("gear", true)]);
        b.set_key("gear", KeyCode::KeyJ);
        b.axes.insert(AxisAct::Yaw, AxisBind { dev, name: "Pedals".into(), code: 5, sign: -1.0, dead: 0.1 });
        let mut c = Bindings::default();
        c.read(&b.json());
        assert_eq!(c.buttons, b.buttons);
        assert_eq!(c.axes, b.axes);
        assert_eq!(c.map_key(KeyCode::KeyJ), Some(KeyCode::KeyL));
        assert!(c.uses(&dev));
        assert_eq!((axis_name(ABS_Y), axis_name(ABS_RZ), button_name(BTN_TRIGGER), button_name(BTN + 0x2c5)), ("Y".into(), "Rz".into(), "1".into(), "22".into()));
    }

    #[test]
    fn dead_zones() {
        let mut b = Bindings::default();
        let dev = [1u8; 16];
        b.axes.insert(AxisAct::Roll, AxisBind { dev, name: "S".into(), code: 0, sign: 1.0, dead: 0.2 });
        b.axes.insert(AxisAct::Throttle, AxisBind { dev, name: "S".into(), code: 6, sign: 1.0, dead: 0.1 });
        b.axis_event(dev, "S", 0, 0.15);
        b.axis_event(dev, "S", 6, -0.85);
        assert_eq!((b.axis(AxisAct::Roll), b.axis(AxisAct::Throttle)), (Some(0.0), Some(0.0)));
        b.axis_event(dev, "S", 0, 0.6);
        b.axis_event(dev, "S", 6, 1.0);
        assert!((b.axis(AxisAct::Roll).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(b.axis(AxisAct::Throttle), Some(1.0));
    }

    #[test]
    fn presets() {
        let mut b = Bindings::default();
        let stick = [2u8; 16];
        // A VelocityOne seen for the first time gets its preset, once.
        assert!(b.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_some());
        assert_eq!(b.axes[&AxisAct::Yaw].code, ABS_Z);
        assert_eq!(b.buttons["gear"].code, BTN_TOP2);
        b.clear_device(&stick);
        assert!(b.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_none());
        assert!(b.axes.is_empty());
        // Pedals fill in only what is free; an unknown device is left alone.
        let pedals = [3u8; 16];
        b.apply_preset(&PRESETS[1], stick, "Stick", true);
        assert!(b.auto_preset(pedals, "Thrustmaster T-Rudder").is_some());
        assert_eq!(b.axes[&AxisAct::Yaw].dev, stick);
        assert_eq!(b.axes[&AxisAct::BrakeLeft].dev, pedals);
        assert!(b.auto_preset([4u8; 16], "Some Gamepad").is_none());
        // And it is all remembered.
        let mut c = Bindings::default();
        c.read(&b.json());
        assert!(c.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_none());
        assert_eq!(c.axes, b.axes);
    }
}
