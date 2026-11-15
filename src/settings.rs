//! Settings kept between runs, in `<config dir>/fablegame/settings.json`
//! (`~/.config/fablegame` on Linux): the language, sound, camera, HUD and
//! anti-aliasing. Command-line flags still win over them for one run.

use crate::i18n::Lang;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// None: follow the system's language
    pub lang: Option<Lang>,
    pub sound_on: bool,
    pub volume: f32,
    /// 0 chase, 1 cockpit
    pub cam_mode: u8,
    pub cockpit_fov: f32,
    pub look_sens: f64,
    pub show_hud: bool,
    pub taa: bool,
    /// real air traffic (see live.rs)
    pub live_traffic: bool,
    /// real weather (see livewx.rs)
    pub live_weather: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { lang: None, sound_on: true, volume: 1.0, cam_mode: 0, cockpit_fov: crate::cockpit::FOV, look_sens: 1.0, show_hud: true, taa: true, live_traffic: true, live_weather: true }
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("fablegame").join("settings.json"))
}

impl Settings {
    /// The saved settings; the defaults for anything missing or unreadable.
    pub fn load() -> Self {
        let Some(j) = path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()) else {
            return Self::default();
        };
        Self::from_json(&j)
    }

    fn from_json(j: &serde_json::Value) -> Self {
        let d = Self::default();
        let b = |k: &str, v: bool| j[k].as_bool().unwrap_or(v);
        let f = |k: &str, v: f64| j[k].as_f64().filter(|x| x.is_finite()).unwrap_or(v);
        Self {
            lang: j["language"].as_str().and_then(Lang::parse),
            sound_on: b("sound", d.sound_on),
            volume: f("volume", d.volume as f64).clamp(0.0, 1.0) as f32,
            cam_mode: if j["camera"].as_str() == Some("cockpit") { 1 } else { 0 },
            cockpit_fov: f("cockpit_fov", d.cockpit_fov as f64).clamp(35.0, 85.0) as f32,
            look_sens: f("mouse_look_speed", d.look_sens).clamp(0.25, 3.0),
            show_hud: b("hud", d.show_hud),
            taa: b("taa", d.taa),
            live_traffic: b("live_traffic", d.live_traffic),
            live_weather: b("live_weather", d.live_weather),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.lang.map(|l| l.code()),
            "sound": self.sound_on,
            "volume": self.volume,
            "camera": if self.cam_mode == 1 { "cockpit" } else { "chase" },
            "cockpit_fov": self.cockpit_fov,
            "mouse_look_speed": self.look_sens,
            "hud": self.show_hud,
            "taa": self.taa,
            "live_traffic": self.live_traffic,
            "live_weather": self.live_weather,
        })
    }

    /// Writes them (through a temporary file, so a crash cannot leave half a file).
    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.to_json()).unwrap_or_default();
        if let Err(e) = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &p)) {
            log::warn!("saving settings to {}: {e}", p.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings { lang: Some(Lang::Fr), sound_on: false, volume: 0.4, cam_mode: 1, cockpit_fov: 50.0, look_sens: 1.5, show_hud: false, taa: false, live_traffic: false, live_weather: false };
        assert_eq!(Settings::from_json(&s.to_json()), s);
        // Missing or odd values fall back to the defaults.
        let j = serde_json::json!({ "volume": 7.0, "language": "xx" });
        let t = Settings::from_json(&j);
        assert_eq!((t.volume, t.lang, t.taa), (1.0, None, true));
    }
}
