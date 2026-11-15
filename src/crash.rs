//! Crashes: the aircraft breaks into tumbling, burning parts and its fuel goes
//! up in one big rolling fireball, followed by fires along the skid path, a
//! smoke column, a flash of light and scorched ground.
use crate::geo;
use crate::airframe::{self, Mesh};
use crate::damage::{self, Tear};
use crate::fluid::{Domain, FluidSrc};
use crate::plane::{Impact, Plane};
use crate::render::Particle;
use crate::world::World;
use glam::{DQuat, DVec3, Vec3};

pub struct Rng(pub u64);
impl Rng {
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn dir(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.range(-1.0, 1.0), self.range(-1.0, 1.0), self.range(-1.0, 1.0));
            let l = v.length_squared();
            if l > 0.01 && l <= 1.0 {
                return v / l.sqrt();
            }
        }
    }
}

pub struct Debris {
    pub mesh: usize,
    /// the mesh's body-frame point that `pos` refers to
    pub origin: Vec3,
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: DQuat,
    pub omega: DVec3,
    pub scale: f32,
    pub radius: f64,
    pub burn: f32,
    pub char: f32,
    pub resting: bool,
    pub sunk: bool,
    /// breaks it already carried when the aircraft came down (see damage.rs)
    pub tear: [Tear; 2],
    /// how ragged its breaks are
    pub jag: f32,
    emit_acc: f32,
    trail: f32,
}

struct Fire {
    pos: Vec3,
    life: f32,
    size: f32,
    acc: f32,
}

struct Blast {
    at: f64,
    pos: Vec3,
    power: f32,
}

/// A source that feeds the fire simulation for a stretch of time.
struct Burst {
    t0: f64,
    t1: f64,
    src: FluidSrc,
}

pub struct Crash {
    pub site: DVec3,
    pub up: DVec3,
    pub water: bool,
    pub time: f64,
    pub power: f32,
    pub debris: Vec<Debris>,
    fires: Vec<Fire>,
    pending: Vec<Blast>,
    /// (seconds since, power, position) of the most recent blast
    pub last_blast: (f64, f32, Vec3),
    pub scorch: Vec<(Vec3, f32)>,
    pub booms: Vec<f32>,
    rng: Rng,
    skid_last: Vec3,
    column_acc: f32,
    /// the fire simulation's grid, and what feeds it this frame
    pub domain: Option<Domain>,
    pub fluid: Vec<FluidSrc>,
    bursts: Vec<Burst>,
    /// the wreck's velocity just after the hit
    carry: Vec3,
    /// where the smoke leaves the top of the fire simulation's grid (relative
    /// to the site), how much (particles per second), and how fast it rises
    pub top_out: Option<(Vec3, f32, f32)>,
    /// how far above the ground the impact was (into the side of a building):
    /// what falls ends up this far below the site
    pub lift: f64,
    /// when the fire in the street below an elevated impact catches
    street_fire: Option<f64>,
}

pub const GAS: f32 = 0.0;
pub const SPARK: f32 = 1.0;
pub const DUST: f32 = 2.0;
pub const SPRAY: f32 = 3.0;
pub const FLASH: f32 = 4.0;
pub const MIST: f32 = 5.0;
pub const LEAF: f32 = 6.0;
pub const GRIT: f32 = 7.0; // pale masonry dust
pub const SOOT: f32 = 8.0; // smoke carried on from the fire simulation
pub const GLASS: f32 = 9.0; // shards of window glass

