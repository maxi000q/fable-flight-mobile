//! Life at an airport: aircraft at the stands with their ground equipment,
//! and others pushing back, taxiing out along the real taxiways, holding
//! short, taking off, and arriving down the approach to land, roll out,
//! turn off and taxi in to a free stand. The runway in use follows the wind.
//! Movement is scripted along the taxi graph, not flown.
use crate::airfield::{Light, L_GREEN, L_RED, L_STROBE, L_WHITE};
use crate::airmodels::{self as am, TYPES};
use crate::airports::{hold_dist, right_of, Geo};
use crate::render::Inst;
use glam::{DVec3, Vec2, Vec3};

/// Fuselage and fin colours: generic, made-up airlines.
pub const BODY: [[f32; 3]; 8] = [[0.86, 0.86, 0.86], [0.86, 0.86, 0.86], [0.86, 0.86, 0.86], [0.8, 0.82, 0.84], [0.88, 0.86, 0.8], [0.1, 0.18, 0.42], [0.86, 0.86, 0.86], [0.55, 0.58, 0.62]];
pub const FIN: [[f32; 3]; 12] = [[0.08, 0.14, 0.42], [0.62, 0.05, 0.06], [0.12, 0.42, 0.72], [0.06, 0.38, 0.2], [0.86, 0.38, 0.04], [0.32, 0.1, 0.42], [0.04, 0.42, 0.44], [0.86, 0.66, 0.04], [0.42, 0.06, 0.1], [0.3, 0.32, 0.36], [0.02, 0.02, 0.03], [0.86, 0.86, 0.86]];

struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.f() * n as f32) as usize).min(n - 1)
    }
}

pub struct Parked {
    pub stand: usize,
    pub ty: u8,
    pub body: u8,
    pub fin: u8,
    /// when it will leave (seconds of traffic time), if it will
    pub leave: f32,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Phase {
    Push,
    TaxiOut,
    Roll,
    Climb,
    Final,
    Rollout,
    TaxiIn,
}

struct Path {
    pts: Vec<Vec2>,
    cum: Vec<f32>,
}
impl Path {
    fn new(mut pts: Vec<Vec2>) -> Self {
        pts.dedup_by(|a, b| (*a - *b).length() < 0.3);
        if pts.len() == 1 {
            pts.push(pts[0] + Vec2::X * 0.1);
        }
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum[cum.len() - 1] + (w[1] - w[0]).length());
        }
        Self { pts, cum }
    }
    fn len(&self) -> f32 {
        self.cum[self.cum.len() - 1]
    }
    fn at(&self, s: f32) -> Vec2 {
        let s = s.clamp(0.0, self.len());
        let i = match self.cum.binary_search_by(|c| c.total_cmp(&s)) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
        .min(self.pts.len() - 2);
        let l = (self.cum[i + 1] - self.cum[i]).max(1e-4);
        self.pts[i] + (self.pts[i + 1] - self.pts[i]) * ((s - self.cum[i]) / l)
    }
    /// How sharply the path turns over the next `ahead` metres (0 straight .. 2 reversing).
    fn bend(&self, s: f32, ahead: f32) -> f32 {
        let d0 = (self.at(s + 2.0) - self.at(s)).normalize_or_zero();
        let mut worst = 0.0f32;
        let mut k = 8.0;
        while k <= ahead {
            let d = (self.at(s + k + 2.0) - self.at(s + k)).normalize_or_zero();
            worst = worst.max(1.0 - d0.dot(d));
            k += 8.0;
        }
        worst
    }
}

pub struct Mover {
    pub ty: u8,
    body: u8,
    fin: u8,
    pub phase: Phase,
    path: Path,
    s: f32,
    pub v: f32,
    pub pos: Vec2,
    pub hdg: Vec2,
    /// height above the reference ground (threshold) when flying, else 0
    pub alt: f32,
    pitch: f32,
    gear: bool,
    stand: usize,
    rwy: (usize, usize),
    /// where along the path to stop until the runway is free
    hold_s: f32,
    /// where along the path the runway starts (lined up from here on)
    entry_s: f32,
    wait: f32,
    /// how long it has been stuck behind someone
    stuck: f32,
    ref_h: f32,
    gone: bool,
}

pub struct Traffic {
    pub parked: Vec<Parked>,
    /// stand taken (parked there or someone on the way to it)
    taken: Vec<bool>,
    pub movers: Vec<Mover>,
    rng: Rng,
    t: f32,
    next_dep: f32,
    next_arr: f32,
    /// runway and end in use
    pub active: Option<(usize, usize)>,
    /// who has the runway: index into `movers`, for this frame
    lock: Option<usize>,
}

