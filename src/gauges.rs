//! What the instruments show, as against what the aircraft is doing. Each
//! instrument has its own workings, and with them its own lag and errors:
//!
//! - the vertical speed indicator reads a leak across a calibrated orifice,
//!   so it trails the climb or descent by several seconds;
//! - the attitude indicator is a vacuum-driven gyro held level by pendulous
//!   vanes that erect it toward the apparent vertical a few degrees a minute.
//!   After a long turn it is a little out; with the engine stopped the
//!   suction goes, the rotor runs down over minutes and the horizon wanders off;
//! - the heading indicator is a free gyro: it drifts and has to be set to the
//!   compass now and then;
//! - the wet compass card hangs on a pivot in fluid: in a bank or while the
//!   speed changes it tilts and the steep dip of the Earth's field pulls it
//!   off (the turning and acceleration errors), and it swings before settling;
//! - the turn coordinator's gyro is canted, so it also shows roll rate; the
//!   ball is pushed about by the sideways force;
//! - fuel gauges slosh, temperatures take minutes to come up, pressures and
//!   the alternator follow the revs.
use crate::plane::Plane;
use glam::DVec3;
use std::f64::consts::TAU;

pub struct Gauges {
    asi: f64,
    alt: f64,
    vsi: f64,
    /// the attitude gyro's idea of "up" (planet frame), its rotor speed (0..1)
    gyro_up: DVec3,
    spin: f64,
    /// heading indicator: offset from true heading (deg), and its drift (deg/s)
    pub dg_off: f64,
    dg_drift: f64,
    /// compass card: shown heading (deg) and its swing rate
    card: f64,
    card_v: f64,
    tc: f64,
    ball: f64,
    ball_v: f64,
    fuel: [f64; 2],
    oil_t: f64,
    oil_p: f64,
    egt: f64,
    mp: f64,
    rpm: f64,
    suction: f64,
    amps: f64,
    seed: u64,
    t: f64,
}