impl Crash {
    /// `meshes`: renderer meshes of the aircraft's breakable groups, then the two
    /// shard shapes; `pivots`: each group's centre and size (body frame);
    /// `tears`: the break each group already had.
    pub fn new(imp: &Impact, plane: &Plane, meshes: &[usize], pivots: &[(Vec3, f32)], tears: &[[Tear; 2]], seed: u64, wind: DVec3) -> Self {
        let mut rng = Rng(seed | 1);
        let (lat, lon, _) = geo::pos_to_lla(imp.pos);
        let site = geo::lla_to_pos(lat, lon, imp.ground_h);
        let up = site.normalize();
        let speed = imp.vel.length() as f32;
        let power = (speed / 65.0).clamp(0.7, 2.2);
        let mut debris = Vec::new();
        // Velocity after the hit: most of the downward part is lost, the rest carries on.
        let vn = imp.vel.dot(up);
        let slide = imp.vel - up * vn;
        let carry = slide * 0.55 + up * (vn.abs() * 0.25 + 4.0);
        // The parts: each group as it was, except that the wings go to pieces
        // too, snapping across at a station or two (angled, ragged breaks).
        let mut parts: Vec<(usize, Vec3, f32, [Tear; 2], f32)> = pivots.iter().enumerate().map(|(i, &(p, r))| (meshes[i], p, r, tears.get(i).copied().unwrap_or([Tear::NONE; 2]), damage::JAG)).collect();
        for s in 0..2 {
            let gi = airframe::G_LWING + s;
            let spec = plane.spec;
            let k = spec.span / 5.5;
            let end = plane.dmg.wing[s];
            let cuts = if end < 2.2 * k || rng.f() < 0.2 { 0 } else if end > 3.6 * k && rng.f() < 0.25 + 0.3 * (power - 0.7) { 2 } else { 1 };
            if cuts == 0 {
                continue;
            }
            // From the outer end inward, each break inside the one beyond it.
            let mut st = end;
            let mut breaks = Vec::new();
            let mut prev = plane.dmg.stump(s);
            for c in 0..cuts {
                let n = (cuts - 1 - c) as f32;
                let room = st - (0.9 + 0.8 * n) * k;
                if room < 0.8 * k {
                    break;
                }
                st = rng.range((0.8 + 0.8 * n) * k, room);
                let t = damage::wing_break(spec, s, st, rng.range(-0.7, 0.7) * k, rng.range(-0.25, 0.25), rng.range(0.06, 0.32) * k.sqrt(), &prev);
                breaks.push((st, t));
                prev = t;
            }
            let mesh = parts[gi].0;
            let stump = plane.dmg.stump(s);
            let mut outer = (end, stump);
            for &(st, t) in &breaks {
                // The piece between this break and the one beyond it.
                let mid = damage::wing_pt(spec, s, (st + outer.0) * 0.5, 0.4, 0.0);
                parts.push((mesh, mid, ((outer.0 - st) * 0.5).max(0.4 * k), [t.flip(), outer.1], t.jag));
                outer = (st, t);
            }
            parts[gi] = (mesh, damage::wing_pt(spec, s, outer.0 * 0.5, 0.4, 0.0), (outer.0 * 0.5).max(0.5 * k), [outer.1, Tear::NONE], outer.1.jag.max(0.05));
        }
        for (i, &(mesh, pivot, radius, tear, jag)) in parts.iter().enumerate() {
            let wpos = plane.pos + plane.rot * pivot.as_dvec3();
            let out = (wpos - imp.pos).normalize_or_zero() + up * 0.6;
            let v = carry * rng.range(0.5, 1.1) as f64 + out * rng.range(6.0, 22.0) as f64 * power as f64 + rng.dir().as_dvec3() * (speed * 0.12) as f64;
            // Pieces of wing burn on less long than the fuselage with its tanks' worth.
            let split = i >= pivots.len();
            debris.push(Debris {
                mesh,
                origin: pivot,
                pos: wpos + up * 0.5,
                vel: v,
                rot: plane.rot,
                omega: rng.dir().as_dvec3() * rng.range(2.0, 11.0) as f64,
                scale: 1.0,
                radius: radius as f64 * 0.45,
                burn: if imp.water { rng.range(1.0, 4.0) } else if split { rng.range(6.0, 30.0) } else { rng.range(25.0, 70.0) },
                char: 0.15,
                resting: false,
                sunk: false,
                tear,
                jag,
                emit_acc: 0.0,
                trail: 1.0,
            });
        }
        // Shrapnel: small pieces thrown high, each dragging a smoke streak.
        for k in 0..12 {
            let d = (rng.dir() + up.as_vec3() * 1.1).normalize();
            let sp = rng.range(18.0, 70.0) * power;
            debris.push(Debris {
                mesh: meshes[airframe::N_GROUPS + k % 2],
                origin: Vec3::ZERO,
                pos: site + up * 1.0 + rng.dir().as_dvec3() * 2.0,
                vel: carry * rng.range(0.2, 0.9) as f64 + (d * sp).as_dvec3(),
                rot: DQuat::from_axis_angle(rng.dir().as_dvec3(), rng.range(0.0, 6.0) as f64),
                omega: rng.dir().as_dvec3() * rng.range(5.0, 25.0) as f64,
                scale: rng.range(0.5, 1.7),
                radius: 0.25,
                burn: if imp.water { rng.range(0.5, 2.0) } else { rng.range(3.0, 14.0) },
                char: rng.range(0.2, 0.8),
                resting: false,
                sunk: false,
                tear: [Tear::NONE; 2],
                jag: damage::JAG,
                emit_acc: 0.0,
                trail: rng.range(0.4, 1.0),
            });
        }
        let mut c = Self {
            site,
            up,
            water: imp.water,
            time: 0.0,
            power,
            debris,
            fires: Vec::new(),
            pending: Vec::new(),
            last_blast: (1e9, 0.0, Vec3::ZERO),
            scorch: Vec::new(),
            booms: Vec::new(),
            rng,
            skid_last: Vec3::ZERO,
            column_acc: 0.0,
            domain: Some(Domain::new(site, slide, wind, power)),
            fluid: Vec::new(),
            bursts: Vec::new(),
            carry: carry.as_vec3(),
            top_out: None,
            lift: 0.0,
            street_fire: None,
        };
        // One explosion: all the fuel goes at once.
        c.pending.push(Blast { at: 0.0, pos: up.as_vec3() * 2.5, power });
        if !imp.water {
            c.scorch.push((Vec3::ZERO, 12.0 * power));
            // What is left in the tanks burns on where the wreck came to rest
            // (the source of the smoke column).
            c.fires.push(Fire { pos: up.as_vec3() * 0.3, life: 70.0, size: 1.25 * power, acc: 0.0 });
        }
        c
    }