/// Where the aircraft's origin (between its main wheels) is when parked at a stand.
fn parked_at(g: &Geo, si: usize, ty: u8) -> (Vec2, Vec2) {
    let s = &g.stands[si];
    (s.pos - s.dir * TYPES[ty as usize].nose, s.dir)
}

impl Traffic {
    pub fn new(g: &Geo, seed: u32) -> Self {
        let mut rng = Rng(seed | 1);
        let apt = g.apt();
        let mut parked = Vec::new();
        let mut taken = vec![false; g.stands.len()];
        for (i, s) in g.stands.iter().enumerate() {
            let fill = if s.terminal { 0.78 } else if apt.kind == 0 { 0.5 } else { 0.6 };
            if rng.f() < fill {
                let ty = Self::type_for(&mut rng, s.size, apt.kind).min(Self::biggest(g));
                parked.push(Parked { stand: i, ty, body: rng.pick(BODY.len()) as u8, fin: rng.pick(FIN.len()) as u8, leave: if parked.len() < 2 { 0.0 } else { 20.0 + rng.f() * 700.0 } });
                taken[i] = true;
            }
        }
        let next_arr = 2.0 + rng.f() * 10.0;
        Self { parked, taken, movers: Vec::new(), rng, t: 0.0, next_dep: 1.0, next_arr, active: None, lock: None }
    }

    /// The biggest type the longest paved runway can take.
    fn biggest(g: &Geo) -> u8 {
        let len = g.runways.iter().filter(|r| r.paved).map(|r| r.len).fold(0.0, f32::max);
        RWY_NEED.iter().rposition(|need| len >= *need).unwrap_or(0) as u8
    }

    /// A type of aircraft for a stand that takes up to `size`.
    fn type_for(rng: &mut Rng, size: u8, kind: u8) -> u8 {
        match size {
            0 => 0,
            1 => if rng.f() < 0.5 { 1 } else { 0 },
            2 => if kind == 0 { 0 } else if rng.f() < 0.75 { 2 } else { 1 },
            s => s,
        }
    }

    /// How many aircraft are moving at once at most.
    fn busy(&self, g: &Geo, activity: f32) -> usize {
        let base = [1.0, 3.0, 7.0][g.apt().kind as usize];
        ((base * activity).round() as usize).max(1)
    }