/// Small deterministic noise (a few octaves of sines), -1..1.
fn wobble(t: f64, s: f64) -> f64 {
    ((t * 1.7 + s).sin() * 0.5 + (t * 3.1 + s * 2.3).sin() * 0.3 + (t * 7.3 + s * 5.1).sin() * 0.2).clamp(-1.0, 1.0)
}
fn lag(v: f64, to: f64, tau: f64, dt: f64) -> f64 {
    v + (to - v) * (1.0 - (-dt / tau.max(1e-3)).exp())
}
fn wrap(a: f64) -> f64 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// What the instruments are trying to show at this instant.
struct Truth {
    ias_kt: f64,
    alt_ft: f64,
    vs_fpm: f64,
    hdg: f64,
    rpm: f64,
    mp: f64,
    oil_t: f64,
    oil_p: f64,
    egt: f64,
    suction: f64,
}
fn truth(p: &Plane) -> Truth {
    let (_, _, h) = crate::geo::pos_to_lla(p.pos);
    let (e, n, u) = crate::geo::enu(p.pos);
    let f = p.forward();
    let rpm = p.prop_rate * 60.0 / TAU;
    let running = smooth(250.0, 600.0, rpm);
    let thr = p.throttle;
    let eng = (p.dmg.engine as f64).clamp(0.0, 1.0);
    // Manifold pressure: the outside air's, less what the throttle plate holds
    // back while the engine draws; stopped, it reads the outside air.
    let amb = 29.92 * crate::plane::pressure_ratio(h);
    let mp = amb - amb * (0.62 - 0.6 * thr).max(0.0) * running;
    Truth {
        ias_kt: p.ias() * 1.94384,
        alt_ft: h * 3.28084,
        vs_fpm: p.vel.dot(u) * 196.85,
        hdg: f.dot(e).atan2(f.dot(n)).to_degrees().rem_euclid(360.0),
        rpm,
        mp,
        oil_t: 120.0 + (55.0 + 25.0 * thr + 90.0 * (1.0 - eng)) * running,
        oil_p: (22.0 + 58.0 * smooth(500.0, 2300.0, rpm)) * eng.sqrt() * running,
        egt: (0.45 + 0.3 * thr) * running,
        // The engine-driven vacuum pump: about 5 in. Hg from a fast idle up.
        suction: 5.2 * smooth(250.0, 1000.0, rpm).sqrt() * (0.4 + 0.6 * eng),
    }
}
fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Gauges {
    /// Everything reading true and settled (a fresh start).
    pub fn new(p: &Plane, seed: u64) -> Gauges {
        let tr = truth(p);
        let up = p.pos.normalize();
        let s = (seed % 1000) as f64 * 0.001;
        Gauges {
            asi: tr.ias_kt,
            alt: tr.alt_ft,
            vsi: tr.vs_fpm,
            gyro_up: up,
            spin: if tr.rpm > 300.0 { 1.0 } else { 0.0 },
            dg_off: 0.0,
            // A gyro that wanders up to about 3 degrees in 15 minutes.
            dg_drift: (s - 0.5) * 2.0 * 3.0 / 900.0,
            card: tr.hdg,
            card_v: 0.0,
            tc: 0.0,
            ball: 0.0,
            ball_v: 0.0,
            fuel: [21.0, 19.5],
            oil_t: tr.oil_t,
            oil_p: tr.oil_p,
            egt: tr.egt,
            mp: tr.mp,
            rpm: tr.rpm,
            suction: tr.suction,
            amps: 0.0,
            seed: seed | 1,
            t: 0.0,
        }
    }

    /// Set the heading indicator to the compass (what a pilot does every so often).
    pub fn sync_dg(&mut self, p: &Plane) {
        self.dg_off = wrap(self.card - truth(p).hdg);
    }

    /// Advance by `dt` of game time; `time` is the flight time (s) for the fuel burnt.
    pub fn update(&mut self, dt: f64, p: &Plane, time: f64) {
        if dt <= 0.0 {
            return;
        }
        self.t += dt;
        let t = self.t;
        let tr = truth(p);
        let (lat, _, _) = crate::geo::pos_to_lla(p.pos);
        let (e, n, u) = crate::geo::enu(p.pos);
        let sf_w = p.rot * p.sf;
        let g = sf_w.length().max(0.5);

        // Pitot-static: the airspeed needle and altimeter a touch behind, the
        // VSI well behind.
        self.asi = lag(self.asi, tr.ias_kt, 0.25, dt);
        self.alt = lag(self.alt, tr.alt_ft, 0.35, dt);
        self.vsi = lag(self.vsi, tr.vs_fpm, 2.6, dt);

        // Vacuum system and the attitude gyro.
        self.suction = lag(self.suction, tr.suction, 0.8, dt);
        let want_spin = (self.suction / 3.8).clamp(0.0, 1.0);
        self.spin = lag(self.spin, want_spin, if want_spin > self.spin { 40.0 } else { 200.0 }, dt);
        // The erecting vanes pull the gyro toward the apparent vertical (about
        // 3 degrees a minute at full speed); they only work with air through them.
        let appar = sf_w / g;
        let erect = 3f64.to_radians() / 60.0 * smooth(0.5, 0.9, self.spin) * dt;
        let err = self.gyro_up.cross(appar);
        if err.length() > 1e-9 {
            let a = err.length().asin().min(erect);
            self.gyro_up = glam::DQuat::from_axis_angle(err.normalize(), a) * self.gyro_up;
        }
        // A slow rotor is no longer rigid: it precesses away, faster the slower it turns.
        let loose = (1.0 - smooth(0.25, 0.75, self.spin)) * 0.25 * dt;
        if loose > 0.0 {
            let s = self.seed as f64 * 1e-3;
            let wob = e * wobble(t * 0.13, s) + n * wobble(t * 0.11, s + 4.0);
            self.gyro_up = (self.gyro_up + wob * loose).normalize();
        }
        // Keep it near the local vertical as the planet curves under it.
        let drift = self.gyro_up.dot(u).clamp(-1.0, 1.0);
        if drift > 0.9999995 {
            self.gyro_up = u;
        }

        // Heading indicator: a free gyro wandering off, worse as the suction fails.
        self.dg_off = wrap(self.dg_off + (self.dg_drift + (1.0 - self.spin) * 0.15 * wobble(t * 0.05, 7.0)) * dt);

        // The compass card hangs square to the apparent gravity and points along
        // the field's component in its own plane; it reads true here (no
        // variation in this world) with the dip of a dipole field.
        let dip = (2.0 * lat.tan()).atan();
        let b = n * dip.cos() - u * dip.sin();
        let cn = appar;
        let bp = (b - cn * b.dot(cn)).normalize_or_zero();
        let f = p.forward();
        let fp = f - cn * f.dot(cn);
        if bp != DVec3::ZERO && fp.length() > 1e-3 {
            let ee = bp.cross(cn);
            let reading = fp.dot(ee).atan2(fp.dot(bp)).to_degrees();
            // A damped pendulum in fluid: it swings and overshoots a little.
            let (w0, zeta) = (2.2, 0.32);
            self.card_v += (w0 * w0 * wrap(reading - self.card) - 2.0 * zeta * w0 * self.card_v) * dt;
            self.card = (self.card + self.card_v * dt).rem_euclid(360.0);
        }

        // Turn coordinator: a gyro canted 30 degrees, so it feels the roll rate
        // as well; electric, so it keeps going without suction.
        let yaw_r = -p.omega.y;
        let roll_r = -p.omega.z;
        let rate = (yaw_r * 0.866 + roll_r * 0.5).to_degrees() / 0.866;
        self.tc = lag(self.tc, rate, 0.12, dt);
        // The ball rolls to where the apparent gravity is, damped by its fluid.
        let ball_to = (-p.sf.x).atan2(p.sf.y.max(0.5)) / 0.14;
        let (w0, zeta) = (9.0, 0.65);
        self.ball_v += (w0 * w0 * (ball_to - self.ball) - 2.0 * zeta * w0 * self.ball_v) * dt;
        self.ball = (self.ball + self.ball_v * dt).clamp(-1.15, 1.15);
        if self.ball.abs() >= 1.15 {
            self.ball_v = 0.0;
        }

        // Fuel: what is left, sloshing about under the sideways and vertical loads.
        let side = -p.sf.x / 9.81;
        let vert = p.sf.y / 9.81 - 1.0;
        let left = (21.0 - time / 900.0).max(1.0);
        let right = (19.5 - time / 950.0).max(1.0);
        let slosh = |s: f64, k: f64| s * (1.0 + 0.22 * side * k - 0.08 * vert) + 0.25 * wobble(t * 0.9, k * 3.0) * p.air.sigma_w.min(3.0) / 3.0;
        self.fuel[0] = lag(self.fuel[0], slosh(left, -1.0), 1.4, dt);
        self.fuel[1] = lag(self.fuel[1], slosh(right, 1.0), 1.4, dt);

        // Engine: oil takes minutes, EGT seconds; pressure and revs follow at
        // once (the tach needle quivers a little).
        self.oil_t = lag(self.oil_t, tr.oil_t, 140.0, dt);
        self.oil_p = lag(self.oil_p, tr.oil_p, 0.6, dt);
        self.egt = lag(self.egt, tr.egt, 3.0, dt);
        self.mp = lag(self.mp, tr.mp, 0.25, dt);
        self.rpm = lag(self.rpm, tr.rpm, 0.12, dt);
        // The alternator charges once the revs are up; below that the battery carries the load.
        let charge = if tr.rpm > 900.0 { 2.0 + 6.0 * (-t / 90.0).exp() } else if tr.rpm > 200.0 { -3.0 } else { -6.0 };
        self.amps = lag(self.amps, charge, 1.5, dt);
    }

    /// The instrument readings as plane.wgsl takes them (rows 0..4, and the
    /// compass card for row 5).
    pub fn readings(&self, p: &Plane) -> ([[f32; 4]; 5], f32) {
        let tr = truth(p);
        let f = p.forward();
        let r = p.right();
        let up = p.up();
        let gu = self.gyro_up;
        let pitch = f.dot(gu).clamp(-1.0, 1.0).asin().to_degrees();
        let bank = (-r.dot(gu)).atan2(up.dot(gu)).to_degrees();
        let jitter = wobble(self.t * 23.0, 1.3) * 6.0 * smooth(300.0, 800.0, self.rpm);
        (
            [
                [self.asi as f32, self.alt as f32, self.vsi as f32, (tr.hdg + self.dg_off).rem_euclid(360.0) as f32],
                [pitch as f32, bank as f32, self.tc as f32, self.ball.clamp(-1.0, 1.0) as f32],
                [(self.rpm + jitter).max(0.0) as f32, self.mp as f32, self.fuel[0].max(0.0) as f32, self.fuel[1].max(0.0) as f32],
                [self.oil_t as f32, self.oil_p as f32, self.egt as f32, self.amps as f32],
                [self.suction as f32, 0.0, 0.0, 0.0],
            ],
            self.card as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(hdg: f64) -> Plane {
        Plane::spawn(&crate::aircraft::c172::SPEC, 37.8f64.to_radians(), -122.4f64.to_radians(), 1000.0, hdg.to_radians(), 50.0)
    }

    #[test]
    fn compass_reads_heading_when_level() {
        let mut p = level(90.0);
        // Straight and level: the accelerometer feels just the 1 g holding it up.
        p.sf = p.rot.inverse() * p.pos.normalize() * 9.81;
        let mut g = Gauges::new(&p, 3);
        g.card = 80.0;
        for _ in 0..600 {
            g.update(0.01, &p, 0.0);
        }
        assert!(wrap(g.card as f64 - 90.0).abs() < 1.0, "card {}", g.card);
    }

    #[test]
    fn vsi_lags_a_climb() {
        let mut p = level(0.0);
        let mut g = Gauges::new(&p, 3);
        let u = p.pos.normalize();
        p.vel += u * 5.0; // ~1000 fpm
        g.update(0.5, &p, 0.0);
        let after_half = g.vsi;
        for _ in 0..200 {
            g.update(0.05, &p, 0.0);
        }
        assert!(after_half < g.vsi * 0.4, "VSI jumped to {after_half} of {}", g.vsi);
        assert!((g.vsi - 984.0).abs() < 60.0);
    }
}
