//! A rocket's flight (`Engine::Rocket`): none of the aeroplane's model applies
//! to it, so it has its own step. A stack of two stages standing on its
//! engines' skirt; thrust along its axis from whichever of the lowest stage's
//! engine groups is lit, growing as the air thins (sea-level to vacuum
//! specific impulse) while the propellant, and with it the mass, the centre
//! of gravity and the inertia, drains away; steering by swivelling the
//! gimballed engines (and, in the air, the grid fins or flaps, and a little
//! cold gas), through a rate command that holds the attitude when the stick
//! is let go; the air on a blunt cylinder: drag along it, a side force at its
//! centre of pressure, enough of it flown sideways at speed to break it up.
//! `stage_sep` stages it hot, as the real one does: the booster cuts back to
//! its centre engines, the ship lights its own through the vented ring, and a
//! moment later the booster is let go (the damage model lets it fall away as
//! its own body) and the ship flies on with its own engines and propellant.
//! Coming back, the ship falls belly first on its four flaps (each a plate in
//! the airstream, folded more or less to steer it), then swings upright on
//! its engines to land on its skirt.
//!
//! The body frame is the model's (the nose at -Z). Stations are the model's
//! (z from the stack's origin); `pos` is the point `origin` along it: the
//! stack's origin until staging, the middle of the ship after. The dynamics
//! are worked about the centre of gravity, which moves along the axis as the
//! tanks empty and jumps at staging.
use super::*;
use crate::aircraft::Rocket;

/// Standard gravity (m/s²), which specific impulse is measured in.
const G0: f64 = 9.80665;
/// How long the ship's engines burn on top of the booster before it is let go (s).
const HOT: f64 = 0.8;
/// How high the ship's engines are when it swings upright to land, flown for it (m).
const FLIP: f64 = 620.0;
/// The air's push on the barrel flown sideways, and on a flap face on (drag coefficients).
const CROSS: f64 = 0.5;
const PLATE: f64 = 1.3;

impl Plane {
    /// What is still flying: its mass (kg), the station of its centre of
    /// gravity (body z), and its inertia about that, across and about its axis.
    /// Each stage is its structure (heavier toward the engines) and the column
    /// of propellant left in its tanks.
    pub(super) fn rocket_mass(&self, rk: &Rocket) -> (f64, f64, f64, f64) {
        let mut parts = [(0.0, 0.0, 0.0); 4];
        for (i, st) in rk.stages.iter().enumerate().skip(self.stage) {
            let len = st.z[1] - st.z[0];
            let prop = if i == self.stage { self.fuel.min(st.prop) } else { st.prop };
            let col = 0.8 * len * prop / st.prop;
            parts[2 * i] = (st.dry, st.z[0] + 0.6 * len, len);
            parts[2 * i + 1] = (prop, st.z[1] - 0.1 * len - 0.5 * col, col);
        }
        let m: f64 = parts.iter().map(|p| p.0).sum();
        let cg = parts.iter().map(|p| p.0 * p.1).sum::<f64>() / m;
        let r2 = rk.radius * rk.radius;
        let it = parts.iter().map(|&(m, z, l)| m * (l * l / 12.0 + r2 / 4.0 + (z - cg) * (z - cg))).sum();
        (m, cg, it, 0.5 * m * r2)
    }

    /// Standing on the pad: upright on its skirt, tanks full, engines out.
    pub(super) fn spawn_pad(spec: &'static Spec, lat: f64, lon: f64, h: f64, heading: f64) -> Self {
        let rk = spec.rocket().expect("a rocket");
        let mut p = Self::spawn(spec, lat, lon, h + rk.stages[0].z[1], heading, 0.0);
        (p.vel, p.throttle, p.charge, p.on_ground, p.hold) = (DVec3::ZERO, 0.0, 0.0, true, true);
        p
    }