    /// Move everything on by `dt`. `player`: the player's aircraft (local
    /// position, height above the field, ground speed), to keep out of its way.
    pub fn step(&mut self, g: &Geo, dt: f32, wind_from: f32, wind: f32, activity: f32, player: Option<(Vec2, f32)>) {
        if dt <= 0.0 || g.runways.is_empty() {
            return;
        }
        self.t += dt;
        let want = g.active(wind_from, wind);
        if want != self.active {
            // A change of runway: wait until it is quiet.
            if self.active.is_none() || !self.movers.iter().any(|m| matches!(m.phase, Phase::Roll | Phase::Final | Phase::Rollout)) {
                self.active = want;
            }
        }
        let Some((ri, re)) = self.active else { return };
        let r = &g.runways[ri];
        let (thr, rdir) = r.end(re);
        // Is the runway free? The player rolling or landing on it counts.
        let player_on = player.is_some_and(|(p, h)| {
            let (s, x) = r.local(p);
            (h < 40.0 && s > -400.0 && s < r.len + 100.0 && x.abs() < r.width * 0.5 + 25.0) || {
                let d = (thr - p).dot(rdir);
                d > 0.0 && d < 7000.0 && (p - (thr - rdir * d)).length() < 400.0 && h < d * 0.09 + 60.0
            }
        });
        self.lock = self.movers.iter().position(|m| matches!(m.phase, Phase::Roll | Phase::Rollout) || (m.phase == Phase::Climb && m.alt < 120.0) || (m.phase == Phase::TaxiOut && m.s > m.hold_s + 1.0 && m.s < m.entry_s + 400.0));
        let arriving_soon = self.movers.iter().any(|m| m.phase == Phase::Final && (thr - m.pos).dot(rdir) < 6500.0);
        let runway_free = self.lock.is_none() && !arriving_soon && !player_on;

        // New traffic.
        let moving = self.movers.len();
        let cap = self.busy(g, activity);
        if r.paved || g.apt().kind == 0 {
            if self.t > self.next_dep && moving < cap {
                self.next_dep = self.t + (60.0 + self.rng.f() * 120.0) / activity.max(0.2) / [1.0, 1.5, 3.0][g.apt().kind as usize];
                self.depart(g);
            }
            if self.t > self.next_arr && moving < cap {
                self.next_arr = self.t + (70.0 + self.rng.f() * 120.0) / activity.max(0.2) / [1.0, 1.5, 3.0][g.apt().kind as usize];
                self.arrive(g);
            }
        }
        // Everyone due to leave a stand for a while starts their engines.
        let _ = &self.parked;

        let n = self.movers.len();
        let snapshot: Vec<(Vec2, Vec2, f32, bool)> = self.movers.iter().map(|m| (m.pos, m.hdg, TYPES[m.ty as usize].len, m.alt < 1.0)).collect();
        for i in 0..n {
            let has_lock = self.lock == Some(i);
            let m = &mut self.movers[i];
            let ty = TYPES[m.ty as usize];
            match m.phase {
                Phase::Push => {
                    m.v = 1.2;
                    m.s += m.v * dt;
                    // Pushed tail first: the nose points back along the way it is going.
                    let c = (ty.len * 0.25).clamp(3.0, 12.0).min(m.s).max(0.5);
                    m.pos = (m.path.at(m.s - c) + m.path.at(m.s + c)) * 0.5;
                    let h = m.path.at(m.s - c) - m.path.at(m.s + c);
                    if h.length() > 0.5 {
                        m.hdg = h.normalize();
                    }
                    if m.s >= m.path.len() {
                        m.wait += dt;
                        if m.wait > 6.0 {
                            if let Some(p) = Self::taxi_out_path(g, m.pos, m.hdg, (ri, re), m.stand) {
                                m.path = p.0;
                                m.hold_s = p.1;
                                m.entry_s = p.2;
                                m.s = 0.0;
                                m.wait = 0.0;
                                m.phase = Phase::TaxiOut;
                            } else {
                                m.gone = true;
                            }
                        }
                    }
                }
                Phase::TaxiOut | Phase::TaxiIn => {
                    // Taxi speed: slower for the big ones, in turns, and toward a stop.
                    let mut vmax: f32 = if m.ty >= 4 { 11.0 } else { 13.0 };
                    let bend = m.path.bend(m.s, 60.0);
                    vmax = vmax.min(13.0 - bend * 12.0).max(3.5);
                    let mut stop_at = m.path.len();
                    if m.phase == Phase::TaxiOut && m.s < m.hold_s + 0.5 && !(has_lock || (runway_free && self.lock.is_none())) {
                        stop_at = m.hold_s;
                    }
                    if m.phase == Phase::TaxiOut && m.s >= m.hold_s - 0.5 && self.lock.is_none() && runway_free {
                        self.lock = Some(i);
                    }
                    if m.phase == Phase::TaxiOut {
                        stop_at = stop_at.min(m.entry_s);
                    }
                    let left = stop_at - m.s;
                    vmax = vmax.min((2.0 * 1.0 * left.max(0.0)).sqrt());
                    // Keep clear of whoever is ahead on the ground.
                    let mut blocked = false;
                    for (j, (p, _, len, ground)) in snapshot.iter().enumerate() {
                        if j == i || !ground {
                            continue;
                        }
                        let d = *p - m.pos;
                        let ahead = d.dot(m.hdg);
                        if ahead > 0.0 && ahead < (len + ty.len) * 0.6 + 25.0 && d.perp_dot(m.hdg).abs() < (ty.span * 0.6).max(14.0) {
                            blocked = true;
                        }
                    }
                    if let Some((p, h)) = player {
                        let d = p - m.pos;
                        let ahead = d.dot(m.hdg);
                        if h < 8.0 && ahead > 0.0 && ahead < ty.len * 0.6 + 45.0 && d.perp_dot(m.hdg).abs() < ty.span * 0.6 + 12.0 {
                            blocked = true;
                        }
                    }
                    if blocked && m.stuck < 45.0 {
                        m.stuck += dt;
                        vmax = 0.0;
                    } else if !blocked {
                        m.stuck = (m.stuck - dt).max(0.0);
                    }
                    m.v = if m.v < vmax { (m.v + 0.8 * dt).min(vmax) } else { (m.v - 2.0 * dt).max(vmax) };
                    m.s += m.v * dt;
                    // Corners are cut by following the middle of a stretch of
                    // path (shorter at its start, so nothing jumps).
                    let c = (ty.len * 0.25).clamp(3.0, 12.0).min(m.s.max(0.5));
                    m.pos = (m.path.at(m.s - c) + m.path.at(m.s + c)) * 0.5;
                    let h = m.path.at(m.s + c) - m.path.at(m.s - c);
                    if h.length() > 0.5 {
                        m.hdg = h.normalize();
                    }
                    if m.phase == Phase::TaxiOut && m.s >= m.entry_s - 0.5 && m.v < 4.0 {
                        // Lined up: the take-off run.
                        m.phase = Phase::Roll;
                        m.pos = m.path.at(m.entry_s);
                        m.hdg = rdir;
                        self.lock = Some(i);
                    }
                    if m.phase == Phase::TaxiIn && m.s >= m.path.len() - 0.3 {
                        m.gone = true;
                        let (_, dir) = parked_at(g, m.stand, m.ty);
                        let _ = dir;
                        self.parked.push(Parked { stand: m.stand, ty: m.ty, body: m.body, fin: m.fin, leave: self.t + 300.0 + self.rng.f() * 900.0 });
                    }
                }
                Phase::Roll => {
                    m.v += ty.accel * dt * (1.0 - m.v / (ty.v_rot * 1.6));
                    m.pos += m.hdg * m.v * dt;
                    if m.v > ty.v_rot {
                        m.phase = Phase::Climb;
                    }
                    m.ref_h = g.h(m.pos);
                }
                Phase::Climb => {
                    m.v = (m.v + 1.2 * dt).min(ty.v_rot * 1.45);
                    let target = if m.ty == 0 { 8.0f32 } else { 11.0 };
                    m.pitch = (m.pitch + 3.0 * dt).min(target);
                    let climb = m.v * (m.pitch.to_radians() * 0.62).sin();
                    m.alt += climb * dt;
                    m.pos += m.hdg * m.v * dt;
                    if m.alt > 90.0 {
                        m.gear = false;
                    }
                    if m.alt > 1500.0 || (m.pos - thr).length() > 16_000.0 {
                        m.gone = true;
                    }
                }
                Phase::Final => {
                    let aim = thr + rdir * aim_dist(r.len);
                    let d = (aim - m.pos).dot(rdir);
                    let glide = d.max(0.0) * 3f32.to_radians().tan();
                    // The flare: round out over the last few metres.
                    let h = if glide < 9.0 { 9.0 * (glide / 9.0).powf(1.7) } else { glide };
                    m.alt = h.max(0.0);
                    m.v = if d > 3000.0 { ty.v_app * 1.15 } else { ty.v_app };
                    m.pos += m.hdg * m.v * dt;
                    m.pitch = if glide < 9.0 { 2.0 + (9.0 - glide) * 0.4 } else { 2.5 };
                    if d < -120.0 || (glide < 0.05 && d < 60.0) {
                        m.alt = 0.0;
                        m.phase = Phase::Rollout;
                    }
                }
                Phase::Rollout => {
                    m.pitch = (m.pitch - 2.5 * dt).max(0.0);
                    m.v = (m.v - 2.0 * dt).max(13.0);
                    m.pos += m.hdg * m.v * dt;
                    let (s_now, _) = r.local(m.pos);
                    let along = if re == 0 { s_now } else { r.len - s_now };
                    if m.v <= 14.0 || along > r.len - 350.0 {
                        // Turn off at the next exit, and on to a stand.
                        match Self::taxi_in_path(g, m.pos, m.hdg, ri, m.stand, m.ty) {
                            Some(p) => {
                                m.path = p;
                                m.s = 0.0;
                                m.phase = Phase::TaxiIn;
                            }
                            None if along > r.len - 60.0 => m.gone = true,
                            None => {}
                        }
                    }
                }
            }
            if m.alt <= 0.01 && !matches!(m.phase, Phase::Climb | Phase::Final) {
                m.ref_h = g.h(m.pos);
            }
            if !m.gone && matches!(m.phase, Phase::TaxiIn) && m.wait == 0.0 {
                // The runway is free once it is past the hold line on the way in.
                let (_, x) = r.local(m.pos);
                if x.abs() > hold_dist(r) {
                    m.wait = 1.0;
                }
            }
        }
        // Departures: aircraft that have been at their stands long enough.
        let mut gone_stands = Vec::new();
        self.movers.retain(|m| {
            if m.gone && m.phase != Phase::TaxiIn {
                gone_stands.push(m.stand);
            }
            !m.gone
        });
        for s in gone_stands {
            if s < self.taken.len() && !self.parked.iter().any(|p| p.stand == s) {
                self.taken[s] = false;
            }
        }
    }