    /// The impact was `lift` metres up the side of a building. The fire at the
    /// hole only lasts until the floors give; what is left burns in the street.
    pub fn set_lift(&mut self, lift: f64) {
        if lift < 6.0 {
            return;
        }
        self.lift = lift;
        if let Some(d) = self.domain.as_mut() {
            d.vol_lift = lift as f32;
        }
        let down = -self.up.as_vec3() * lift as f32;
        if let Some(f) = self.fires.first_mut() {
            f.life = 6.0;
        }
        self.street_fire = Some(4.0);
        if let Some(s) = self.scorch.first_mut() {
            s.0 = down;
        }
    }

    fn local(&self, p: DVec3) -> Vec3 {
        (p - self.site).as_vec3()
    }

    fn explode(&mut self, at: Vec3, power: f32, out: &mut Vec<Particle>) {
        let up = self.up.as_vec3();
        let rng = &mut self.rng;
        let p = power;
        // Over water most of the fuel is quenched: more splash than fire.
        let fire = if self.water { 0.35 } else { 1.0 };
        out.push(Particle { pos: (at + up * 2.0).into(), life: 0.22, size0: 10.0 * p, size1: 30.0 * p, kind: FLASH, alpha: 1.6 * p * fire, ..Default::default() });
        let e = up.any_orthonormal_vector();
        let nn = up.cross(e);
        if self.domain.is_some() {
            // The fuel: a cloud of spray torn out of the tanks, thrown on with
            // the wreck and outward, igniting as it goes. Several overlapping
            // blobs, staggered a little in time, so the fireball is lumpy and
            // leans the way the aircraft was going.
            let carry = self.carry;
            let fwd = (carry - up * carry.dot(up)).normalize_or_zero();
            let pc = p.cbrt();
            for k in 0..20 {
                let t0 = self.time + k as f64 * 0.012 + rng.range(0.0, 0.06) as f64;
                let off = rng.dir() * rng.range(2.0, 7.0) * pc + fwd * rng.range(0.0, 13.0) * pc + up * rng.range(1.0, 6.0) * pc;
                self.bursts.push(Burst {
                    t0,
                    t1: t0 + rng.range(0.1, 0.2) as f64,
                    src: FluidSrc {
                        pos: at + off,
                        radius: rng.range(2.6, 4.8) * pc,
                        vel: carry * rng.range(0.3, 0.6) + up * rng.range(3.0, 11.0) + rng.dir() * rng.range(0.0, 8.0),
                        radial: rng.range(8.0, 16.0) * p.sqrt(),
                        fuel: 80.0 * fire,
                        soot: 9.0 * fire,
                        dust: 0.0,
                        temp: 1.5 * fire.sqrt(),
                        vel_k: 12.0,
                        temp_k: 30.0,
                        flicker: 0.0,
                        expand: rng.range(8.0, 17.0) * p.sqrt(),
                        turb: 0.8,
                    },
                });
            }
            // A skirt of dust (or mist) blown out along the ground by the blast.
            let n = 14;
            for k in 0..n {
                let a = k as f32 * std::f32::consts::TAU / n as f32 + rng.range(-0.2, 0.2);
                let d = e * a.cos() + nn * a.sin();
                let t0 = self.time + rng.range(0.0, 0.05) as f64;
                self.bursts.push(Burst {
                    t0,
                    t1: t0 + 0.3,
                    src: FluidSrc {
                        pos: at - up * 2.0 + d * rng.range(2.5, 5.0) * p,
                        radius: rng.range(2.5, 4.0),
                        vel: d * rng.range(16.0, 28.0) * p + up * rng.range(1.0, 4.0),
                        radial: 3.0,
                        fuel: 0.0,
                        soot: 0.0,
                        dust: if self.water { 1.0 } else { 6.0 },
                        temp: 0.0,
                        vel_k: 14.0,
                        temp_k: 0.0,
                        flicker: 0.0,
                        expand: 0.0,
                        turb: 0.0,
                    },
                });
            }
        } else {
            for _ in 0..(84.0 * p * fire) as usize {
                let d = rng.dir();
                let f = rng.f().cbrt();
                out.push(Particle { pos: (at + d * f * 3.2 * p).into(), vel: (d * f * 27.0 * p + up * rng.range(4.0, 9.0) * p).into(), life: rng.range(3.4, 5.6), size0: rng.range(2.8, 4.2) * p, size1: rng.range(10.0, 14.5) * p, temp: rng.range(2.3, 3.1), kind: GAS, drag: rng.range(1.9, 2.4), buoy: rng.range(7.0, 10.0), seed: rng.f(), alpha: rng.range(0.75, 0.95), age: 0.0 });
            }
        }
        for _ in 0..(34.0 * p * fire) as usize {
            let d = (rng.dir() + up * 0.8).normalize();
            out.push(Particle { pos: at.into(), vel: (d * rng.range(15.0, 60.0) * p.sqrt()).into(), life: rng.range(0.6, 2.0), size0: rng.range(0.05, 0.12), size1: 0.03, kind: SPARK, drag: rng.range(0.3, 0.9), seed: rng.f(), alpha: rng.range(0.5, 1.0), ..Default::default() });
        }
        if self.water {
            // Low skirt of mist pushed out over the water.
            for k in 0..(60.0 * p) as usize {
                let a = k as f32 * 2.399 + rng.f();
                let d = e * a.cos() + nn * a.sin();
                out.push(Particle { pos: (at + d * rng.range(1.0, 5.0) - up * 1.5).into(), vel: (d * rng.range(14.0, 34.0) * p + up * rng.range(0.0, 3.0)).into(), life: rng.range(3.0, 6.0), size0: rng.range(2.0, 3.5), size1: rng.range(8.0, 13.0) * p, temp: 0.25, kind: MIST, drag: rng.range(1.8, 2.6), buoy: 1.0, seed: rng.f(), alpha: rng.range(0.22, 0.4), age: 0.0 });
            }
        }
        if self.water {
            // Column of water thrown up by the impact.
            for _ in 0..(300.0 * p) as usize {
                let d = (up * rng.range(1.0, 3.2) + rng.dir() * 0.8).normalize();
                out.push(Particle { pos: (at + rng.dir() * rng.range(0.0, 3.0)).into(), vel: (d * rng.range(12.0, 50.0) * p).into(), life: rng.range(1.6, 4.5), size0: rng.range(0.5, 1.2), size1: rng.range(2.5, 5.5), kind: SPRAY, drag: rng.range(0.25, 0.6), seed: rng.f(), alpha: rng.range(0.45, 0.8), ..Default::default() });
            }
        }
        self.last_blast = (0.0, power, at);
        self.booms.push(power);
    }

