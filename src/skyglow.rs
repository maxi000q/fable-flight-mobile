//! City light in the air at night: the light of the towns around (NASA Black
//! Marble, `sources::NightLights`) as a map of the light rising off the ground,
//! blurred more for each layer of air higher up. The haze and the clouds
//! scatter it (common.wgsl `city_up`, `glow_march`), so a city sits under a dome
//! of orange haze that shows from far away and brightens as you fly into it.

use crate::geo;
use glam::DVec3;
use std::sync::mpsc::{channel, Receiver};

/// Cells across the map.
pub const N: usize = 256;
/// Side of a cell (m): the map covers about 300 km.
pub const CELL: f64 = 1200.0;
/// How far the camera may move from the map's centre before it is redrawn (m).
const MOVE: f64 = 25_000.0;
/// Blur (Gaussian sigma, cells) of channels 1..3: the light that reaches about
/// 1.5, 4.5 and 15 km up (common.wgsl `city_up`). Channel 0 is the map, barely blurred.
const SIGMA: [f32; 3] = [1.0, 3.0, 10.0];

struct Map {
    /// centre on the sea-level sphere
    c: DVec3,
}

#[derive(Default)]
pub struct SkyGlow {
    map: Option<Map>,
    job: Option<(DVec3, Receiver<Vec<u16>>)>,
}

impl SkyGlow {
    /// Starts a new map when the camera has moved away from this one, and
    /// hands back a finished one's texels (N x N RGBA16F) to upload this frame.
    pub fn update(&mut self, cam: DVec3) -> Option<Vec<u16>> {
        let (lat, lon, _) = geo::pos_to_lla(cam);
        let here = geo::lla_to_pos(lat, lon, 0.0);
        let mut out = None;
        if let Some((c, rx)) = &self.job {
            match rx.try_recv() {
                Ok(px) => {
                    self.map = Some(Map { c: *c });
                    self.job = None;
                    out = Some(px);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(_) => {}
            }
        }
        let far = self.map.as_ref().is_none_or(|m| m.c.distance(here) > MOVE);
        if far && self.job.is_none() {
            let (tx, rx) = channel();
            let (la, lo) = (lat.to_degrees(), lon.to_degrees());
            std::thread::spawn(move || {
                let _ = tx.send(build(la, lo));
            });
            self.job = Some((here, rx));
        }
        out
    }

    /// Globals `glow_c`, `glow_e`, `glow_n`: the map's centre relative to the
    /// camera + half its side (0 = no map); its east axis + the ground's height
    /// under the camera; its north axis + how strongly the lights are on.
    pub fn globals(&self, cam: DVec3, ground: f64, on: f32) -> [[f32; 4]; 3] {
        let Some(m) = &self.map else { return [[0.0; 4]; 3] };
        let (e, n, _) = geo::enu(m.c);
        let c = m.c - cam;
        let v = |a: DVec3, w: f32| [a.x as f32, a.y as f32, a.z as f32, w];
        [v(c, (N as f64 * CELL * 0.5) as f32), v(e, ground as f32), v(n, on)]
    }
}

fn build(lat: f64, lon: f64) -> Vec<u16> {
    let raw = crate::sources::NightLights::get().grid(lat, lon, N, CELL);
    // Even the lowest air sees a few cells' worth of ground: no hard coastlines.
    let mut chans = vec![blur(&raw, 0.7)];
    for s in SIGMA {
        chans.push(blur(&raw, s));
    }
    let mut px = vec![0u16; N * N * 4];
    for i in 0..N * N {
        for (k, ch) in chans.iter().enumerate() {
            px[i * 4 + k] = f16(ch[i]);
        }
    }
    px
}

/// Separable Gaussian, zero outside.
fn blur(src: &[f32], sigma: f32) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as i64;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|v| v / ks).collect();
    let n = N as i64;
    let pass = |a: &[f32], horiz: bool| {
        let mut b = vec![0f32; N * N];
        for y in 0..n {
            for x in 0..n {
                let mut s = 0.0;
                for (j, w) in k.iter().enumerate() {
                    let o = j as i64 - r;
                    let (u, v) = if horiz { (x + o, y) } else { (x, y + o) };
                    if (0..n).contains(&u) && (0..n).contains(&v) {
                        s += a[(v * n + u) as usize] * w;
                    }
                }
                b[(y * n + x) as usize] = s;
            }
        }
        b
    };
    pass(&pass(src, true), false)
}

/// f32 to IEEE half (no NaN handling; the values here are small and positive).
fn f16(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (man | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    sign | ((exp as u16) << 10) | (((man + 0x1000) >> 13) as u16).min(0x3ff)
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_speed() {
        let t = std::time::Instant::now();
        let px = super::build(40.75, -73.99);
        let max = px.iter().step_by(4).map(|&h| h).max().unwrap();
        eprintln!("skyglow map {:.0} ms, max half {max:#x}", t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(super::f16(1.0), 0x3c00);
        assert_eq!(super::f16(0.5), 0x3800);
    }
}