    /// Someone at a stand pushes back and taxis out.
    fn depart(&mut self, g: &Geo) {
        let t = self.t;
        let cands: Vec<usize> = (0..self.parked.len()).filter(|k| self.parked[*k].leave < t).collect();
        if cands.is_empty() {
            return;
        }
        let k = cands[self.rng.pick(cands.len())];
        let p = self.parked.swap_remove(k);
        let (pos, dir) = parked_at(g, p.stand, p.ty);
        let ty = TYPES[p.ty as usize];
        let st = &g.stands[p.stand];
        // Airliners are pushed back out of the gate; light aircraft just taxi.
        let (path, phase) = if st.terminal || p.ty >= 2 {
            (Path::new(Self::push_path(g, p.stand, pos, dir, ty.len * 0.9 + 10.0)), Phase::Push)
        } else {
            (Path::new(vec![pos, pos]), Phase::Push)
        };
        self.movers.push(Mover { ty: p.ty, body: p.body, fin: p.fin, phase, path, s: 0.0, v: 0.0, pos, hdg: dir, alt: 0.0, pitch: 0.0, gear: true, stand: p.stand, rwy: self.active.unwrap_or((0, 0)), hold_s: 0.0, entry_s: 0.0, wait: 0.0, stuck: 0.0, ref_h: g.h(pos), gone: false });
        // The stand stays taken until it has left (see `step`).
    }

