//! Crashes: the aircraft breaks into tumbling, burning parts and its fuel goes
//! up in one big rolling fireball, followed by fires along the skid path, a
//! smoke column, a flash of light and scorched ground.
use crate::geo;
use crate::airframe::{self, Mesh};
use crate::damage::{self, Tear};
use crate::fluid::{Domain, FluidSrc};
use crate::plane::{Impact, Plane};
use crate::render::Particle;
use crate::world::{BHit, World};
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
    /// which of the aircraft's groups it is part of (None: a shard)
    pub group: Option<usize>,
    /// a heavy piece moves as a box (half its size, body frame, about
    /// `origin`) of this mass that slides, tumbles and ploughs through what
    /// is in its way; a light one as a ball
    pub rigid: bool,
    half: Vec3,
    mass: f32,
    /// body-frame inverse moments of inertia
    inv_i: DVec3,
    /// how long it has barely moved
    still: f32,
    /// a piece of wing that still has fuel in it to burst
    tank: bool,
    /// where it last left burning fuel behind
    skid: Vec3,
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
    /// how big the aircraft is next to a light single (1); `heavy` for an airliner
    pub size: f32,
    heavy: bool,
    /// buildings the wreckage ploughed through this frame (tile, footprint,
    /// where, velocity): main.rs brings them down
    pub crush: Vec<(geo::TileId, usize, DVec3, DVec3)>,
    crushed: Vec<(geo::TileId, usize)>,
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
    /// `tears`: the break each group already had; `groups`: the groups' meshes
    /// (body frame). `through`: the share of its speed the aircraft kept going
    /// through the building it hit (0: it stopped there, or hit the ground).
    pub fn new(imp: &Impact, plane: &Plane, meshes: &[usize], groups: &[Mesh], pivots: &[(Vec3, f32)], tears: &[[Tear; 2]], through: f32, seed: u64, wind: DVec3) -> Self {
        let mut rng = Rng(seed | 1);
        let spec = plane.spec;
        let size = (spec.span / 5.5).max(1.0);
        let heavy = size > 2.0;
        let (lat, lon, _) = geo::pos_to_lla(imp.pos);
        let site = geo::lla_to_pos(lat, lon, imp.ground_h);
        let up = site.normalize();
        let speed = imp.vel.length() as f32;
        let power = (speed / 65.0).clamp(0.7, 2.2);
        let mut debris = Vec::new();
        // Velocity after the hit: most of the downward part is lost, the rest carries on.
        let vn = imp.vel.dot(up);
        let slide = imp.vel - up * vn;
        let carry = if !heavy {
            slide * 0.55 + up * (vn.abs() * 0.25 + 4.0)
        } else if through > 0.0 {
            // Through a house or two: an airliner hardly slows down for them.
            imp.vel * through as f64
        } else if imp.building.is_some() {
            slide * 0.25 + up * 2.0
        } else {
            // Into the ground: the pieces hit it as boxes and slide on from there.
            imp.vel * 0.95
        };
        // The parts: each group as it was, except that the wings go to pieces
        // too, snapping across at a station or two (angled, ragged breaks).
        let mut parts: Vec<(usize, usize, Vec3, f32, [Tear; 2], f32)> = pivots.iter().enumerate().map(|(i, &(p, r))| (i, meshes[i], p, r, tears.get(i).copied().unwrap_or([Tear::NONE; 2]), damage::JAG)).collect();
        for s in 0..2 {
            let gi = airframe::G_LWING + s;
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
            let mesh = parts[gi].1;
            let stump = plane.dmg.stump(s);
            let mut outer = (end, stump);
            for &(st, t) in &breaks {
                // The piece between this break and the one beyond it.
                let mid = damage::wing_pt(spec, s, (st + outer.0) * 0.5, 0.4, 0.0);
                parts.push((gi, mesh, mid, ((outer.0 - st) * 0.5).max(0.4 * k), [t.flip(), outer.1], t.jag));
                outer = (st, t);
            }
            parts[gi] = (gi, mesh, damage::wing_pt(spec, s, outer.0 * 0.5, 0.4, 0.0), (outer.0 * 0.5).max(0.5 * k), [outer.1, Tear::NONE], outer.1.jag.max(0.05));
        }
        if heavy {
            // An airliner's fuselage breaks into sections: behind the wing,
            // and on a hard hit ahead of it too.
            let gf = airframe::G_FUSE;
            if !parts[gf].4[0].is_some() {
                if let Some((lo, hi, _)) = kept_box(&groups[gf], &[Tear::NONE; 2]) {
                    let len = hi.z - lo.z;
                    let mut zs = vec![lo.z + len * rng.range(0.56, 0.7)];
                    if rng.f() < 0.3 + 0.35 * (power - 0.7) {
                        zs.insert(0, lo.z + len * rng.range(0.22, 0.34));
                    }
                    let cuts: Vec<Tear> = zs.iter().map(|&z| damage::tail_break(spec, z, rng.range(-0.3, 0.3), rng.range(-0.35, 0.35), rng.range(0.1, 0.35) * size.sqrt())).collect();
                    let mesh = parts[gf].1;
                    for (k, t) in cuts.iter().enumerate() {
                        let aft = cuts.get(k + 1).copied().unwrap_or(Tear::NONE);
                        parts.push((gf, mesh, Vec3::ZERO, 1.0, [t.flip(), aft], t.jag));
                    }
                    parts[gf].4 = [cuts[0], Tear::NONE];
                    parts[gf].5 = cuts[0].jag;
                }
            }
            // The gear is torn off and buried in the rest.
            parts.retain(|p| p.0 != airframe::G_GEAR);
        }
        // How much of its group's mesh (and mass) each piece keeps.
        let shares: Vec<Option<(Vec3, Vec3, f32)>> = parts.iter().map(|p| if heavy { kept_box(&groups[p.0], &p.4) } else { None }).collect();
        let fuse_omega = plane.rot * plane.omega;
        for (i, &(g, mesh, pivot, radius, tear, jag)) in parts.iter().enumerate() {
            // Pieces of wing burn on less long than the fuselage with its tanks' worth.
            let split = i >= pivots.len() || (heavy && i >= pivots.len() - 1);
            let burn = if imp.water { rng.range(1.0, 4.0) } else if split { rng.range(6.0, 30.0) } else { rng.range(25.0, 70.0) };
            let mut d = Debris {
                mesh,
                origin: pivot,
                pos: DVec3::ZERO,
                vel: DVec3::ZERO,
                rot: plane.rot,
                omega: DVec3::ZERO,
                scale: 1.0,
                radius: radius as f64 * 0.45,
                burn,
                char: 0.15,
                resting: false,
                sunk: false,
                tear,
                jag,
                emit_acc: 0.0,
                trail: 1.0,
                group: Some(g),
                rigid: false,
                half: Vec3::ZERO,
                mass: 0.0,
                inv_i: DVec3::ZERO,
                still: 0.0,
                tank: false,
                skid: Vec3::splat(1e6),
            };
            if let Some((lo, hi, share)) = shares[i] {
                // A heavy piece: a box as big as what it kept, that goes on
                // the way the aircraft was going and turns as it was turning.
                let c = (lo + hi) * 0.5;
                let half = ((hi - lo) * 0.5).max(Vec3::splat(0.3));
                let m = (spec.groups[g].1 * share).max(200.0);
                let wpos = plane.pos + plane.rot * c.as_dvec3();
                let out = (wpos - imp.pos).normalize_or_zero();
                let out = out - up * out.dot(up) * 0.7;
                let r = wpos - plane.pos;
                d.origin = c;
                d.pos = wpos;
                d.vel = carry * rng.range(0.88, 1.0) as f64 + fuse_omega.cross(r) * 0.5 + out * (rng.range(0.5, 3.0) * power) as f64 + rng.dir().as_dvec3() * (speed * 0.02) as f64;
                d.omega = fuse_omega * 0.7 + rng.dir().as_dvec3() * rng.range(0.05, if split { 0.6 } else { 0.3 }) as f64;
                d.radius = half.length() as f64 * 0.5;
                d.rigid = true;
                d.half = half;
                d.mass = m;
                // (a hollow shell: more of its mass far out than a solid box)
                let full = (half * 2.0).as_dvec3();
                let ib = DVec3::new(full.y * full.y + full.z * full.z, full.x * full.x + full.z * full.z, full.x * full.x + full.y * full.y) * (m as f64 / 12.0 * 1.6);
                d.inv_i = DVec3::ONE / ib;
                d.tank = g == airframe::G_LWING || g == airframe::G_RWING;
                d.burn = if imp.water { rng.range(2.0, 6.0) } else if g == airframe::G_FUSE || d.tank { rng.range(45.0, 85.0) } else { rng.range(15.0, 45.0) };
            } else {
                let wpos = plane.pos + plane.rot * pivot.as_dvec3();
                let out = (wpos - imp.pos).normalize_or_zero() + up * 0.6;
                d.pos = wpos + up * 0.5;
                d.vel = carry * rng.range(0.5, 1.1) as f64 + out * rng.range(6.0, 22.0) as f64 * power as f64 + rng.dir().as_dvec3() * (speed * 0.12) as f64;
                d.omega = rng.dir().as_dvec3() * rng.range(2.0, 11.0) as f64;
            }
            debris.push(d);
        }
        // Shrapnel: small pieces thrown high, each dragging a smoke streak.
        let (n_shard, ss) = if heavy { (24, size.sqrt()) } else { (12, 1.0) };
        for k in 0..n_shard {
            let d = (rng.dir() + up.as_vec3() * 1.1).normalize();
            let sp = if heavy { rng.range(12.0, 45.0) } else { rng.range(18.0, 70.0) } * power;
            debris.push(Debris {
                mesh: meshes[airframe::N_GROUPS + k % 2],
                origin: Vec3::ZERO,
                pos: site + up * 1.0 + rng.dir().as_dvec3() * 2.0 * ss as f64,
                vel: carry * rng.range(0.2, 0.9) as f64 + (d * sp).as_dvec3(),
                rot: DQuat::from_axis_angle(rng.dir().as_dvec3(), rng.range(0.0, 6.0) as f64),
                omega: rng.dir().as_dvec3() * rng.range(5.0, 25.0) as f64 / ss as f64,
                scale: rng.range(0.5, 1.7) * ss,
                radius: 0.25 * ss as f64,
                burn: if imp.water { rng.range(0.5, 2.0) } else { rng.range(3.0, 14.0) },
                char: rng.range(0.2, 0.8),
                resting: false,
                sunk: false,
                tear: [Tear::NONE; 2],
                jag: damage::JAG,
                emit_acc: 0.0,
                trail: rng.range(0.4, 1.0),
                group: None,
                rigid: false,
                half: Vec3::ZERO,
                mass: 0.0,
                inv_i: DVec3::ZERO,
                still: 0.0,
                tank: false,
                skid: Vec3::ZERO,
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
            domain: Some(Domain::new(site, slide, wind, power, size)),
            fluid: Vec::new(),
            bursts: Vec::new(),
            carry: carry.as_vec3(),
            top_out: None,
            lift: 0.0,
            street_fire: None,
            size,
            heavy,
            crush: Vec::new(),
            crushed: Vec::new(),
        };
        // One explosion: all the fuel goes at once.
        c.pending.push(Blast { at: 0.0, pos: up.as_vec3() * 2.5, power });
        if heavy && !imp.water {
            // An airliner's tanks do not all go at once: what is left in the
            // wings bursts as they break up and tumble on.
            c.pending.push(Blast { at: c.rng.range(0.35, 0.9) as f64, pos: Vec3::NAN, power: power * 0.55 });
            c.pending.push(Blast { at: c.rng.range(1.3, 2.6) as f64, pos: Vec3::NAN, power: power * 0.4 });
        }
        if !imp.water {
            let big = if heavy { size.powf(0.75) } else { 1.0 };
            c.scorch.push((Vec3::ZERO, 12.0 * power * big));
            // What is left in the tanks burns on where the wreck came to rest
            // (the source of the smoke column).
            c.fires.push(Fire { pos: up.as_vec3() * 0.3, life: if heavy { 85.0 } else { 70.0 }, size: 1.25 * power * if heavy { size.powf(0.8) } else { 1.0 }, acc: 0.0 });
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
        // Everything scales with the aircraft: lengths as its size, speeds
        // and times as the root of it (a big fireball rises and rolls as a
        // small one does, only slower).
        let (s, ss) = (self.size.powf(0.9), self.size.sqrt());
        out.push(Particle { pos: (at + up * 2.0 * s).into(), life: 0.22 * ss, size0: 10.0 * p * s, size1: 30.0 * p * s, kind: FLASH, alpha: 1.6 * p * fire, ..Default::default() });
        let e = up.any_orthonormal_vector();
        let nn = up.cross(e);
        // (a heavy wreck's later bursts may happen past the grid's edge)
        let heavy = self.heavy;
        if self.domain.as_ref().is_some_and(|d| !heavy || d.contains(at, 8.0 * s)) {
            // The fuel: a cloud of spray torn out of the tanks, thrown on with
            // the wreck and outward, igniting as it goes. Several overlapping
            // blobs, staggered a little in time, so the fireball is lumpy and
            // leans the way the aircraft was going.
            let carry = if heavy { self.carry - up * self.carry.dot(up) } else { self.carry };
            let fwd = (carry - up * carry.dot(up)).normalize_or_zero();
            let pc = p.cbrt();
            // (an airliner carries far more fuel for its size: more blobs)
            let n_blob = if heavy { 34 } else { 20 };
            for k in 0..n_blob {
                let t0 = self.time + (k as f64 * 0.012 + rng.range(0.0, 0.06) as f64) * ss as f64;
                let off = (rng.dir() * rng.range(2.0, 7.0) * pc + fwd * rng.range(0.0, 13.0) * pc + up * rng.range(1.0, 6.0) * pc) * s;
                self.bursts.push(Burst {
                    t0,
                    t1: t0 + (rng.range(0.1, 0.2) * ss) as f64,
                    src: FluidSrc {
                        pos: at + off,
                        radius: rng.range(2.6, 4.8) * pc * s,
                        // (the spray slows in the air far quicker than the wreck on the ground)
                        vel: carry * if heavy { rng.range(0.08, 0.2) } else { rng.range(0.3, 0.6) } + (up * rng.range(3.0, 11.0) + rng.dir() * rng.range(0.0, 8.0)) * ss,
                        radial: rng.range(8.0, 16.0) * p.sqrt() * ss,
                        // (fed for longer, so as much per volume as a small one's)
                        fuel: 80.0 * fire / ss,
                        soot: 9.0 * fire / ss,
                        dust: 0.0,
                        temp: 1.5 * fire.sqrt(),
                        vel_k: 12.0,
                        temp_k: 30.0,
                        flicker: 0.0,
                        expand: rng.range(8.0, 17.0) * p.sqrt() / ss,
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
                    t1: t0 + 0.3 * ss as f64,
                    src: FluidSrc {
                        pos: at - up * 2.0 + d * rng.range(2.5, 5.0) * p * s,
                        radius: rng.range(2.5, 4.0) * s,
                        vel: (d * rng.range(16.0, 28.0) * p + up * rng.range(1.0, 4.0)) * ss,
                        radial: 3.0 * ss,
                        fuel: 0.0,
                        soot: 0.0,
                        dust: if self.water { 1.0 } else { 6.0 } / ss,
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
            for _ in 0..((84.0 * p * fire * ss) as usize).min(320) {
                let d = rng.dir();
                let f = rng.f().cbrt();
                out.push(Particle { pos: (at + d * f * 3.2 * p * s).into(), vel: ((d * f * 27.0 * p + up * rng.range(4.0, 9.0) * p) * ss).into(), life: rng.range(3.4, 5.6) * ss, size0: rng.range(2.8, 4.2) * p * s, size1: rng.range(10.0, 14.5) * p * s, temp: rng.range(2.3, 3.1), kind: GAS, drag: rng.range(1.9, 2.4) / ss, buoy: rng.range(7.0, 10.0), seed: rng.f(), alpha: rng.range(0.75, 0.95), age: 0.0 });
            }
        }
        for _ in 0..(34.0 * p * fire * ss) as usize {
            let d = (rng.dir() + up * 0.8).normalize();
            out.push(Particle { pos: at.into(), vel: (d * rng.range(15.0, 60.0) * p.sqrt() * ss).into(), life: rng.range(0.6, 2.0) * ss, size0: rng.range(0.05, 0.12) * ss, size1: 0.03, kind: SPARK, drag: rng.range(0.3, 0.9), seed: rng.f(), alpha: rng.range(0.5, 1.0), ..Default::default() });
        }
        if self.water {
            // Low skirt of mist pushed out over the water.
            for k in 0..(60.0 * p * ss) as usize {
                let a = k as f32 * 2.399 + rng.f();
                let d = e * a.cos() + nn * a.sin();
                out.push(Particle { pos: (at + (d * rng.range(1.0, 5.0) - up * 1.5) * s).into(), vel: ((d * rng.range(14.0, 34.0) * p + up * rng.range(0.0, 3.0)) * ss).into(), life: rng.range(3.0, 6.0) * ss, size0: rng.range(2.0, 3.5) * s, size1: rng.range(8.0, 13.0) * p * s, temp: 0.25, kind: MIST, drag: rng.range(1.8, 2.6) / ss, buoy: 1.0, seed: rng.f(), alpha: rng.range(0.22, 0.4), age: 0.0 });
            }
        }
        if self.water {
            // Column of water thrown up by the impact.
            for _ in 0..(300.0 * p * ss) as usize {
                let d = (up * rng.range(1.0, 3.2) + rng.dir() * 0.8).normalize();
                out.push(Particle { pos: (at + rng.dir() * rng.range(0.0, 3.0) * s).into(), vel: (d * rng.range(12.0, 50.0) * p * ss).into(), life: rng.range(1.6, 4.5) * ss, size0: rng.range(0.5, 1.2) * ss, size1: rng.range(2.5, 5.5) * ss, kind: SPRAY, drag: rng.range(0.25, 0.6), seed: rng.f(), alpha: rng.range(0.45, 0.8), ..Default::default() });
            }
        }
        self.last_blast = (0.0, power * ss.min(2.5), at);
        self.booms.push(if heavy { (power * 1.5).min(2.2) } else { power });
    }

    /// Advance the wreck. `dt` is game time.
    pub fn update(&mut self, dt: f64, world: &World, out: &mut Vec<Particle>) {
        self.time += dt;
        self.last_blast.0 += dt;
        self.fluid.clear();
        let now = self.time;
        self.fluid.extend(self.bursts.iter().filter(|b| b.t0 <= now && b.t1 > now).map(|b| b.src));
        let n_burst = self.fluid.len();
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
            let big = d.group.is_some();
            if d.rigid && !d.resting {
                let prev = d.pos;
                let c = rigid_step(d, dt, world, &self.crushed);
                let upw = d.pos.normalize();
                if c.water {
                    let lp = (d.pos - site).as_vec3();
                    for _ in 0..120 {
                        let dir = (up * self.rng.range(1.0, 3.0) + self.rng.dir()).normalize();
                        out.push(Particle { pos: (lp + self.rng.dir() * d.half.max_element() * 0.7).into(), vel: (dir * self.rng.range(8.0, 35.0)).into(), life: self.rng.range(1.2, 3.5), size0: 1.0, size1: self.rng.range(3.0, 7.0), kind: SPRAY, drag: 0.4, seed: self.rng.f(), alpha: 0.6, ..Default::default() });
                    }
                    d.sunk = true;
                    continue;
                }
                if let Some((b, p)) = c.build {
                    // Into a building: a house is no match for an airliner's
                    // wreckage, which goes through it, slowed by about its
                    // weight; what stops it stands.
                    let hgt = (b.top - b.base).max(3.0);
                    let mb = b.area.max(25.0) as f64 * hgt * 120.0 * 0.3;
                    let m = d.mass as f64;
                    let keep = m / (m + mb);
                    if d.vel.length() * keep > 4.0 {
                        self.crushed.push((b.tile, b.idx));
                        self.crush.push((b.tile, b.idx, p, d.vel));
                        d.vel *= keep;
                        d.omega *= keep.sqrt();
                        let (la, lo, _) = geo::pos_to_lla(p);
                        let base = (geo::lla_to_pos(la, lo, b.base) - site).as_vec3();
                        // Fuel thrown into it: the ruin burns.
                        if d.burn > 0.0 && self.fires.len() < 24 {
                            self.fires.push(Fire { pos: base + up * 0.5, life: self.rng.range(30.0, 70.0), size: (b.area.sqrt() / 7.0).clamp(1.0, 3.5), acc: 0.0 });
                        }
                        // A piece of wing bursts its tank against it.
                        if d.tank && d.burn > 0.0 && self.pending.len() < 6 {
                            d.tank = false;
                            self.pending.push(Blast { at: self.time + 0.05, pos: (d.pos - site).as_vec3() + up * 2.0, power: self.power * 0.4 });
                        }
                    } else {
                        // Stopped against it.
                        let vu = d.vel.dot(upw);
                        d.pos = prev;
                        d.vel = upw * vu.min(0.0) - (d.vel - upw * vu) * 0.15;
                        d.omega *= 0.5;
                    }
                }
                let lp = (d.pos - site).as_vec3();
                if c.slide > 4.0 {
                    let at = (c.at - site).as_vec3();
                    let tang = (d.vel - upw * d.vel.dot(upw)).as_vec3();
                    let dir0 = tang.normalize_or_zero();
                    // Sparks where metal grinds along, and the earth it throws up.
                    for _ in 0..4 {
                        let dir = (dir0 * 0.6 + up + self.rng.dir() * 0.7).normalize();
                        out.push(Particle { pos: at.into(), vel: (dir * self.rng.range(4.0, 22.0)).into(), life: self.rng.range(0.3, 1.1), size0: 0.08, size1: 0.03, kind: SPARK, drag: 0.6, seed: self.rng.f(), alpha: 0.8, ..Default::default() });
                    }
                    let n = (c.slide / 12.0).min(3.0);
                    let mut k = n;
                    while k > 0.0 && self.rng.f() < k {
                        k -= 1.0;
                        let r = &mut self.rng;
                        let vel = tang * r.range(0.3, 0.8) + r.dir() * (2.0 + c.slide * 0.15) + up * r.range(2.0, 6.0);
                        out.push(Particle { pos: at.into(), vel: vel.into(), life: r.range(3.0, 7.0), size0: r.range(1.0, 2.0), size1: r.range(5.0, 11.0), kind: DUST, drag: 1.2, buoy: 0.2, seed: r.f(), alpha: r.range(0.45, 0.7), ..Default::default() });
                    }
                }
                // Burning fuel left behind along its path.
                if c.slide > 1.0 && d.burn > 5.0 && d.group != Some(airframe::G_PROP) && d.mass > 3000.0 {
                    let fp = lp - up * up.dot(lp - (c.at - site).as_vec3());
                    let gap = 9.0 * self.size.sqrt();
                    if (fp - d.skid).length() > gap && self.fires.len() < 40 {
                        d.skid = fp;
                        self.fires.push(Fire { pos: fp, life: self.rng.range(20.0, 55.0), size: self.rng.range(0.8, 1.8) * self.size.sqrt(), acc: 0.0 });
                        if self.scorch.len() < 8 && n_scorch < 8 {
                            self.scorch.push((fp, self.rng.range(3.5, 6.5) * self.size.sqrt()));
                        }
                    }
                }
            } else if !d.resting {
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
                        if big && d.group != Some(airframe::G_PROP) && d.burn > 5.0 {
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
                // (a heavy section burns all along itself, in bigger flames)
                let (sc, spread) = if d.rigid { (self.size.sqrt(), d.half) } else { (1.0, Vec3::splat(d.radius as f32 * 0.8)) };
                d.emit_acc += rate * dtf * sc;
                let lp = (d.pos - site).as_vec3();
                let lv = d.vel.as_vec3();
                if d.rigid && self.domain.as_ref().is_some_and(|dm| dm.contains(lp, 3.0)) {
                    // A big burning section: fire along its length.
                    d.emit_acc = 0.0;
                    let k = 0.4 + (d.burn / 8.0).min(1.0);
                    let hf = d.half;
                    let (ax, len, mid) = if hf.z >= hf.x && hf.z >= hf.y { (Vec3::Z, hf.z, hf.x.max(hf.y)) } else if hf.x >= hf.y { (Vec3::X, hf.x, hf.z.max(hf.y)) } else { (Vec3::Y, hf.y, hf.x.max(hf.z)) };
                    let rad = (mid * 1.3).clamp(1.6, 9.0);
                    let along = d.rot.as_quat() * ax;
                    let n = if len > 2.5 * mid { 3 } else { 1 };
                    for j in 0..n {
                        let off = if n == 1 { 0.0 } else { (j as f32 - 1.0) * len * 0.65 };
                        self.fluid.push(FluidSrc { pos: lp + along * off, radius: rad, vel: lv, radial: 0.5, fuel: 1.6 * k, soot: 0.4 * k, dust: 0.0, temp: 1.3, vel_k: 5.0, temp_k: 8.0, flicker: 0.7, expand: 0.0, turb: 0.0 });
                    }
                } else if self.domain.as_ref().is_some_and(|dm| dm.contains(lp, 3.0)) {
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
                    let s = if big { sc } else { 0.45 };
                    let at = if d.rigid { d.rot.as_quat() * (Vec3::new(r.range(-1.0, 1.0), r.range(-1.0, 1.0), r.range(-1.0, 1.0)) * spread) } else { r.dir() * d.radius as f32 * 0.8 };
                    let hot = r.f() < if big { 0.45 } else { 0.03 };
                    out.push(Particle {
                        pos: (lp + at - lv * dtf * r.f()).into(),
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
        // An airliner's fire is so much bigger that at a small one's soot
        // per volume its smoke is a wall of black: thinner here.
        if self.heavy {
            for (i, f) in self.fluid.iter_mut().enumerate() {
                f.soot *= if i < n_burst { 0.5 } else { 0.35 };
            }
        }

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
            let ss = self.size.sqrt();
            let n = rate.clamp(12.0, 40.0 * ss);
            let faint = (rate / n).min(1.0);
            self.column_acc += n * dtf;
            let wide = (rate.sqrt() * 2.0).clamp(3.0, 12.0 * self.size);
            while self.column_acc >= 1.0 {
                self.column_acc -= 1.0;
                let r = &mut self.rng;
                let d = r.dir();
                out.push(Particle {
                    pos: (at + (d - up * d.dot(up)) * wide * r.f().sqrt()).into(),
                    vel: (up * rise.clamp(2.0, 12.0) * r.range(0.7, 1.1) + r.dir() * 1.0).into(),
                    life: r.range(14.0, 26.0),
                    size0: r.range(4.0, 6.5) * ss,
                    size1: r.range(18.0, 32.0) * self.power.sqrt() * ss,
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

    /// Where most of a heavy wreck is (None for a light aircraft).
    pub fn wreck_centre(&self) -> Option<DVec3> {
        if !self.heavy {
            return None;
        }
        let (mut sum, mut m) = (DVec3::ZERO, 0.0);
        for d in self.debris.iter().filter(|d| d.rigid && !d.sunk) {
            sum += d.pos * d.mass as f64;
            m += d.mass as f64;
        }
        (m > 0.0).then(|| sum / m)
    }

    /// Brightness of the fire light: a hard flash per blast over a flickering glow.
    pub fn light(&self) -> f32 {
        let (t, p, _) = self.last_blast;
        let flash = p * 9.0 * (-(t as f32) / 0.3).exp() + p * 5.0 * (-(t as f32) / 1.8).exp();
        let burning = self.debris.iter().filter(|d| d.burn > 0.0 && !d.sunk && d.group.is_some()).count() as f32;
        let glow = (burning * 1.3 + self.fires.len().min(12) as f32 * 0.5) * (0.8 + 0.2 * ((self.time * 23.0).sin() * (self.time * 7.1).cos()) as f32);
        flash + glow
    }
}


/// The bounds (body frame) of what a break keeps of a group's mesh, and
/// the share of the mesh that is.
fn kept_box(m: &Mesh, tear: &[Tear; 2]) -> Option<(Vec3, Vec3, f32)> {
    let step = (m.v.len() / 6000).max(1);
    let (mut lo, mut hi, mut n, mut all) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN), 0usize, 0usize);
    for v in m.v.iter().step_by(step) {
        all += 1;
        let q = Vec3::from(v.pos);
        if tear.iter().all(|t| t.keeps(q)) {
            lo = lo.min(q);
            hi = hi.max(q);
            n += 1;
        }
    }
    (n > 8).then(|| (lo, hi, n as f32 / all as f32))
}

/// What a heavy piece ran into in one step.
#[derive(Default)]
struct Contact {
    /// sliding speed where it touches the ground (0: in the air), and where
    slide: f32,
    at: DVec3,
    water: bool,
    /// a building it went into below its roof, and where
    build: Option<(BHit, DVec3)>,
}

/// One step of a heavy piece as a rigid box: gravity, then the corners (and
/// points along its length) that are in the ground push it back out with
/// an impulse, and rub with friction, so it slides, digs in and tumbles on
/// its own. Corners that go into a building's walls are reported instead.
fn rigid_step(d: &mut Debris, dt: f64, world: &World, ignore: &[(geo::TileId, usize)]) -> Contact {
    let mut c = Contact::default();
    let n_sub = ((dt / (1.0 / 120.0)).ceil() as usize).clamp(1, 4);
    let h = dt / n_sub as f64;
    let hf = d.half.as_dvec3();
    // The long axis gets points along it too: a fuselage lying across a
    // hump rests on the hump, not on its ends.
    let long = if hf.x >= hf.y && hf.x >= hf.z { 0 } else if hf.y >= hf.z { 1 } else { 2 };
    let mut pts = [DVec3::ZERO; 12];
    let mut k = 0;
    for t in [-1.0, 0.0, 1.0] {
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let mut q = [0.0; 3];
            q[long] = t;
            q[(long + 1) % 3] = a;
            q[(long + 2) % 3] = b;
            pts[k] = DVec3::new(q[0], q[1], q[2]) * hf;
            k += 1;
        }
    }
    let m_inv = 1.0 / d.mass.max(1.0) as f64;
    for _ in 0..n_sub {
        let upw = d.pos.normalize();
        d.vel -= upw * 9.81 * h;
        d.vel *= 1.0 - 0.01 * h;
        d.pos += d.vel * h;
        let w = d.omega * h;
        if w.length_squared() > 1e-14 {
            d.rot = (DQuat::from_scaled_axis(w) * d.rot).normalize();
        }
        let rot = d.rot;
        let iw = |v: DVec3| rot * (d.inv_i * (rot.inverse() * v));
        let mut deepest = 0.0f64;
        let mut touching = false;
        for &pb in &pts {
            let r = rot * pb;
            let p = d.pos + r;
            let (la, lo, ph) = geo::pos_to_lla(p);
            let Some(g) = world.ground(la, lo) else { continue };
            let mut floor = g.h;
            if let Some(b) = world.building(la, lo).filter(|b| !b.bridge && !ignore.contains(&(b.tile, b.idx))) {
                if ph < b.top && ph > b.base - 3.0 {
                    if ph > b.top - 1.5 {
                        floor = b.top;
                    } else {
                        if c.build.is_none() {
                            c.build = Some((b, p));
                        }
                        continue;
                    }
                }
            } else if ph < g.h - 1.5 {
                // Under the 3D tiles the buildings are the ground's own top.
                if let Some(b) = world.photo_building(la, lo, g.h).filter(|b| !ignore.contains(&(b.tile, b.idx))) {
                    if c.build.is_none() {
                        c.build = Some((b, p));
                    }
                    continue;
                }
            }
            let pen = floor - ph;
            if pen <= 0.0 {
                continue;
            }
            if g.water && floor == g.h {
                c.water = true;
                return c;
            }
            touching = true;
            deepest = deepest.max(pen);
            // The ground pushes back at this corner: no bounce to speak of,
            // and friction along it.
            let vc = d.vel + d.omega.cross(r);
            let vn = vc.dot(upw);
            if vn >= 0.0 {
                continue;
            }
            let rn = r.cross(upw);
            let kn = m_inv + upw.dot(iw(rn).cross(r));
            let j = -(1.1 * vn) / kn;
            d.vel += upw * (j * m_inv);
            d.omega += iw(rn * j);
            let vc = d.vel + d.omega.cross(r);
            let vt = vc - upw * vc.dot(upw);
            let sl = vt.length();
            if sl > 1e-4 {
                let t = vt / sl;
                let rt = r.cross(t);
                let kt = m_inv + t.dot(iw(rt).cross(r));
                let jt = (0.7 * j).min(sl / kt);
                d.vel -= t * (jt * m_inv);
                d.omega -= iw(rt * jt);
            }
            if sl as f32 > c.slide {
                c.slide = sl as f32;
                c.at = p;
            }
        }
        if touching {
            d.pos += upw * deepest.min(1.5) * 0.6;
            // Crumpling and scraping soak up its spin.
            d.omega *= 1.0 - (0.6 * h).min(0.5);
        }
        if touching && d.vel.length() < 0.4 && d.omega.length() < 0.25 {
            d.still += h as f32;
            if d.still > 0.6 {
                d.resting = true;
                d.vel = DVec3::ZERO;
                d.omega = DVec3::ZERO;
                break;
            }
        } else {
            d.still = 0.0;
        }
    }
    c
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
