//! Radio navigation: VORs and NDBs from OurAirports' navaids (public domain;
//! `assets/navaids.tsv.gz`, rebuilt by `assets/mknav.py`), and an ILS on each
//! paved runway end of 1500 m and more at the medium and large airports, made
//! from the runway itself (localizer at the far end, a 3° glide slope down to
//! the aiming point the airport traffic flies to, see `traffic::aim_dist`). What the needles show is worked out the way the receivers
//! do: the radial from a VOR against the OBS, the angle off a localizer and
//! above a glide slope antenna, the bearing to an NDB.
//!
//! The world has no magnetic variation (see gauges.rs), so radials and courses
//! are true.

use crate::{airports, geo};
use glam::DVec3;
use std::io::Read;
use std::sync::OnceLock;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    Vor,
    Ndb,
    Ils,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub kind: Kind,
    /// "BUB"; for an ILS the runway ("25L")
    pub ident: String,
    /// "Brussels"; for an ILS the airport ("EBBR")
    pub name: String,
    /// kHz (a VOR's 114.60 MHz is 114600); 0 for the runways' ILS, which are tuned by name
    pub freq_khz: u32,
    /// the antenna (a localizer's at the far end of the runway), degrees, metres
    pub lat: f64,
    pub lon: f64,
    pub elev: f64,
    pub dme: bool,
    /// ILS: the approach course (degrees true), the runway threshold, and
    /// how far past it the glide slope comes down (m)
    pub course: f64,
    pub thr: (f64, f64),
    pub aim: f64,
}

impl Station {
    pub fn pos(&self) -> DVec3 {
        geo::lla_to_pos(self.lat.to_radians(), self.lon.to_radians(), self.elev)
    }

    /// "114.60", "377", or "ILS" for the runways'.
    pub fn freq_text(&self) -> String {
        match self.kind {
            Kind::Vor => format!("{:.2}", self.freq_khz as f64 / 1000.0),
            Kind::Ndb => format!("{}", self.freq_khz),
            Kind::Ils => "ILS".into(),
        }
    }

    /// "BUB Brussels", "EBBR 25L".
    pub fn label(&self) -> String {
        match self.kind {
            Kind::Ils => format!("{} {}", self.name, self.ident),
            _ => format!("{} {}", self.ident, self.name),
        }
    }
}

/// The VORs and NDBs of the world.
pub fn db() -> &'static [Station] {
    static DB: OnceLock<Vec<Station>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut text = String::new();
        let _ = flate2::read::GzDecoder::new(&include_bytes!("../assets/navaids.tsv.gz")[..]).read_to_string(&mut text);
        text.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() < 9 {
                    return None;
                }
                Some(Station {
                    kind: if f[0] == "V" { Kind::Vor } else { Kind::Ndb },
                    ident: f[1].to_string(),
                    name: f[7].to_string(),
                    freq_khz: f[2].parse().ok()?,
                    lat: f[3].parse().ok()?,
                    lon: f[4].parse().ok()?,
                    elev: f[5].parse::<f64>().unwrap_or(0.0) / 3.28084,
                    dme: f[6] == "1",
                    course: 0.0,
                    thr: (0.0, 0.0),
                    aim: 0.0,
                })
            })
            .collect()
    })
}

fn dist_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let p = |q: (f64, f64)| geo::lla_to_pos(q.0.to_radians(), q.1.to_radians(), 0.0);
    (p(a) - p(b)).length()
}

/// Bearing (degrees true) from one point to another, as seen at the first.
pub fn bearing(from: DVec3, to: DVec3) -> f64 {
    let (e, n, _) = geo::enu(from);
    let d = to - from;
    d.dot(e).atan2(d.dot(n)).to_degrees().rem_euclid(360.0)
}