    /// Back along the taxi lane that leads to a stand, as a tug pushes an
    /// airliner out: from `pos` (the aircraft's origin), `len` metres.
    fn push_path(g: &Geo, stand: usize, pos: Vec2, dir: Vec2, len: f32) -> Vec<Vec2> {
        let gr = &g.graph;
        let st = &g.stands[stand];
        let mut pts = vec![pos];
        let mut total = 0.0;
        let (mut cur, mut prev): (u32, Option<u32>) = (st.node, None);
        let mut heading = -dir;
        for _ in 0..40 {
            let here = gr.nodes[cur as usize].p;
            let next = gr.nodes[cur as usize].edges.iter().copied().filter(|e| Some(*e) != prev && gr.edges[*e as usize].rwy.is_none()).max_by(|a, b| {
                let da = (gr.nodes[gr.other(*a, cur) as usize].p - here).normalize_or_zero().dot(heading);
                let db = (gr.nodes[gr.other(*b, cur) as usize].p - here).normalize_or_zero().dot(heading);
                da.total_cmp(&db)
            });
            let Some(e) = next else { break };
            let n = gr.other(e, cur);
            let p = gr.nodes[n as usize].p;
            heading = (p - here).normalize_or_zero();
            if (p - st.pos).length() > (st.pos - pos).length() + 1.0 {
                let last = pts[pts.len() - 1];
                let seg = (p - last).length();
                if total + seg >= len {
                    pts.push(last + (p - last) * ((len - total) / seg.max(1e-3)));
                    return pts;
                }
                total += seg;
                pts.push(p);
            }
            prev = Some(e);
            cur = n;
        }
        if pts.len() < 2 {
            pts.push(pos - dir * len);
        }
        pts
    }

    /// Someone appears on a long final.
    fn arrive(&mut self, g: &Geo) {
        let Some((ri, re)) = self.active else { return };
        let r = &g.runways[ri];
        if !r.paved && g.apt().kind > 0 {
            return;
        }
        let kind = g.apt().kind;
        // A free stand, and an aircraft that fits it.
        let free: Vec<usize> = (0..g.stands.len()).filter(|i| !self.taken[*i]).collect();
        if free.is_empty() {
            return;
        }
        let si = free[self.rng.pick(free.len())];
        let ty = Self::type_for(&mut self.rng, g.stands[si].size, kind);
        let t = TYPES[ty as usize];
        // Only what the runway can take.
        if r.len < RWY_NEED[ty as usize] {
            return;
        }
        self.taken[si] = true;
        let (thr, dir) = r.end(re);
        let d0 = if ty == 0 { 3500.0 } else { 11_000.0 };
        let pos = thr - dir * d0;
        let alt = (d0 + aim_dist(r.len)) * 3f32.to_radians().tan();
        self.movers.push(Mover { ty, body: self.rng.pick(BODY.len()) as u8, fin: self.rng.pick(FIN.len()) as u8, phase: Phase::Final, path: Path::new(vec![pos, pos]), s: 0.0, v: t.v_app, pos, hdg: dir, alt, pitch: 2.5, gear: true, stand: si, rwy: (ri, re), hold_s: 0.0, entry_s: 0.0, wait: 0.0, stuck: 0.0, ref_h: g.h(thr), gone: false });
    }

    /// From a stand to the runway's end: the path, where to hold short, and
    /// where the runway begins.
    fn taxi_out_path(g: &Geo, from: Vec2, hdg: Vec2, rwy: (usize, usize), stand: usize) -> Option<(Path, f32, f32)> {
        let gr = &g.graph;
        let r = &g.runways[rwy.0];
        let (thr, dir) = r.end(rwy.1);
        let start_end = if rwy.1 == 0 { r.a } else { r.b };
        // Join the graph ahead of the nose.
        let near = gr.nearest(from, true).or_else(|| gr.nearest(from, false))?;
        let _ = (stand, hdg);
        // The runway node nearest its start that a taxiway leads to.
        let entry = (0..gr.nodes.len() as u32)
            .filter(|n| {
                let nd = &gr.nodes[*n as usize];
                nd.edges.iter().any(|e| gr.edges[*e as usize].rwy == Some(rwy.0 as u16)) && nd.edges.iter().any(|e| gr.edges[*e as usize].rwy.is_none())
            })
            .min_by(|a, b| (gr.nodes[*a as usize].p - start_end).length().total_cmp(&(gr.nodes[*b as usize].p - start_end).length()))?;
        if (gr.nodes[entry as usize].p - start_end).length() > r.len * 0.45 {
            return None;
        }
        let nodes = gr.path(near, entry)?;
        let mut pts = vec![from];
        pts.extend(nodes.iter().map(|n| gr.nodes[*n as usize].p));
        let entry_p = gr.nodes[entry as usize].p;
        // Line up, then the whole runway ahead.
        let mut path = Path::new(pts);
        let entry_s = path.len();
        let far = if rwy.1 == 0 { r.b } else { r.a };
        let _ = thr;
        path = Path::new([path.pts.clone(), vec![entry_p + dir * 30.0, far]].concat());
        // Hold where the path first comes within the runway's protected strip.
        let dh = hold_dist(r);
        let mut hold_s = (entry_s - dh).max(0.0);
        let mut s = 0.0;
        while s < entry_s {
            let (a, x) = r.local(path.at(s));
            if x.abs() < dh && a > -200.0 && a < r.len + 200.0 {
                hold_s = (s - 3.0).max(0.0);
                break;
            }
            s += 2.0;
        }
        Some((path, hold_s, entry_s))
    }