    /// In the air: climbing straight up at `speed` on all engines, the belly
    /// toward `f`, so pitching down leans it that way.
    pub(super) fn rocket_air_start(&mut self, f: DVec3, u: DVec3, speed: f64) {
        let rk = self.spec.rocket().expect("a rocket");
        self.fuel = rk.stages[0].prop;
        self.rot = DQuat::from_mat3(&DMat3::from_cols(f.cross(u), u, -f)) * DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2);
        self.vel = u * speed;
        (self.throttle, self.charge, self.gear, self.gear_down) = (1.0, 1.0, 1.0, true);
        self.sf = DVec3::new(0.0, 0.0, -9.81);
    }

    /// The engines that are lit: the group's name, how many burn, how many the stage has.
    pub fn engines(&self) -> Option<(&'static str, u32, u32)> {
        let st = &self.spec.rocket()?.stages[self.stage];
        let g = st.groups[self.group(st.groups.len())];
        Some((g.0, g.1, st.n))
    }
    /// Which engine group burns: the one picked, or the last (the centre) while staging.
    fn group(&self, n: usize) -> usize {
        if self.hot > 0.0 { n - 1 } else { self.lit.min(n - 1) }
    }
    /// The ship alone on its way back, to practise bringing it down: nearly
    /// empty, falling belly first, set to fly its own landing.
    pub fn rocket_return(&mut self) {
        let Some(rk) = self.spec.rocket() else { return };
        if self.stage != 0 {
            return;
        }
        let st = &rk.stages[1];
        // (Its booster is long gone: a break far behind everything hides those pieces.)
        self.dmg.tail = Tear::new(Vec3::Z, 1.0e4, 1.0);
        self.origin = 0.5 * (st.z[0] + st.z[1]);
        self.rot *= DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2);
        (self.stage, self.lit, self.fuel, self.throttle, self.charge, self.auto) = (1, 1, 60_000.0, 0.0, 0.0, true);
        (self.vel, self.omega, self.sas) = (self.pos.normalize() * -90.0, DVec3::ZERO, DVec3::ZERO);
    }
    /// Have the ship fly its own landing, or take it back.
    pub fn toggle_autoland(&mut self) {
        if self.spec.rocket().is_some() {
            self.auto = !self.auto;
            self.note(if self.auto { "AUTOMATIC LANDING ARMED" } else { "AUTOMATIC LANDING OFF" });
        }
    }
    /// Light the stage's next group of engines (all of them, then fewer).
    pub fn next_engines(&mut self) {
        if let Some(rk) = self.spec.rocket() {
            self.lit = (self.lit + 1) % rk.stages[self.stage].groups.len();
        }
    }
    /// Stage: light the ship and drop the booster (from the next step; never standing on the ground).
    pub fn stage_sep(&mut self) {
        self.stage_cmd = self.spec.rocket().is_some() && self.stage == 0;
    }
    /// The thrust it is making (N).
    pub fn thrust_now(&self) -> f64 {
        self.spec.rocket().map_or(0.0, |rk| self.power * rk.stages[self.stage].thrust)
    }
    /// The stage that is flying, by name.
    pub fn stage_name(&self) -> Option<&'static str> {
        Some(self.spec.rocket()?.stages[self.stage].name)
    }

    /// The middle of what is flying.
    pub fn centre(&self) -> DVec3 {
        match self.spec.rocket() {
            Some(rk) => self.pos + self.rot * DVec3::new(0.0, 0.0, 0.5 * (rk.stages[1].z[0] + rk.stages[self.stage].z[1]) - self.origin),
            None => self.pos,
        }
    }
    /// Where the model's own origin is (`pos`, but for a ship that has left its booster).
    pub fn model_pos(&self) -> DVec3 {
        self.pos - self.rot * DVec3::new(0.0, 0.0, self.origin)
    }
    /// How far round it its own shadow map must reach (m).
    pub fn shadow_r(&self) -> f32 {
        self.spec.shadow_r * if self.stage > 0 { 0.45 } else { 1.0 }
    }
    /// What the plumes show (plane.wgsl `rocket_lit`): the stage burning (2:
    /// both, staging hot) and, in the fraction, how many of its engines (0.9 = all).
    pub fn plume_code(&self) -> f32 {
        let frac = self.engines().map_or(1.0, |(_, lit, n)| lit as f32 / n as f32);
        (if self.hot > 0.0 { 2.0 } else { self.stage as f32 }) + 0.9 * frac
    }
    /// How hot the air is making its windward side (0 .. 1: the tiles glowing).
    pub fn heat(&self) -> f32 {
        let (_, _, h) = geo::pos_to_lla(self.pos);
        ((density(h) / RHO0).sqrt() * (self.airspeed().length() / 7800.0).powi(3) * 150.0).min(1.0) as f32
    }
    /// The highest it will coast to as it flies now (m above the sea; None: it is not coming back).
    pub fn apogee(&self) -> Option<f64> {
        let mu = 9.81 * geo::R * geo::R;
        let en = 0.5 * self.vel.length_squared() - mu / self.pos.length();
        let h = self.pos.cross(self.vel).length();
        (en < 0.0).then(|| -mu / (2.0 * en) * (1.0 + (1.0 + 2.0 * en * h * h / (mu * mu)).max(0.0).sqrt()) - geo::R)
    }
    /// How far over the ground it is from where it left it (m).
    pub fn downrange(&self) -> f64 {
        self.pos.normalize().dot(self.launch.normalize()).clamp(-1.0, 1.0).acos() * geo::R
    }
    /// How long the propellant lasts at this rate (s).
    pub fn burnout(&self) -> Option<f64> {
        (self.fuel_flow > 1.0).then(|| self.fuel / self.fuel_flow)
    }

    /// Hot staging begins: the booster down to its centre engines, the ship's lit on top of it.
    fn stage_hot(&mut self) {
        if self.stage != 0 || self.hot > 0.0 {
            return;
        }
        if self.on_ground {
            self.note("NO STAGING ON THE GROUND");
            return;
        }
        self.hot = 1e-6;
        self.note("HOT STAGING");
    }
    /// And ends: the booster is let go and the ship is on its own.
    fn separate(&mut self, rk: &Rocket) {
        // The booster goes as one body, pushed back by the ship's engines burning on top of it.
        let mut req = ShedReq::new(Sel::Group(G_TAIL), Tear::NONE, Vec3::new(0.0, 0.0, 3.0), 0.12);
        (req.rigid, req.clean) = (true, true);
        self.shed.push(req);
        // (A break far behind everything: it only marks the booster's pieces as gone.)
        self.dmg.tail = Tear::new(Vec3::Z, 1.0e4, 1.0);
        (self.stage, self.lit, self.hot, self.fuel, self.sas) = (1, 0, 0.0, rk.stages[1].prop, DVec3::ZERO);
        // From here on `pos` is the middle of the ship.
        let st = &rk.stages[1];
        let d = DVec3::new(0.0, 0.0, 0.5 * (st.z[0] + st.z[1]) - self.origin);
        self.vel += self.rot * self.omega.cross(d);
        self.pos += self.rot * d;
        self.origin += d.z;
        self.dmg.vibe += 4.0;
        self.note("STAGE SEPARATION");
    }
    /// It is destroyed. (The wreck is made from the model, about the model's origin.)
    fn wreck(&mut self, imp: Impact, why: &'static str) -> Option<Impact> {
        (self.pos, self.origin) = (self.model_pos(), 0.0);
        self.impact(imp, why)
    }

    pub(super) fn step_rocket(&mut self, dt: f64, c: &Controls, world: &World, rk: &Rocket) -> Option<Impact> {
        self.dmg_prev = self.dmg;
        self.dmg.vibe = 0.0;
        self.throttle = match c.throttle_set {
            Some(t) => t.clamp(0.0, 1.0),
            None => (self.throttle + c.throttle * dt * 0.6).clamp(0.0, 1.0),
        };
        self.cmd += (DVec3::new(c.pitch, c.yaw, c.roll) - self.cmd) * (dt * 7.0).min(1.0);
        if std::mem::take(&mut self.stage_cmd) {
            self.stage_hot();
        }
        if self.hot > 0.0 {
            self.hot += dt;
            if self.hot > HOT {
                self.separate(rk);
            }
        }
        let st = &rk.stages[self.stage];
        let o = self.origin;
        let (lat, lon, h) = geo::pos_to_lla(self.pos);
        let upw = self.pos.normalize();
        let rho = density(h);
        let ground = world.ground(lat, lon);
        // The landing flown for it, as the real one's computers do: level on
        // its belly until the ground is close, then the three sea-level
        // engines swing it upright, pointed against whatever drift is left,
        // and burn so that it stops as its skirt touches.
        if self.auto && self.stage == 1 {
            let up = h - ground.as_ref().map_or(0.0, |g| g.h) - (st.z[1] - o);
            let vz = self.vel.dot(upw);
            if self.on_ground {
                (self.auto, self.throttle) = (false, 0.0);
                self.note("LANDED");
            } else if up < FLIP && (vz < 0.0 || self.charge > 0.02) {
                self.lit = 1;
                let aim = (upw - (self.vel - upw * vz) * 0.03).normalize();
                let turn = self.rot.inverse() * self.forward().cross(aim);
                self.cmd = DVec3::new((turn.x * 6.0).clamp(-1.0, 1.0), (-turn.y * 6.0).clamp(-1.0, 1.0), 0.0);
                let stop = (2.0 * 4.5 * (up - 1.0).max(0.0)).sqrt();
                self.throttle = if vz < -stop.max(1.5) { ((-vz - stop) * 0.08).clamp(0.03, 1.0) } else { 0.0 };
            }
        }
        let (mass, cg, it, ir) = self.rocket_mass(rk);
        let inertia = DVec3::new(it, it, ir);
        let cgv = DVec3::new(0.0, 0.0, cg - o);

        // The engines: the lit group, throttled between its floor and full
        // thrust; each makes more as the air's back-pressure falls away, for
        // the same propellant.
        let (_, n_lit, n_gim) = st.groups[self.group(st.groups.len())];
        let on = self.ignition && self.fuel > 0.0 && self.throttle > 0.02 && self.dmg.engine > 0.0;
        let want = if on { rk.throttle_min + (1.0 - rk.throttle_min) * self.throttle } else { 0.0 };
        self.charge += (want - self.charge) * (dt * if want > self.charge { 2.5 } else { 6.0 }).min(1.0);
        self.running = self.charge > 0.02;
        let f1 = st.thrust / st.n as f64;
        let vac = st.isp[1] / st.isp[0];
        let each = f1 * (vac - (vac - 1.0) * pressure_ratio(h)) * self.charge;
        let thrust = n_lit as f64 * each;
        self.fuel_flow = n_lit as f64 * f1 * self.charge / (st.isp[0] * G0);
        self.fuel = (self.fuel - self.fuel_flow * dt).max(0.0);
        self.power = thrust / st.thrust;
        self.n1 = self.charge * n_lit as f64 / st.n as f64;
        self.burner = self.charge;

        // The air on it: a blunt cylinder. Drag along the axis (most near the
        // speed of sound), a side force from any angle of attack acting at the
        // centre of pressure, and the air resisting its turning.
        let v_air = (self.vel - self.wind).length();
        self.turbulence(dt, v_air, upw);
        let vb = self.rot.inverse() * self.airspeed();
        let v = vb.length();
        let q = 0.5 * rho * v * v;
        let mach = v / sound_speed(h);
        let (front, aft) = (rk.stages[1].z[0], st.z[1]);
        let len = aft - front;
        let r = rk.radius;
        let mut force_b = DVec3::NEG_Z * thrust;
        let mut torque_b = DVec3::ZERO;
        let mut sa = 0.0;
        if v > 0.5 {
            let ca = -vb.z / v;
            let side = DVec3::new(vb.x, vb.y, 0.0);
            sa = side.length() / v;
            self.alpha = (-vb.y).atan2(-vb.z);
            let area = std::f64::consts::PI * r * r;
            let cd = 0.28 + 0.32 * smooth(0.8, 1.1, mach) * (1.0 - 0.55 * smooth(1.2, 3.0, mach));
            force_b.z += q * area * cd * ca * ca.abs();
            let n = -side.normalize_or_zero() * q * (area * 2.0 * sa * ca.abs() + 2.0 * r * len * CROSS * sa * sa);
            force_b += n;
            torque_b += DVec3::new(0.0, 0.0, front + len * st.cp - cg).cross(n);
            torque_b -= DVec3::new(self.omega.x, self.omega.y, 0.0) * (q / v.max(30.0) * r * len.powi(3) / 3.0);
        }
        // Flown sideways through thick air it folds up.
        let load = q * sa / st.q_alpha;
        self.dmg.vibe += (load * 1.5 + self.charge * 0.25 * (rho / RHO0) * n_lit as f64 / st.n as f64) as f32;
        self.dmg.fatigue = (self.dmg.fatigue + (if load > 1.0 { 2.5 * load } else { -1.0 }) as f32 * dt as f32).max(0.0);
        if self.dmg.fatigue > 1.0 {
            self.note("BROKEN UP BY THE AIRSTREAM");
            return self.wreck(Impact { pos: self.pos, vel: self.vel, water: false, ground_h: ground.as_ref().map_or(h, |g| g.h), building: None }, "aerodynamic loads");
        }

        // Steering: the stick asks for a rate of turn, and what there is to
        // turn it with gives it: the gimballed engines' side thrust, the
        // booster's fins or the ship's flaps in the airstream, cold-gas
        // thrusters. Let go, it holds its attitude (the integral of the rate
        // error); climbing through thick air it also lets its nose follow the
        // way it is going, so a nudge off the vertical becomes a gravity turn.
        if !self.on_ground {
            let arm = (aft - cg).max(1.0);
            let powered = self.charge > 0.02;
            let push = n_gim as f64 * each * rk.gimbal.sin() + q * st.fins + 3.0e4 + 0.02 * mass;
            let most = DVec3::new(push * arm, push * arm, push * 0.4 * r);
            let (turn, spin) = if self.stage == 0 { (0.035, 0.10) } else if powered { (0.30, 0.30) } else { (0.15, 0.30) };
            let mut want = DVec3::new(self.cmd.x * turn, -self.cmd.y * turn, -self.cmd.z * spin);
            if v > 40.0 && (self.stage == 0 || powered) {
                want += DVec3::new(vb.y, -vb.x, 0.0) / v * (0.5 * smooth(150.0, 800.0, q) * smooth(0.80, 0.92, -vb.z / v));
            }
            let e = want - self.omega;
            self.sas = (self.sas + e * dt).clamp(DVec3::splat(-0.15), DVec3::splat(0.15));
            let mut need = inertia * (e * 4.0 + self.sas * 6.0);
            self.stick = DVec3::ZERO;
            // The ship's flaps: plates either side at the nose and the tail,
            // face on to the air it falls through belly first. Each folds
            // away to shed its share: fore against aft pitches it, left
            // against right rolls it (they know what the air does to the
            // barrel between them, and make up for it).
            let k = 0.5 * rho * vb.y * vb.y * PLATE;
            if self.stage == 1 && k > 1.0 {
                let [[af, xf, zf], [aa, xa, za]] = rk.flaps;
                let (lf, la, way) = (zf - cg, za - cg, -vb.y.signum() * k);
                let (t0, gp, gr) = (-way * (af * lf + aa * la), -2.0 * way * (af * lf - aa * la), 2.0 * way * (af * xf + aa * xa));
                let dp = ((need.x - torque_b.x - t0) / gp).clamp(-0.45, 0.45);
                let dr = ((need.z - torque_b.z) / gr).clamp(dp.abs() - 0.5, 0.5 - dp.abs());
                let tf = DVec3::new(t0 + gp * dp, 0.0, gr * dr);
                force_b.y += way * (af * (1.0 + 2.0 * dp) + aa * (1.0 - 2.0 * dp));
                need -= tf + DVec3::new(torque_b.x, 0.0, torque_b.z);
                torque_b += tf;
                (self.stick.x, self.stick.z) = (2.0 * dp, -3.6 * dr);
            }
            let tau = need.clamp(-most, most);
            torque_b += tau;
            // (What turns it pushes sideways at the tail.)
            force_b += DVec3::new(tau.y / arm, -tau.x / arm, 0.0);
            if self.stick.x == 0.0 {
                self.stick.x = tau.x / most.x;
            }
            if self.stick.z == 0.0 {
                self.stick.z = -tau.z / most.z;
            }
            self.stick.y = -tau.y / most.y;
        } else {
            (self.sas, self.stick) = (DVec3::ZERO, DVec3::ZERO);
        }

        // Gravity, weaker with height.
        let g = 9.81 * ((self.pos.length() - h) / self.pos.length()).powi(2);
        let mut force_w = self.rot * force_b - upw * (mass * g);

        // The ground: it stands (and lands) on the rim of its skirt; anything
        // else of it that touches, or a skirt set down too hard, and the tanks burst.
        self.on_ground = false;
        if self.t > 1.5 && ground.as_ref().is_some_and(|g| h - g.h < len + 50.0) {
            let height = |p: DVec3| {
                let (la, lo, ph) = geo::pos_to_lla(p);
                world.ground(la, lo).map(|g| (g.h - ph, g.water, g.h))
            };
            let pt = |a: f64, z: f64| DVec3::new(r * a.cos(), r * a.sin(), z - o);
            for k in 0..4 {
                let a = std::f64::consts::FRAC_PI_2 * k as f64;
                for body in [pt(a, front + 0.25 * len), pt(a, front + 0.6 * len), DVec3::new(0.0, 0.0, front - o)] {
                    let p = self.pos + self.rot * body;
                    if let Some((pen, water, gh)) = height(p) {
                        if pen > 0.0 {
                            return self.wreck(Impact { pos: p, vel: self.vel, water, ground_h: gh, building: None }, "fell over");
                        }
                    }
                }
            }
            // A spring and damper sized to what it weighs now.
            let (w0, n) = (12.0, 8);
            let (ks, cs) = (mass * w0 * w0 / n as f64, 1.6 * mass * w0 / n as f64);
            for k in 0..n {
                let rb = pt(std::f64::consts::TAU * k as f64 / n as f64, aft);
                let p = self.pos + self.rot * rb;
                let Some((pen, water, gh)) = height(p) else { continue };
                if pen <= 0.0 {
                    continue;
                }
                let pv = self.vel + self.rot * self.omega.cross(rb);
                let vn = pv.dot(upw);
                if vn < -6.0 || water {
                    return self.wreck(Impact { pos: p, vel: self.vel, water, ground_h: gh, building: None }, if water { "water" } else { "hard landing" });
                }
                self.on_ground = true;
                let nf = (pen * ks - vn * cs).max(0.0);
                let tang = pv - upw * vn;
                let f = upw * nf - tang.normalize_or_zero() * (0.7 * nf).min(tang.length() * cs);
                force_w += f;
                torque_b += (rb - cgv).cross(self.rot.inverse() * f);
            }
        }

        // The motion, about the centre of gravity.
        let acc = force_w / mass;
        self.sf = self.rot.inverse() * (acc + upw * g);
        self.g_load = -self.sf.z / 9.81;
        let v_cg = self.vel + self.rot * self.omega.cross(cgv) + acc * dt;
        let p_cg = self.pos + self.rot * cgv + v_cg * dt;
        self.omega += (torque_b - self.omega.cross(inertia * self.omega)) / inertia * dt;
        self.omega = self.omega.clamp_length_max(3.0);
        self.rot = (self.rot * DQuat::from_scaled_axis(self.omega * dt)).normalize();
        self.pos = p_cg - self.rot * cgv;
        self.vel = v_cg - self.rot * self.omega.cross(cgv);
        None
    }
}