fn wrap(a: f64) -> f64 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// The stations of a kind within `radius` metres, nearest first, with how far.
pub fn near(lat: f64, lon: f64, radius: f64, kind: Kind) -> Vec<(Station, f64)> {
    let mut out: Vec<(Station, f64)> = if kind == Kind::Ils {
        ils_near(lat, lon, radius)
    } else {
        let dlat = radius / 111_000.0;
        let dlon = dlat / lat.to_radians().cos().max(0.05);
        db().iter()
            .filter(|s| s.kind == kind && (s.lat - lat).abs() < dlat && wrap(s.lon - lon).abs() < dlon)
            .map(|s| (s.clone(), dist_m((lat, lon), (s.lat, s.lon))))
            .filter(|(_, d)| *d < radius)
            .collect()
    };
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

/// The runways' ILS near a point: one each way down every paved runway of
/// 1500 m or more at the medium and large airports.
fn ils_near(lat: f64, lon: f64, radius: f64) -> Vec<(Station, f64)> {
    let db = airports::Db::get();
    let mut out = Vec::new();
    for (i, _) in db.near(lat, lon, radius) {
        let a = &db.apts[i as usize];
        if a.kind < 1 {
            continue;
        }
        for r in a.rwys.iter().filter(|r| r.paved && r.len >= 1500.0) {
            let (Some(p0), Some(p1)) = (r.pos[0], r.pos[1]) else { continue };
            for e in 0..2 {
                let (thr, far) = if e == 0 { (p0, p1) } else { (p1, p0) };
                let elev = a.elev as f64 / 3.28084;
                let t = geo::lla_to_pos(thr.0.to_radians(), thr.1.to_radians(), elev);
                let f = geo::lla_to_pos(far.0.to_radians(), far.1.to_radians(), elev);
                let st = Station { kind: Kind::Ils, ident: r.ident[e].clone(), name: a.ident.clone(), freq_khz: 0, lat: far.0, lon: far.1, elev, dme: true, course: bearing(t, f), thr, aim: crate::traffic::aim_dist(r.len) as f64 };
                out.push((st, dist_m((lat, lon), thr)));
            }
        }
    }
    out.retain(|s| s.1 < radius);
    out
}

/// What a NAV receiver shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavReading {
    /// in range: the needles are alive (else the OFF flag)
    pub live: bool,
    /// course needle, -1..1 full scale; + = the course is to the right (fly right)
    pub cdi: f64,
    /// VOR: TO (true) or FROM (false); None for an ILS
    pub to: Option<bool>,
    /// glide slope, -1..1 full scale; + = the slope is above (fly up); None off it
    pub gs: Option<f64>,
    /// DME: nautical miles (an ILS's to the threshold)
    pub dme: Option<f64>,
    /// the radial the aircraft is on (degrees true from the station)
    pub radial: f64,
}

/// The NAV receiver tuned to `st`, the OBS at `obs`, the aircraft at `pos`.
pub fn nav_reading(st: &Station, obs: f64, pos: DVec3) -> NavReading {
    let ant = st.pos();
    let alt_ft = geo::pos_to_lla(pos).2.max(0.0) * 3.28084;
    let dist_nm = (pos - ant).length() / 1852.0;
    let radial = bearing(ant, pos);
    match st.kind {
        Kind::Ils => {
            // Localizer: the angle off the centreline, seen from the far end
            // of the runway; ±2.5° is full scale, usable to 18 NM, 35° each side.
            let c = st.course.to_radians();
            let (e, n, u) = geo::enu(ant);
            let d = pos - ant;
            let fwd = e * c.sin() + n * c.cos();
            let right = e * c.cos() - n * c.sin();
            let before = -d.dot(fwd);
            let x = d.dot(right);
            let angle = x.atan2(before).to_degrees();
            let live = before > 0.0 && angle.abs() < 35.0 && dist_nm < 18.0;
            // Glide slope: 3° down to the aiming point; ±0.7° full scale.
            let thr = geo::lla_to_pos(st.thr.0.to_radians(), st.thr.1.to_radians(), st.elev);
            let gs_ant = thr + fwd * st.aim;
            let to_gs = gs_ant - pos;
            let horiz = (to_gs - u * to_gs.dot(u)).length();
            let h = (pos - gs_ant).dot(u);
            let elev_angle = h.atan2(horiz).to_degrees();
            let gs_live = live && horiz < 10.0 * 1852.0 && angle.abs() < 8.0 && to_gs.dot(fwd) > 0.0;
            NavReading {
                live,
                cdi: (-angle / 2.5).clamp(-1.0, 1.0),
                to: None,
                gs: gs_live.then(|| ((3.0 - elev_angle) / 0.7).clamp(-1.0, 1.0)),
                dme: Some((pos - thr).length() / 1852.0),
                radial,
            }
        }
        _ => {
            // VOR: in line of sight (to 130 NM); the radial against the OBS,
            // ±10° full scale.
            let los = 1.23 * (alt_ft.sqrt() + (st.elev.max(0.0) * 3.28084).sqrt());
            let live = st.kind == Kind::Vor && dist_nm < los.min(130.0);
            let to_bearing = (radial + 180.0).rem_euclid(360.0);
            let to = wrap(obs - to_bearing).abs() < 90.0;
            let dev = if to { wrap(to_bearing - obs) } else { wrap(obs - radial) };
            NavReading { live, cdi: (dev / 10.0).clamp(-1.0, 1.0), to: Some(to), gs: None, dme: (live && st.dme).then_some(dist_nm), radial }
        }
    }
}