    /// From where it is rolling out on the runway, off at the next exit and on to its stand.
    fn taxi_in_path(g: &Geo, from: Vec2, hdg: Vec2, ri: usize, stand: usize, ty: u8) -> Option<Path> {
        let gr = &g.graph;
        let r = &g.runways[ri];
        // Exits ahead: runway nodes a taxiway leaves from.
        let mut exits: Vec<(f32, u32)> = (0..gr.nodes.len() as u32)
            .filter_map(|n| {
                let nd = &gr.nodes[n as usize];
                let on = nd.edges.iter().any(|e| gr.edges[*e as usize].rwy == Some(ri as u16));
                let off = nd.edges.iter().any(|e| gr.edges[*e as usize].rwy.is_none());
                let ahead = (nd.p - from).dot(hdg);
                (on && off && ahead > 25.0 && r.local(nd.p).1.abs() < r.width).then_some((ahead, n))
            })
            .collect();
        exits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let st = g.stands.get(stand)?;
        for (_, n) in exits.into_iter().take(4) {
            let Some(nodes) = gr.path(n, st.node) else { continue };
            // Leaving by a taxiway, not straight on down the runway.
            if nodes.len() >= 2 {
                let e = gr.nodes[n as usize].edges.iter().find(|e| gr.other(**e, n) == nodes[1]);
                if e.is_some_and(|e| gr.edges[*e as usize].rwy.is_some()) {
                    continue;
                }
            }
            let mut pts = vec![from];
            pts.extend(nodes.iter().map(|k| gr.nodes[*k as usize].p));
            // Stop with the nose on the stand's mark.
            let nose = TYPES[ty as usize].nose;
            while pts.len() > 2 && (pts[pts.len() - 1] - st.pos).length() < nose + 2.0 {
                pts.pop();
            }
            pts.push(st.pos - st.dir * nose);
            return Some(Path::new(pts));
        }
        None
    }