    /// Advance the wreck. `dt` is game time.
    pub fn update(&mut self, dt: f64, world: &World, out: &mut Vec<Particle>) {
        self.time += dt;
        self.last_blast.0 += dt;
        self.fluid.clear();
        let now = self.time;
        self.fluid.extend(self.bursts.iter().filter(|b| b.t0 <= now && b.t1 > now).map(|b| b.src));
        self.bursts.retain(|b| b.t1 > now);
        let dtf = dt as f32;
        let up = self.up.as_vec3();
        if self.street_fire.is_some_and(|t| now >= t) {
            self.street_fire = None;
            self.fires.push(Fire { pos: -up * self.lift as f32 + up * 0.3, life: 70.0, size: 1.1 * self.power, acc: 0.0 });
        }

        // Scheduled blasts go off at whichever big burning part is nearest the idea.
        let due: Vec<Blast> = {
            let t = self.time;
            let (now, later): (Vec<Blast>, Vec<Blast>) = std::mem::take(&mut self.pending).into_iter().partition(|b| b.at <= t);
            self.pending = later;
            now
        };
        for (k, b) in due.into_iter().enumerate() {
            let pos = if b.pos.is_nan() {
                let pick = [2usize, 3, 0][(self.time * 7.0) as usize % 3 + k * 0 % 3];
                self.debris.get(pick).filter(|d| !d.sunk).map(|d| self.local(d.pos) + up).unwrap_or(up * 2.0)
            } else {
                b.pos
            };
            self.explode(pos, b.power, out);
        }

        let n_scorch = self.scorch.len();
        for i in 0..self.debris.len() {
            let site = self.site;
            let d = &mut self.debris[i];
            if d.sunk {
                continue;
            }
            let big = d.mesh < 7;
            if !d.resting {
                let upw = d.pos.normalize();
                d.vel -= upw * 9.81 * dt;
                d.vel *= 1.0 - (0.02 + if big { 0.0 } else { 0.25 }) * dt;
                d.pos += d.vel * dt;
                let w = d.omega * dt;
                if w.length_squared() > 1e-12 {
                    d.rot = (DQuat::from_scaled_axis(w) * d.rot).normalize();
                }
                let (lat, lon, h) = geo::pos_to_lla(d.pos);
                let mut gh = None;
                if let Some(g) = world.ground(lat, lon) {
                    gh = Some((g.h, g.water));
                }
                if let Some(b) = world.building(lat, lon) {
                    if h < b.top + 1.0 && (!b.bridge || h > b.base) {
                        gh = Some((b.top, false));
                    }
                }
                if let Some((g, water)) = gh {
                    if h < g + d.radius {
                        if water {
                            // Splash and sink.
                            let lp = (d.pos - site).as_vec3();
                            for _ in 0..if big { 60 } else { 10 } {
                                let dir = (up * self.rng.range(1.0, 3.0) + self.rng.dir()).normalize();
                                out.push(Particle { pos: lp.into(), vel: (dir * self.rng.range(5.0, 22.0)).into(), life: self.rng.range(0.8, 2.2), size0: 0.4, size1: self.rng.range(1.5, 3.5), kind: SPRAY, drag: 0.5, seed: self.rng.f(), alpha: 0.6, ..Default::default() });
                            }
                            d.sunk = true;
                            continue;
                        }
                        d.pos += upw * (g + d.radius - h);
                        let vn = d.vel.dot(upw);
                        let tang = d.vel - upw * vn;
                        if vn < 0.0 {
                            d.vel = tang * (1.0 - (6.0 * dt).min(0.6)) - upw * vn * 0.28;
                            d.omega *= 0.82;
                            // Sparks where metal grinds along the ground.
                            if tang.length() > 6.0 {
                                let lp = (d.pos - site).as_vec3();
                                for _ in 0..3 {
                                    let dir = (tang.as_vec3().normalize() * 0.6 + up + self.rng.dir() * 0.7).normalize();
                                    out.push(Particle { pos: lp.into(), vel: (dir * self.rng.range(4.0, 18.0)).into(), life: self.rng.range(0.3, 1.1), size0: 0.05, size1: 0.02, kind: SPARK, drag: 0.6, seed: self.rng.f(), alpha: 0.8, ..Default::default() });
                                }
                            }
                        }
                        if d.vel.length() < 0.8 && vn.abs() < 1.0 {
                            d.resting = true;
                            d.vel = DVec3::ZERO;
                        }
                        // Burning fuel left behind where the big pieces slide.
                        if big && d.mesh != 5 && d.burn > 5.0 {
                            let lp = (d.pos - site).as_vec3() - up * d.radius as f32;
                            if (lp - self.skid_last).length() > 9.0 && self.fires.len() < 8 {
                                self.skid_last = lp;
                                self.fires.push(Fire { pos: lp, life: self.rng.range(14.0, 40.0), size: self.rng.range(0.8, 1.8), acc: 0.0 });
                                if self.scorch.len() < 8 && n_scorch < 8 {
                                    self.scorch.push((lp, self.rng.range(3.5, 6.5)));
                                }
                            }
                        }
                    }
                }
            }
            // Fire and smoke streaming off burning wreckage.
            if d.burn > 0.0 {
                d.burn -= dtf;
                d.char = (d.char + dtf * 0.08).min(1.0);
                let moving = d.vel.length() as f32;
                // Fast pieces leave a puff every metre or so, so the trail reads as
                // one streak; each is fainter to keep the streak's density.
                let base = if big { 20.0 } else { 9.0 * d.trail } * (0.4 + (d.burn / 8.0).min(1.0));
                let rate = base + moving * if big { 0.8 } else { 1.2 * d.trail };
                let thin = (base / rate).sqrt();
                d.emit_acc += rate * dtf;
                let lp = (d.pos - site).as_vec3();
                let lv = d.vel.as_vec3();
                if self.domain.as_ref().is_some_and(|dm| dm.contains(lp, 3.0)) {
                    // Burning wreckage feeds the fire simulation instead.
                    d.emit_acc = 0.0;
                    let k = 0.4 + (d.burn / 8.0).min(1.0);
                    self.fluid.push(if big {
                        FluidSrc { pos: lp, radius: (d.radius as f32 * 1.6).max(1.6), vel: lv, radial: 0.5, fuel: 1.6 * k, soot: 0.35 * k, dust: 0.0, temp: 1.3, vel_k: 5.0, temp_k: 8.0, flicker: 0.7, expand: 0.0, turb: 0.0 }
                    } else {
                        FluidSrc { pos: lp, radius: 1.2, vel: lv, radial: 0.0, fuel: 0.3 * k * d.trail, soot: 1.4 * k * d.trail, dust: 0.0, temp: 0.8, vel_k: 6.0, temp_k: 8.0, flicker: 0.0, expand: 0.0, turb: 0.0 }
                    });
                }
                while d.emit_acc >= 1.0 {
                    d.emit_acc -= 1.0;
                    let r = &mut self.rng;
                    let s = if big { 1.0 } else { 0.45 };
                    let hot = r.f() < if big { 0.45 } else { 0.03 };
                    out.push(Particle {
                        pos: (lp + r.dir() * d.radius as f32 * 0.8 - lv * dtf * r.f()).into(),
                        vel: (lv * 0.25 + r.dir() * 1.2 + up * r.range(0.5, 3.0)).into(),
                        life: if hot { r.range(0.6, 1.5) } else { r.range(2.5, 6.5) } * (0.6 + 0.4 * s),
                        size0: r.range(0.5, 1.1) * s,
                        size1: if hot { r.range(1.6, 3.0) } else { r.range(3.5, 7.5) } * s * (1.0 + moving * 0.01),
                        temp: if hot { r.range(1.3, 2.0) } else { r.range(0.3, 0.6) },
                        kind: if hot { GAS } else { SOOT },
                        drag: 1.0,
                        buoy: if hot { 7.0 } else { 4.0 },
                        seed: r.f(),
                        alpha: if hot { 0.85 } else { 0.5 } * thin,
                        age: 0.0,
                    });
                }
            }
        }

        // Pool fires.
        for f in self.fires.iter_mut() {
            f.life -= dtf;
            if f.life <= 0.0 {
                continue;
            }
            let k = (f.life / 6.0).min(1.0);
            if let Some(dm) = self.domain.as_ref().filter(|dm| dm.contains(f.pos, 2.0)) {
                let _ = dm;
                let flick = 0.75 + 0.5 * self.rng.f();
                self.fluid.push(FluidSrc { pos: f.pos + up * 0.6, radius: f.size * 2.2 + 1.2, vel: up * 1.5, radial: 0.6, fuel: 3.0 * k * flick, soot: 0.45 * k, dust: 0.0, temp: 1.35, vel_k: 2.0, temp_k: 6.0, flicker: 1.0, expand: 0.0, turb: 0.0 });
                continue;
            }
            f.acc += 16.0 * k * dtf;
            while f.acc >= 1.0 {
                f.acc -= 1.0;
                let r = &mut self.rng;
                let hot = r.f() < 0.6;
                let off = r.dir() * f.size * 1.4;
                out.push(Particle {
                    pos: (f.pos + off - up * off.dot(up) + up * 0.3).into(),
                    vel: (up * r.range(1.0, 4.0) + r.dir() * 0.8).into(),
                    life: if hot { r.range(0.7, 1.4) } else { r.range(4.0, 9.0) },
                    size0: r.range(0.5, 1.0) * f.size,
                    size1: if hot { r.range(1.8, 3.2) } else { r.range(5.0, 10.0) } * f.size,
                    temp: if hot { r.range(1.4, 2.1) } else { 0.45 },
                    kind: GAS,
                    drag: 0.9,
                    buoy: if hot { 8.0 } else { 4.5 },
                    seed: r.f(),
                    alpha: if hot { 0.9 } else { 0.5 },
                    age: 0.0,
                });
            }
        }
        self.fires.retain(|f| f.life > 0.0);

        // The main smoke column over the site (steam over water).
        let col_life = if self.water { 6.0 } else { 75.0 };
        if self.time < col_life && self.domain.is_none() {
            let k = (1.0 - (self.time / col_life) as f32).powf(0.6);
            self.column_acc += 11.0 * k * dtf * self.power.sqrt();
            while self.column_acc >= 1.0 {
                self.column_acc -= 1.0;
                let r = &mut self.rng;
                out.push(Particle {
                    pos: (r.dir() * 3.0 * self.power + up * 1.0).into(),
                    vel: (up * r.range(3.0, 9.0) + r.dir() * 1.5).into(),
                    life: r.range(12.0, 24.0),
                    size0: r.range(2.0, 4.0),
                    size1: r.range(18.0, 34.0) * self.power.sqrt(),
                    temp: if self.water { 0.2 } else { r.range(0.5, 1.1) },
                    kind: if self.water { MIST } else { GAS },
                    drag: 0.55,
                    buoy: 6.0,
                    seed: r.f(),
                    alpha: if self.water { 0.3 } else { 0.62 },
                    age: 0.0,
                });
            }
        }
        // Above the grid the column carries on as particles.
        if let Some((at, rate, rise)) = self.top_out {
            // Never so few that the column breaks into beads: below that, fainter puffs.
            let n = rate.clamp(12.0, 40.0);
            let faint = (rate / n).min(1.0);
            self.column_acc += n * dtf;
            let wide = (rate.sqrt() * 2.0).clamp(3.0, 12.0);
            while self.column_acc >= 1.0 {
                self.column_acc -= 1.0;
                let r = &mut self.rng;
                let d = r.dir();
                out.push(Particle {
                    pos: (at + (d - up * d.dot(up)) * wide * r.f().sqrt()).into(),
                    vel: (up * rise.clamp(2.0, 12.0) * r.range(0.7, 1.1) + r.dir() * 1.0).into(),
                    life: r.range(14.0, 26.0),
                    size0: r.range(4.0, 6.5),
                    size1: r.range(18.0, 32.0) * self.power.sqrt(),
                    temp: r.range(0.35, 0.6),
                    kind: SOOT,
                    drag: 0.5,
                    buoy: 4.0,
                    seed: r.f(),
                    alpha: r.range(0.55, 0.7) * faint,
                    age: 0.0,
                });
            }
        }
    }

