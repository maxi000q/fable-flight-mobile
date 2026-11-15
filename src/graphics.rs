//! Graphics settings: how hard the renderer works, as presets (Low, Medium,
//! High, Ultra) or tuned one by one. High is what the game always drew before
//! these settings existed; Ultra goes past it (supersampling, a finer shadow
//! map, finer cloud steps, terrain and scenery further out). Saved in
//! `<config>/fablegame/graphics.json`.
//!
//! Google's photorealistic 3D tiles are not part of this: they are switched on
//! and given their own detail level in the map imagery settings (gmaps.rs).
use crate::menu::{self, DIM, TEXT};
use egui::RichText;
use std::path::PathBuf;

/// A quality step. `Off` only applies to the shadows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl Level {
    pub const PRESETS: [Level; 4] = [Level::Low, Level::Medium, Level::High, Level::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::Low => "Low",
            Level::Medium => "Medium",
            Level::High => "High",
            Level::Ultra => "Ultra",
        }
    }

    pub fn parse(s: &str) -> Option<Level> {
        [Level::Off, Level::Low, Level::Medium, Level::High, Level::Ultra].into_iter().find(|l| l.name().eq_ignore_ascii_case(s.trim()))
    }

    fn pick(self, low: f64, medium: f64, high: f64, ultra: f64) -> f64 {
        match self {
            Level::Off | Level::Low => low,
            Level::Medium => medium,
            Level::High => high,
            Level::Ultra => ultra,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Graphics {
    /// render resolution as a share of the window's (each side): below 1 is
    /// upscaled, above 1 supersampled
    pub scale: f32,
    pub shadows: Level,
    pub clouds: Level,
    pub terrain: Level,
    /// how far out buildings and trees are loaded
    pub scenery: Level,
    /// how far out buildings keep their full geometry, dressing and window depth
    pub city: Level,
    /// how far out trees are drawn, and how near they turn into impostors
    pub trees: Level,
    /// moving and parked cars (Off, or Low to High; Ultra is High)
    pub cars: Level,
    /// ambient occlusion (GTAO)
    pub ao: bool,
    // Not part of the presets: preferences more than costs.
    /// temporal anti-aliasing (F2)
    pub taa: bool,
    pub vsync: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self::preset(Level::High)
    }
}

pub const SCALE_MIN: f32 = 0.5;
pub const SCALE_MAX: f32 = 2.0;

impl Graphics {
    /// One of the presets (taa and vsync on).
    pub fn preset(l: Level) -> Self {
        let mut g = Graphics { scale: 1.0, shadows: Level::High, clouds: Level::High, terrain: Level::High, scenery: Level::High, city: Level::High, trees: Level::High, cars: Level::High, ao: true, taa: true, vsync: true };
        g.apply(l);
        g
    }

    /// Sets everything a preset covers (not taa or vsync).
    pub fn apply(&mut self, l: Level) {
        let l = if l == Level::Off { Level::Low } else { l };
        self.scale = l.pick(0.7, 0.85, 1.0, 1.5) as f32;
        self.shadows = l;
        self.clouds = l;
        self.terrain = l;
        self.scenery = l;
        self.city = l;
        self.trees = l;
        self.cars = if l == Level::Ultra { Level::High } else { l };
        self.ao = l != Level::Low;
    }

    /// The preset these settings are, if any.
    pub fn matches(&self) -> Option<Level> {
        Level::PRESETS.into_iter().find(|l| {
            let mut p = *self;
            p.apply(*l);
            p == *self
        })
    }

    /// Side of the sun's shadow map (texels; Off keeps a small unused one).
    pub fn shadow_res(&self) -> u32 {
        match self.shadows {
            Level::Off => 256,
            Level::Low => 1024,
            Level::Medium => 2048,
            Level::High => 4096,
            Level::Ultra => 6144,
        }
    }

    /// Length of the cloud raymarch's steps relative to High (clouds.wgsl).
    pub fn cloud_step(&self) -> f32 {
        self.clouds.pick(1.8, 1.35, 1.0, 0.7) as f32
    }

    /// Terrain tile split threshold (tile width / distance, world.rs `lod_k`):
    /// lower splits sooner, so finer tiles reach further.
    pub fn lod_k(&self) -> f64 {
        self.terrain.pick(0.95, 0.78, 0.62, 0.5)
    }

    /// Multiplies how far out building (and tree) tiles are loaded.
    pub fn reach(&self) -> f64 {
        self.scenery.pick(0.6, 0.8, 1.0, 1.3)
    }

    /// Multiplies the distances buildings change level of detail at (their
    /// dissolve into far stand-ins, the dressing, the window depth close up).
    pub fn city_k(&self) -> f32 {
        self.city.pick(0.45, 0.7, 1.0, 1.4) as f32
    }

    /// How far out trees are drawn (m); the last fifth dissolves away.
    pub fn tree_far(&self) -> f32 {
        self.trees.pick(1400.0, 2200.0, 3200.0, 4500.0) as f32
    }

    /// Multiplies the distance trees turn from models into impostors at.
    /// Never above 1: the leaf cards and the impostor bake are made for it.
    pub fn tree_imp_k(&self) -> f32 {
        self.trees.pick(0.5, 0.75, 1.0, 1.0) as f32
    }

    /// Share of the cars on the roads (0..1, by each car's own rank) and how
    /// far off moving and parked cars are drawn (m).
    pub fn car_density(&self) -> (f32, f64, f64) {
        match self.cars {
            Level::Off => (0.0, 0.0, 0.0),
            Level::Low => (0.35, 900.0, 450.0),
            Level::Medium => (0.65, 1500.0, 750.0),
            Level::High | Level::Ultra => (1.0, 2200.0, 1100.0),
        }
    }

    /// Whether the settings have ever been saved (else the defaults may
    /// adapt to the machine, e.g. a lower resolution on an integrated GPU).
    pub fn saved() -> bool {
        Self::path().is_some_and(|p| p.exists())
    }

    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("fablegame").join("graphics.json"))
    }

    /// The saved settings, or High.
    pub fn load() -> Self {
        let j: serde_json::Value = Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        let mut g = Self::default();
        if let Some(l) = j["preset"].as_str().and_then(Level::parse) {
            g.apply(l);
        }
        let lvl = |k: &str, d: Level| j[k].as_str().and_then(Level::parse).filter(|l| *l != Level::Off).unwrap_or(d);
        if let Some(s) = j["scale"].as_f64() {
            g.scale = (s as f32).clamp(SCALE_MIN, SCALE_MAX);
        }
        g.shadows = j["shadows"].as_str().and_then(Level::parse).unwrap_or(g.shadows);
        g.clouds = lvl("clouds", g.clouds);
        g.terrain = lvl("terrain", g.terrain);
        g.scenery = lvl("scenery", g.scenery);
        g.city = lvl("city", g.city);
        g.trees = lvl("trees", g.trees);
        g.cars = j["cars"].as_str().and_then(Level::parse).map(|l| if l == Level::Ultra { Level::High } else { l }).unwrap_or(g.cars);
        g.ao = j["ao"].as_bool().unwrap_or(g.ao);
        g.taa = j["taa"].as_bool().unwrap_or(g.taa);
        g.vsync = j["vsync"].as_bool().unwrap_or(g.vsync);
        g
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let j = serde_json::json!({
            "preset": self.matches().map(Level::name).unwrap_or("custom"),
            "scale": self.scale,
            "shadows": self.shadows.name(),
            "clouds": self.clouds.name(),
            "terrain": self.terrain.name(),
            "scenery": self.scenery.name(),
            "city": self.city.name(),
            "trees": self.trees.name(),
            "cars": self.cars.name(),
            "ao": self.ao,
            "taa": self.taa,
            "vsync": self.vsync,
        });
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&j).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// The settings panel. `window` is the window's size in pixels (for the
    /// render resolution readout), `fps` the frame rate now. Returns true when
    /// something changed (the caller applies and saves).
    pub fn ui(&mut self, ui: &mut egui::Ui, window: (u32, u32), fps: f64) -> bool {
        let before = *self;
        menu::heading(ui, "GRAPHICS");
        ui.horizontal(|ui| {
            ui.label("Preset");
            let cur = self.matches();
            for l in Level::PRESETS {
                if ui.selectable_label(cur == Some(l), l.name()).clicked() {
                    self.apply(l);
                }
            }
            if cur.is_none() {
                ui.label(RichText::new("Custom").color(menu::ACCENT));
            }
        });
        ui.label(RichText::new(match self.matches() {
            Some(Level::Low) => "Low: 70% resolution, a coarse shadow map, quicker clouds, nearer scenery, simpler cities and trees, light traffic, no ambient occlusion.",
            Some(Level::Medium) => "Medium: 85% resolution and a lighter version of everything else.",
            Some(Level::High) => "High: the full look at the window's own resolution (what the game has always drawn).",
            Some(Level::Ultra) => "Ultra: past High. Supersampled at 150%, a 6K shadow map, finer clouds, terrain and scenery further out. Wants a strong GPU.",
            _ => "Custom: tuned below.",
        }).size(11.5).color(DIM));
        ui.add_space(4.0);
        egui::Grid::new(ui.scope_id().with("gfx")).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Render resolution");
            ui.vertical(|ui| {
                let (w, h) = window;
                let (rw, rh) = ((w as f32 * self.scale).round() as u32, (h as f32 * self.scale).round() as u32);
                ui.add(egui::Slider::new(&mut self.scale, SCALE_MIN..=SCALE_MAX).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                let what = if self.scale > 1.001 { "supersampled" } else if self.scale < 0.999 { "upscaled" } else { "native" };
                ui.label(RichText::new(format!("{rw}×{rh}, {what}")).size(11.0).color(DIM));
            });
            ui.end_row();
            let levels = |ui: &mut egui::Ui, v: &mut Level, off: bool| {
                ui.horizontal(|ui| {
                    if off {
                        ui.selectable_value(v, Level::Off, "Off");
                    }
                    for l in Level::PRESETS {
                        ui.selectable_value(v, l, l.name());
                    }
                });
            };
            ui.label("Shadows");
            ui.vertical(|ui| {
                levels(ui, &mut self.shadows, true);
                if self.shadows != Level::Off {
                    let r = self.shadow_res();
                    ui.label(RichText::new(format!("{r}×{r} map")).size(11.0).color(DIM));
                }
            });
            ui.end_row();
            ui.label("Clouds");
            levels(ui, &mut self.clouds, false);
            ui.end_row();
            ui.label("Terrain detail");
            levels(ui, &mut self.terrain, false);
            ui.end_row();
            ui.label("Scenery distance");
            ui.vertical(|ui| {
                levels(ui, &mut self.scenery, false);
                ui.label(RichText::new(format!("buildings and trees out to {:.1} km", 7.5 * self.reach())).size(11.0).color(DIM));
            });
            ui.end_row();
            ui.label("City detail");
            ui.vertical(|ui| {
                levels(ui, &mut self.city, false);
                let k = self.city_k();
                ui.label(RichText::new(format!("full buildings out to {:.1} km (towers further), lit window depth to {:.0} m", 1.3 * k, 550.0 * k)).size(11.0).color(DIM));
            });
            ui.end_row();
            ui.label("Tree detail");
            ui.vertical(|ui| {
                levels(ui, &mut self.trees, false);
                ui.label(RichText::new(format!("trees out to {:.1} km, full models to {:.0} m", self.tree_far() / 1000.0, 120.0 * self.tree_imp_k())).size(11.0).color(DIM));
            });
            ui.end_row();
            ui.label("Traffic");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    for l in [Level::Off, Level::Low, Level::Medium, Level::High] {
                        ui.selectable_value(&mut self.cars, l, l.name());
                    }
                });
                let (share, far, _) = self.car_density();
                let t = if share > 0.0 { format!("{:.0}% of the cars, out to {:.1} km", share * 100.0, far / 1000.0) } else { "no cars on the roads".into() };
                ui.label(RichText::new(t).size(11.0).color(DIM));
            });
            ui.end_row();
            ui.label("Ambient occlusion");
            let t = if self.ao { "on" } else { "off" };
            ui.checkbox(&mut self.ao, t);
            ui.end_row();
            ui.label("Anti-aliasing");
            ui.checkbox(&mut self.taa, "temporal (TAA)");
            ui.end_row();
            ui.label("VSync");
            let t = if self.vsync { "on (no tearing)" } else { "off (uncapped)" };
            ui.checkbox(&mut self.vsync, t);
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.label(RichText::new(format!("Now {fps:.0} fps. Changes apply at once; scenery and terrain fill in over a few seconds.")).size(11.5).color(TEXT));
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip() {
        for l in Level::PRESETS {
            assert_eq!(Graphics::preset(l).matches(), Some(l));
        }
        let mut g = Graphics::preset(Level::High);
        g.taa = false;
        g.vsync = false;
        assert_eq!(g.matches(), Some(Level::High), "taa and vsync are not part of a preset");
        g.scale = 1.2;
        assert_eq!(g.matches(), None);
    }

    #[test]
    fn high_is_the_old_look() {
        let g = Graphics::default();
        assert_eq!((g.scale, g.shadow_res(), g.cloud_step(), g.lod_k(), g.reach(), g.ao), (1.0, 4096, 1.0, 0.62, 1.0, true));
        assert_eq!((g.city_k(), g.tree_far(), g.tree_imp_k(), g.car_density()), (1.0, 3200.0, 1.0, (1.0, 2200.0, 1100.0)));
    }
}