    /// Instances (camera-relative) for every aircraft and piece of ground
    /// equipment, and the lights the moving ones show.
    pub fn draw(&self, g: &Geo, cam: DVec3, _t: f64, out: &mut Vec<(u32, Inst)>, lights: &mut Vec<Light>) {
        let push = |out: &mut Vec<(u32, Inst)>, mesh: u32, origin: DVec3, x: Vec3, y: Vec3, z: Vec3, tint: [f32; 3]| {
            let o = (origin - cam).as_vec3();
            out.push((mesh, Inst { m: [[x.x, y.x, z.x, o.x], [x.y, y.y, z.y, o.y], [x.z, y.z, z.z, o.z]], tint: [tint[0], tint[1], tint[2], 0.0], ext: [0.0; 4] }));
        };
        let up = g.dir3(Vec3::Z);
        let frame = |hdg: Vec2, pitch: f32| {
            let f = g.dir3(Vec3::new(hdg.x, hdg.y, 0.0));
            let (sp, cp) = pitch.to_radians().sin_cos();
            let fwd = f * cp + up * sp;
            let upv = up * cp - f * sp;
            (fwd.cross(upv), upv, -fwd)
        };
        let aircraft = |out: &mut Vec<(u32, Inst)>, ty: u8, body: u8, fin: u8, origin: DVec3, x: Vec3, y: Vec3, z: Vec3, gear: bool| {
            let base = am::M_BODY + 3 * ty as u32;
            push(out, base, origin, x, y, z, BODY[body as usize]);
            push(out, base + 1, origin, x, y, z, FIN[fin as usize]);
            if gear {
                push(out, base + 2, origin, x, y, z, [0.5; 3]);
            }
        };
        let far = (g.center - cam).length() > 14_000.0;
        for p in &self.parked {
            let (pos, dir) = parked_at(g, p.stand, p.ty);
            let origin = g.world(pos, g.h(pos) as f64);
            if far || (origin - cam).length() > 9000.0 {
                continue;
            }
            let (x, y, z) = frame(dir, 0.0);
            aircraft(out, p.ty, p.body, p.fin, origin, x, y, z, true);
            if p.ty >= 2 && (origin - cam).length() < 2500.0 {
                self.ground_crew(g, p, pos, dir, cam, out);
            }
        }
        for (i, m) in self.movers.iter().enumerate() {
            let ty = TYPES[m.ty as usize];
            let h = if m.alt > 0.0 || matches!(m.phase, Phase::Climb | Phase::Final) { m.ref_h + m.alt } else { g.h(m.pos) };
            let origin = g.world(m.pos, h as f64);
            let (x, y, z) = frame(m.hdg, m.pitch);
            let d = (origin - cam).length();
            if d > 30_000.0 {
                continue;
            }
            aircraft(out, m.ty, m.body, m.fin, origin, x, y, z, m.gear);
            if m.phase == Phase::Push && m.ty >= 2 && d < 3000.0 {
                // The tug pushing it, at the nose.
                let nose = m.pos + m.hdg * (ty.nose + 4.5);
                let o = g.world(nose, g.h(nose) as f64);
                let (tx, ty_, tz) = frame(-m.hdg, 0.0);
                push(out, am::M_PUSHBACK, o, tx, ty_, tz, [1.0; 3]);
            }
            // Lights: navigation, beacon, strobes, landing and taxi lights.
            let at = |p: Vec3| -> [f32; 3] { ((origin - cam).as_vec3() + x * p.x + y * p.y - z * p.z).into() };
            let fwd = -z;
            let mut l = |p: Vec3, inten: f32, col: [u8; 3], dir: Vec3, spread: f32, fx: [u8; 4]| {
                if lights.len() < crate::render::MAX_DYN_LIGHTS {
                    lights.push(Light { pos: at(p), inten, dir: dir.into(), spread, col: [col[0], col[1], col[2], 0], fx });
                }
            };
            let steady = [0, 0, 255, 1];
            l(Vec3::new(-ty.tip.x, ty.tip.y, -ty.tip.z), 40.0, L_RED, Vec3::ZERO, 0.0, steady);
            l(Vec3::new(ty.tip.x, ty.tip.y, -ty.tip.z), 40.0, L_GREEN, Vec3::ZERO, 0.0, steady);
            l(Vec3::new(0.0, ty.tail.y * 0.35, -ty.tail.z), 25.0, L_WHITE, Vec3::ZERO, 0.0, steady);
            let ph = ((i * 37) % 255) as u8;
            l(Vec3::new(0.0, ty.cy + ty.rf + 0.1, 0.0), 120.0, L_RED, Vec3::ZERO, 0.0, [12, ph, 25, 1]);
            l(Vec3::new(0.0, ty.cy - ty.rf - 0.1, 1.0), 120.0, L_RED, Vec3::ZERO, 0.0, [12, ph.wrapping_add(128), 25, 1]);
            let flying = matches!(m.phase, Phase::Roll | Phase::Climb | Phase::Final | Phase::Rollout);
            if flying {
                for sx in [-1.0f32, 1.0] {
                    l(Vec3::new(sx * ty.tip.x, ty.tip.y, -ty.tip.z), 1600.0, L_STROBE, Vec3::ZERO, 0.0, [11, ph.wrapping_add(60), 7, 1]);
                }
            }
            if flying && m.alt < 900.0 {
                for sx in [-1.0f32, 1.0] {
                    l(Vec3::new(sx * ty.lamp.x, ty.lamp.y, -ty.lamp.z), 6000.0, L_WHITE, fwd, 0.86, [0, 0, 255, 1]);
                }
            } else if matches!(m.phase, Phase::TaxiOut | Phase::TaxiIn) && m.v > 0.5 {
                l(Vec3::new(0.0, ty.lamp.y * 0.6, ty.nose * 0.7), 900.0, L_WHITE, fwd, 0.85, [0, 0, 255, 0]);
            }
        }
    }