    /// Brightness of the fire light: a hard flash per blast over a flickering glow.
    pub fn light(&self) -> f32 {
        let (t, p, _) = self.last_blast;
        let flash = p * 9.0 * (-(t as f32) / 0.3).exp() + p * 5.0 * (-(t as f32) / 1.8).exp();
        let burning = self.debris.iter().filter(|d| d.burn > 0.0 && !d.sunk && d.mesh < 7).count() as f32;
        let glow = (burning * 1.3 + self.fires.len() as f32 * 0.5) * (0.8 + 0.2 * ((self.time * 23.0).sin() * (self.time * 7.1).cos()) as f32);
        flash + glow
    }
}


/// Two generic bits of wreckage: a torn skin panel and a lump of metal.
pub fn shards() -> [Mesh; 2] {
    let mut panel = Mesh::default();
    let p = [Vec3::new(-0.6, 0.0, -0.4), Vec3::new(0.5, 0.08, -0.5), Vec3::new(0.7, -0.05, 0.4), Vec3::new(-0.4, 0.03, 0.55)];
    panel.quad(&airframe::PAINT_WHITE, p);
    panel.quad(&airframe::ALU, [p[3], p[2], p[1], p[0]]);
    let mut chunk = Mesh::default();
    chunk.rbox(&airframe::STEEL, Vec3::ZERO, Vec3::new(0.22, 0.16, 0.3), 0.03, glam::Quat::IDENTITY);
    [panel, chunk]
}