/// The ADF needle: the bearing to an NDB relative to the nose (degrees,
/// clockwise), while it is in range (60 NM).
pub fn adf_bearing(st: &Station, pos: DVec3, heading: f64) -> Option<f64> {
    let ant = st.pos();
    ((pos - ant).length() < 60.0 * 1852.0).then(|| (bearing(pos, ant) - heading).rem_euclid(360.0))
}

/// The radios: what NAV 1 and the ADF are tuned to, the OBS, whether the
/// instruments are on screen, and the altimeter setting (None: kept on the
/// QNH, as a pilot sets it before take-off and on the way down).
pub struct Radios {
    pub nav: Option<Station>,
    pub obs: f64,
    pub adf: Option<Station>,
    pub show: bool,
    pub baro: Option<f64>,
}

impl Default for Radios {
    fn default() -> Self {
        Self { nav: None, obs: 0.0, adf: None, show: false, baro: None }
    }
}

impl Radios {
    /// Tunes NAV 1, setting the OBS to fly straight to a VOR (or the runway's course).
    pub fn tune_nav(&mut self, st: Station, pos: DVec3) {
        self.obs = match st.kind {
            Kind::Ils => st.course,
            _ => bearing(pos, st.pos()).round(),
        };
        self.nav = Some(st);
        self.show = true;
    }

    pub fn tune_adf(&mut self, st: Station) {
        self.adf = Some(st);
        self.show = true;
    }

    /// The altimeter setting (hPa), given the QNH.
    pub fn setting(&self, qnh: f64) -> f64 {
        self.baro.unwrap_or(qnh)
    }

    /// How far the altimeter reads above the true altitude (ft): about 27 ft
    /// for each hectopascal the setting is above the QNH.
    pub fn alt_error_ft(&self, qnh: f64) -> f64 {
        (self.setting(qnh) - qnh) * 27.3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64, alt: f64) -> DVec3 {
        geo::lla_to_pos(lat.to_radians(), lon.to_radians(), alt)
    }

    /// The point `d` metres from `p` on bearing `brg`, at height `alt`.
    fn off(p: DVec3, brg: f64, d: f64, alt: f64) -> DVec3 {
        let (e, n, _) = geo::enu(p);
        let q = p + (e * brg.to_radians().sin() + n * brg.to_radians().cos()) * d;
        let (la, lo, _) = geo::pos_to_lla(q);
        geo::lla_to_pos(la, lo, alt)
    }

    #[test]
    fn database() {
        let bub = near(50.9, 4.5, 20_000.0, Kind::Vor).into_iter().find(|s| s.0.ident == "BUB").unwrap().0;
        assert_eq!((bub.freq_text().as_str(), bub.dme), ("114.60", true));
        assert!(!near(50.9, 4.5, 60_000.0, Kind::Ndb).is_empty());
        let ils: Vec<_> = near(50.9, 4.5, 15_000.0, Kind::Ils).into_iter().filter(|s| s.0.name == "EBBR").collect();
        assert!(ils.len() >= 4, "{} ILS at EBBR", ils.len());
        let r25l = ils.iter().find(|s| s.0.ident == "25L").unwrap();
        assert!(wrap(r25l.0.course - 245.0).abs() < 5.0, "25L course {}", r25l.0.course);
    }