    /// Baggage, fuel and the rest round an airliner at its stand.
    fn ground_crew(&self, g: &Geo, p: &Parked, pos: Vec2, dir: Vec2, cam: DVec3, out: &mut Vec<(u32, Inst)>) {
        let ty = TYPES[p.ty as usize];
        let right = right_of(dir);
        let seed = (p.stand as u32).wrapping_mul(2654435761);
        let h = |k: u32| ((seed ^ k.wrapping_mul(0x9e3779b9)) >> 8) as f32 / 16_777_216.0;
        let put = |out: &mut Vec<(u32, Inst)>, mesh: u32, at: Vec2, face: Vec2, tint: [f32; 3]| {
            let o = (g.world(at, g.h(at) as f64) - cam).as_vec3();
            let f = g.dir3(Vec3::new(face.x, face.y, 0.0));
            let u = g.dir3(Vec3::Z);
            let x = f.cross(u);
            let z = -f;
            out.push((mesh, Inst { m: [[x.x, u.x, z.x, o.x], [x.y, u.y, z.y, o.y], [x.z, u.z, z.z, o.z]], tint: [tint[0], tint[1], tint[2], 0.0], ext: [0.0; 4] }));
        };
        let white = [0.85, 0.85, 0.82];
        let yellow = [0.85, 0.62, 0.05];
        // Belt loader up to the aft hold, on the right side.
        let aft = pos - dir * (ty.len - ty.nose) * 0.45;
        put(out, am::M_LOADER, aft + right * (ty.rf + 4.2), -right, white);
        // A tug and its carts waiting beside it.
        let tug = aft + right * (ty.rf + 9.0) - dir * 3.0;
        put(out, am::M_TUG, tug, dir, if h(1) < 0.5 { white } else { yellow });
        for k in 0..(2 + (h(2) * 3.0) as i32) {
            put(out, am::M_CART, tug - dir * (4.6 + 3.6 * k as f32), dir, white);
        }
        // Forward hold too on the big ones.
        if p.ty >= 3 {
            let fwd = pos + dir * (ty.door - 4.0);
            put(out, am::M_LOADER, fwd + right * (ty.rf + 4.2), -right, white);
        }
        // Fuel at the right wing, sometimes.
        if h(3) < 0.45 {
            put(out, am::M_FUEL, pos + right * (ty.span * 0.3) + dir * 6.0, -dir, white);
        }
        // Stairs at the front door on stands without a bridge.
        if !g.stands[p.stand].terminal || p.ty < 3 {
            put(out, am::M_STAIRS, pos + dir * ty.door - right * (ty.rf + 2.8), right, white);
        }
        // Cones at the wing tips and nose.
        for c in [pos + dir * (ty.nose + 1.5), pos + right * (ty.span * 0.5 + 1.0), pos - right * (ty.span * 0.5 + 1.0)] {
            put(out, am::M_CONE, c, dir, [1.0; 3]);
        }
    }
}

/// Runway each type of aircraft needs (m).
const RWY_NEED: [f32; 5] = [350.0, 1200.0, 1500.0, 1750.0, 2400.0];

/// How far past the threshold an arrival aims (the painted aiming point).
pub fn aim_dist(len: f32) -> f32 {
    if len >= 2400.0 {
        400.0
    } else if len >= 1200.0 {
        300.0
    } else {
        (len * 0.22).max(120.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AIRPORT=KSFO cargo test --release traffic_runs -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn traffic_runs() {
        let cache = dirs::cache_dir().unwrap().join("fablegame");
        let src = crate::sources::Sources::new(crate::fetch::Fetcher::new(cache, false), String::new());
        for code in std::env::var("AIRPORT").unwrap_or("KSFO".into()).split(',') {
            let idx = crate::airports::Db::get().find(code).unwrap();
            let g = crate::airports::build_geo(&src, idx).unwrap();
            let mut t = Traffic::new(&g, idx.wrapping_mul(2654435761) ^ 0x5bd1e995);
            let (mut deps, mut arrs, mut parked_in) = (0, 0, 0);
            let mut seen_climb = std::collections::HashSet::new();
            let dt = 1.0 / 30.0;
            let t0 = std::time::Instant::now();
            for k in 0..(1200.0 / dt) as usize {
                let before: Vec<Phase> = t.movers.iter().map(|m| m.phase).collect();
                let np = t.parked.len();
                t.step(&g, dt, 280.0, 10.0, 1.0, None);
                for (i, m) in t.movers.iter().enumerate() {
                    if m.phase == Phase::Climb && before.get(i) == Some(&Phase::Roll) {
                        deps += 1;
                        seen_climb.insert(i);
                    }
                    if m.phase == Phase::Rollout && before.get(i) == Some(&Phase::Final) {
                        arrs += 1;
                    }
                }
                if t.parked.len() > np {
                    parked_in += t.parked.len() - np;
                }
                if k % (60.0 / dt) as usize == 0 {
                    let st: Vec<String> = t.movers.iter().map(|m| format!("{:?}({:.0},{:.0} v{:.0} s{:.0}/{:.0} stuck{:.0})", m.phase, m.pos.x, m.pos.y, m.v, m.s, m.path.len(), m.stuck)).collect();
                    println!("{code} t={:.0}s parked {} moving {}: {}", k as f32 * dt, t.parked.len(), t.movers.len(), st.join(" "));
                }
            }
            println!("{code}: {deps} take-offs, {arrs} landings, {parked_in} parked after landing, {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