    #[test]
    fn vor_needle() {
        let st = Station { kind: Kind::Vor, ident: "TST".into(), name: "Test".into(), freq_khz: 113_000, lat: 45.0, lon: 5.0, elev: 0.0, dme: true, course: 0.0, thr: (0.0, 0.0), aim: 0.0 };
        let s = st.pos();
        // 20 NM west of the station at 5000 ft, OBS 090: on course, TO.
        let p = off(s, 270.0, 20.0 * 1852.0, 1524.0);
        let r = nav_reading(&st, 90.0, p);
        assert!(r.live && r.to == Some(true) && r.cdi.abs() < 0.02, "{r:?}");
        assert!((r.dme.unwrap() - 20.0).abs() < 0.2);
        // A little north of that course: the course is to the right (south).
        let r = nav_reading(&st, 90.0, off(s, 275.0, 20.0 * 1852.0, 1524.0));
        assert!(r.cdi > 0.4 && r.to == Some(true), "{r:?}");
        // East of it on the 090 radial outbound: FROM, centred; south of it, fly left.
        let r = nav_reading(&st, 90.0, off(s, 90.0, 20.0 * 1852.0, 1524.0));
        assert!(r.to == Some(false) && r.cdi.abs() < 0.02, "{r:?}");
        let r = nav_reading(&st, 90.0, off(s, 95.0, 20.0 * 1852.0, 1524.0));
        assert!(r.cdi < -0.4, "{r:?}");
        // Out of line of sight, low and far: dead.
        assert!(!nav_reading(&st, 90.0, off(s, 270.0, 120.0 * 1852.0, 100.0)).live);
    }

    #[test]
    fn ils_needles() {
        // A runway pointing east: threshold at (45, 5), 3 km long.
        let thr = at(45.0, 5.0, 100.0);
        let far = off(thr, 90.0, 3000.0, 100.0);
        let (fl, fo, _) = geo::pos_to_lla(far);
        let st = Station { kind: Kind::Ils, ident: "09".into(), name: "TEST".into(), freq_khz: 0, lat: fl.to_degrees(), lon: fo.to_degrees(), elev: 100.0, dme: true, course: 90.0, thr: (45.0, 5.0), aim: 300.0 };
        let gs = off(thr, 90.0, 300.0, 100.0);
        // 5 NM out on the centreline, on the 3° slope: both centred (within
        // the 0.06° the Earth's curve puts between a slope over the ground
        // and the straight beam).
        let d = 5.0 * 1852.0;
        let on = off(gs, 270.0, d, 100.0 + d * 3f64.to_radians().tan());
        let r = nav_reading(&st, 0.0, on);
        assert!(r.live && r.cdi.abs() < 0.05 && r.gs.unwrap().abs() < 0.15, "{r:?}");
        // High: fly down. Left of the centreline (north): fly right.
        let r = nav_reading(&st, 0.0, off(gs, 270.0, d, 100.0 + d * 4f64.to_radians().tan()));
        assert!(r.gs.unwrap() < -0.9, "{r:?}");
        let r = nav_reading(&st, 0.0, off(gs, 272.0, d, 100.0 + d * 3f64.to_radians().tan()));
        assert!(r.cdi > 0.5, "{r:?}");
        // Behind the antenna (past the far end): dead.
        assert!(!nav_reading(&st, 0.0, off(far, 90.0, 2000.0, 400.0)).live);
    }

    #[test]
    fn adf_and_altimeter() {
        let st = Station { kind: Kind::Ndb, ident: "N".into(), name: "N".into(), freq_khz: 350, lat: 45.0, lon: 5.0, elev: 0.0, dme: false, course: 0.0, thr: (0.0, 0.0), aim: 0.0 };
        // 10 NM south heading north: the needle points ahead; heading east, 90° left.
        let p = off(st.pos(), 180.0, 10.0 * 1852.0, 1000.0);
        assert!(wrap(adf_bearing(&st, p, 0.0).unwrap()).abs() < 1.0);
        assert!(wrap(adf_bearing(&st, p, 90.0).unwrap() - 270.0).abs() < 1.0);
        let mut r = Radios::default();
        assert_eq!(r.alt_error_ft(999.0), 0.0);
        // Left on standard in a low: reads about 380 ft high.
        r.baro = Some(1013.0);
        assert!((r.alt_error_ft(999.0) - 382.2).abs() < 0.1);
    }
}
