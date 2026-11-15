//! The flight deck of the 747-400, laid out from photographs of real ones
//! (JAL, KLM, Martinair, Lufthansa, Cathay and Air China ships on Wikimedia
//! Commons): the two-crew glass cockpit with its six 8-inch displays (PFD and
//! ND for each pilot, the two EICAS screens in the middle), the mode control
//! panel along the glareshield with an EFIS control panel at each end, the
//! three standby instruments, the gear lever, the control columns, the
//! pedestal with three CDUs, the four thrust levers, speedbrake and flap
//! levers, fuel control switches and the radio panels, the overhead panel,
//! side consoles with the tiller and oxygen masks, two observer seats and the
//! circuit breaker panels behind the pilots.
//!
//! Everything is painted Boeing's brown-grey with white legends (drawn in
//! plane.wgsl / deck747.wgsl from the `pat::LEGEND` quads placed here), the
//! knobs a cream plastic, the lining an off-white. What is shown on the
//! displays is drawn in the shader from the instrument readings.
use super::*;
use glam::{Mat3, Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// The floor (body y) and how far the lining sits inside the skin.
pub const FLOOR: f32 = 2.10;
const INSET: f32 = 0.10;
/// The lining's window openings are this much larger than the skin's: the
/// trim round each window. Keep in step with plane.wgsl (`surface`).
pub const TRIM: f32 = 0.03;

// ---------------------------------------------------------------- materials

const fn panel_col() -> Vec3 {
    Vec3::new(0.150, 0.124, 0.098)
}
const PANEL: Mat = mt(mat::PLASTIC, 0.150, 0.124, 0.098).pat(pat::PANEL);
const PANEL_DK: Mat = mt(mat::PLASTIC, 0.105, 0.088, 0.072).pat(pat::PANEL);
const BEZEL: Mat = mt(mat::PLASTIC, 0.135, 0.112, 0.090).pat(pat::PANEL);
const GLARE: Mat = mt(mat::FABRIC, 0.030, 0.028, 0.027).pat(pat::LINING);
const LINER: Mat = mt(mat::PLASTIC, 0.46, 0.43, 0.38).pat(pat::LINING);
const CARPET: Mat = mt(mat::FABRIC, 0.032, 0.034, 0.040).pat(pat::CARPET);
const CREAM: Mat = mt(mat::PLASTIC, 0.60, 0.56, 0.46);
const BTN: Mat = mt(mat::PLASTIC, 0.020, 0.020, 0.022);
const COLUMN: Mat = mt(mat::PLASTIC, 0.048, 0.037, 0.031);
const FRAME: Mat = mt(mat::PLASTIC, 0.05, 0.05, 0.052);
const FLEECE: Mat = mt(mat::FABRIC, 0.36, 0.34, 0.31).pat(pat::FLEECE);
const SEAT_SIDE: Mat = mt(mat::FABRIC, 0.05, 0.05, 0.055).pat(pat::SEAT);
const SEAT_SHELL: Mat = mt(mat::PLASTIC, 0.10, 0.095, 0.09);
const CB_BLACK: Mat = mt(mat::PLASTIC, 0.012, 0.012, 0.013);

/// Legend styles (see deck747.wgsl `legend`).
pub mod style {
    pub const TEXT: u32 = 0;
    pub const TEXT2: u32 = 1;
    pub const BUTTON: u32 = 2;
    pub const BUTTON_AMBER: u32 = 3;
    pub const BUTTON_ON: u32 = 4;
    pub const SCALE: u32 = 5;
    pub const LINES: u32 = 6;
    pub const PLACARD: u32 = 7;
    pub const KEY: u32 = 8;
    pub const RED_PLACARD: u32 = 9;
    pub const BUTTON_LIT: u32 = 10;
}

/// What the displays show (`mat::DISPLAY` extra); keep in step with deck747.wgsl.
pub mod screen {
    pub const PFD: f32 = 20.0;
    pub const ND: f32 = 21.0;
    pub const EICAS: f32 = 22.0;
    pub const EICAS2: f32 = 23.0;
    pub const CDU: f32 = 24.0;
    pub const CDU_C: f32 = 25.0;
    pub const RADIO: f32 = 26.0;
    pub const MCP_IAS: f32 = 27.0;
    pub const MCP_HDG: f32 = 28.0;
    pub const MCP_VS: f32 = 29.0;
    pub const MCP_ALT: f32 = 30.0;
    pub const XPDR: f32 = 31.0;
    pub const CLOCK: f32 = 32.0;
    pub const EFIS_MINS: f32 = 33.0;
}

// ---------------------------------------------------------------- strings

/// Every string a legend or a display can print, looked up by index. The
/// shader gets them as a table (`strings_wgsl`); index 0 means "made-up text".
pub const STRINGS: &[&str] = &[
    "", "IAS/MACH", "HDG", "ALT", "VERT SPD", "LNAV", "VNAV", "FL CH", "HOLD", "A/T ARM", "F/D", "APP", "LOC", "BANK LIMIT", "CMD", "DISENGAGE",
    "MINS", "BARO", "MAP", "PLN", "VOR", "RANGE", "WXR", "STA", "WPT", "ARPT", "DATA", "POS", "TERR", "FUEL CONTROL", "RUN", "CUTOFF",
    "STAB TRIM", "AUTO", "ALTN", "GEAR", "UP", "DN", "OFF", "AUTOBRAKES", "RTO", "DISARM", "MAX AUTO", "GND PROX", "FLAP", "OVRD", "SOURCE", "IRS",
    "FMC", "NAV", "ON", "ARMED", "EMER LIGHTS", "ENG START", "CON", "NORM", "FLT", "FUEL JETTISON", "NOZZLE", "X FEED", "MAIN", "PUMPS", "CTR", "STAB",
    "HYD", "DEMAND", "ELEC", "GEN CONT", "BUS TIE", "DRIVE DISC", "BATTERY", "STBY POWER", "APU", "EXT PWR", "ALIGN", "ATT", "NACELLE", "WING", "ANTI-ICE", "WINDOW HEAT",
    "WIPER", "LANDING", "OUTBD", "INBD", "RWY TURNOFF", "TAXI", "BEACON", "STROBE", "LOGO", "IND LTS", "TEST", "BRT", "PACKS", "TRIM AIR", "RECIRC", "AFT CARGO",
    "HI FLOW", "OUTFLOW", "CABIN ALT", "LDG ALT", "YAW DAMPER", "UPR", "LWR", "PASS OXYGEN", "SEAT BELTS", "NO SMOKING", "CAPT", "F/O", "PUSH", "TO/GA", "REV", "SPEED BRAKE",
    "FLAPS", "CARGO FIRE", "DISCH", "ARM", "FWD", "AFT", "ENG FIRE", "1", "2", "3", "4", "CHR", "ET", "RESET", "FLOOD", "PANEL",
    "DOME", "STORM", "GLARESHIELD", "INTERPHONE", "VHF L", "VHF C", "VHF R", "HF L", "HF R", "MIC", "PA", "SAT", "AUDIO", "XPDR", "STBY", "TA/RA",
    "TILT", "GAIN", "EVAC", "COMMAND", "PARK BRAKE", "BRAKE SOURCE", "PUSH HEADREST", "PULL", "LOCK OVRD", "NOSE/BODY", "SPD", "THR", "SEL", "V/S", "ALT HOLD", "HDG SEL",
    "VALVE", "FAULT", "IGN", "BLEED", "ISLN", "DUCT", "SMOKE EVAC", "CALL", "ATC", "EICAS", "ENG", "STAT", "ELEC/HYD", "FUEL", "ECS", "DRS",
    "CANC", "RCL", "EFIS", "DSPL", "CTL", "SYS", "CDU", "EIU", "UP 1 5 10 20", "25 30", "DOWN", "EPR", "N1", "EGT", "N2", "FF",
    "OIL P", "OIL T", "OIL Q", "VIB", "TAT", "SAT", "TOTAL FUEL", "GROSS WT", "KGS X 1000", "CAB ALT", "RATE", "GS", "TAS", "MAG", "VOR L", "VOR R",
    "CARPE", "DERBY", "SFO", "OAK", "NM", "SPD", "VNAV PTH", "VNAV SPD", "STD", "MCP SPD", "IN", "HPA", "FPV", "MTRS", "ACT RTE 1 LEGS", "CARPE",
    "2219.0Z", "35.0NM", "TFC", "PACK 2 OFF", "APU RUNNING", "DUCT PRESS", "MASTER", "WARNING", "CAUTION", "RA", "DH", "WINDSHEAR", "CMD L", "CMD C", "CMD R", "DISC",
    "TO", "CRZ", "CLB", "REF", "PROG", "EXEC", "INIT", "MENU", "LEGS", "DEP ARR", "FIX", "NAV RAD", "PREV PAGE", "NEXT PAGE", "CLR", "DEL",
    "25", "30",
    // The Hornet's cockpit (fa18/pit.rs, pit18.wgsl).
    "DANGER", "FIRE", "MASTER CAUTION", "EXTGH", "A/A", "A/G", "MASTER ARM", "EMERG JETT", "APU FIRE", "IR COOL", "HMD", "SPIN", "NIGHT", "CONT", "5", "6", "7", "8", "9", "0", "ENT", "I/P", "ADF", "VOL", "COMM 1", "COMM 2", "EMCON", "A/P", "IFF", "TCN", "ILS", "D/L", "BCN", "ON/OFF", "DAY", "BLK LVL", "W/B", "BAL", "AOA", "INS", "LOCK", "SHOOT", "MODE", "QTY", "ZONE", "INT", "CRS", "ECM", "LDG GEAR", "NOSE", "LEFT", "RIGHT", "HALF", "FULL", "EXT LT", "FCS", "COMM", "INTR LT", "SNSR", "KY-58", "DEFOG", "IDLE", "MIL", "MAX", "CANOPY", "EJECTION SEAT", "UPPER", "RWS", "TWS", "STT", "RSET", "SIL", "AZ", "EL", "OPR", "ADI", "HSI", "SA", "STORES", "SUPT", "BIT", "FPAS", "CHKLST", "UFC", "TAC", "WYPT", "SEQ1", "SCL", "DCLTR", "MK-82", "AIM-9X", "AIM-120", "GUN", "PRGM", "T/R", "RCV", "X", "Y", "71X", "M", "RPM", "TEMP", "NOZ", "OIL", "BINGO", "T", "I", "L", "R", "G", "N", "E", "S", "W", "DIST", "RNG", "40",
];

/// Index of a string in `STRINGS` (panics if it is missing: a typo in the layout).
pub fn sid(s: &str) -> u32 {
    STRINGS.iter().skip(1).position(|x| *x == s).map(|i| i as u32 + 1).unwrap_or_else(|| panic!("no legend string {s:?}"))
}

/// Stroke-font glyph index for a character (plane.wgsl `GSEG`).
pub fn glyph(c: char) -> u32 {
    match c {
        '0'..='9' => c as u32 - '0' as u32,
        'N' => 10,
        'E' => 11,
        'S' => 12,
        'W' => 13,
        '.' => 14,
        ' ' => 15,
        _ => {
            const REST: &str = "ABCDFGHIJKLMOPQRTUVXYZ/-+:°<>()[]*'#";
            REST.chars().position(|x| x == c).map(|i| 16 + i as u32).unwrap_or(15)
        }
    }
}

/// The string table as WGSL: 15 glyphs a string, 8 bits each, the length in the top byte.
pub fn strings_wgsl() -> String {
    let mut out = format!("var<private> STRS: array<vec4u, {}> = array<vec4u, {}>(\n", STRINGS.len(), STRINGS.len());
    for (k, s) in STRINGS.iter().enumerate() {
        let mut b = [15u32; 16];
        let n = s.chars().count().min(15);
        for (i, c) in s.chars().take(15).enumerate() {
            b[i] = glyph(c);
        }
        b[15] = n as u32;
        let w: Vec<String> = (0..4).map(|j| format!("{}u", b[j * 4] | b[j * 4 + 1] << 8 | b[j * 4 + 2] << 16 | b[j * 4 + 3] << 24)).collect();
        out += &format!("  vec4u({}){}\n", w.join(", "), if k + 1 < STRINGS.len() { "," } else { "" });
    }
    out += ");\n";
    // Names for the ones the displays print (the first of any repeats).
    let mut seen = std::collections::HashSet::new();
    for (k, s) in STRINGS.iter().enumerate().skip(1) {
        let name: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        if seen.insert(name.clone()) {
            out += &format!("const S_{name}: u32 = {k}u;\n");
        }
    }
    out
}

pub(crate) fn legend_mat(style: u32, s: u32, col: Vec3) -> Mat {
    mt(mat::PLASTIC, col.x, col.y, col.z).pat(pat::LEGEND).x((style + 16 * s) as f32)
}

// ---------------------------------------------------------------- shell geometry

/// The lining's section at station z.
fn lsec(z: f32) -> Sec {
    let mut s = sec(z);
    s.w -= INSET;
    s.top -= INSET;
    s.bot += INSET;
    s
}
/// Where a ray from (0, 0.3) at angle `th` from the vertical (toward +x) meets a section.
fn ray_pt(s: &Sec, th: f32) -> Vec2 {
    let dir = Vec2::new(th.sin(), th.cos());
    let o = Vec2::new(0.0, 0.3);
    let f = |p: Vec2| {
        let up = p.y > s.mid;
        let n = if up { s.nt } else { s.nb };
        let h = if up { s.top - s.mid } else { s.mid - s.bot };
        (p.x / s.w).abs().powf(n) + ((p.y - s.mid) / h).abs().powf(n)
    };
    let (mut a, mut b) = (0.0f32, 6.0f32);
    for _ in 0..40 {
        let m = (a + b) * 0.5;
        if f(o + dir * m) < 1.0 { a = m } else { b = m }
    }
    o + dir * ((a + b) * 0.5)
}
/// Half width of the lining at station z, height y.
pub fn lin_x(z: f32, y: f32) -> f32 {
    let s = lsec(z);
    if y >= s.top {
        return 0.0;
    }
    let (mut a, mut c) = (0.0f32, PI);
    for _ in 0..30 {
        let m = (a + c) * 0.5;
        if sec_pt(&s, m).y > y { a = m } else { c = m }
    }
    sec_pt(&s, (a + c) * 0.5).x
}
/// Height of the lining overhead at station z, lateral position x.
pub fn lin_top(z: f32, x: f32) -> f32 {
    let s = lsec(z);
    let x = x.abs().min(s.w * 0.999);
    let (mut a, mut c) = (0.0f32, FRAC_PI_2);
    for _ in 0..30 {
        let m = (a + c) * 0.5;
        if sec_pt(&s, m).x < x { a = m } else { c = m }
    }
    sec_pt(&s, (a + c) * 0.5).y
}
/// Clamp a point to stay `gap` inside the lining (at its own station).
fn inside(p: Vec3, gap: f32) -> Vec3 {
    let y = p.y.min(lin_top(p.z, p.x) - gap);
    let hx = (lin_x(p.z, y) - gap).max(0.0);
    Vec3::new(p.x.clamp(-hx, hx), y, p.z)
}

/// A window's outline in (distance from the nose, angle) grown by `e` (m), `n` points a corner.
fn outline(w: [f32; 4], e: f32, n: usize) -> Vec<(f32, f32)> {
    let c = Vec2::new((w[0] + w[1]) * 0.5, (w[2] + w[3]) * 0.5 * 3.4);
    let h = Vec2::new((w[1] - w[0]) * 0.5, (w[3] - w[2]) * 0.5 * 3.4);
    let r = 0.06;
    let k = h - Vec2::splat(r);
    let rr = r + e;
    let corners = [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
    let mut pts: Vec<Vec2> = Vec::new();
    for (q, (sx, sy)) in corners.iter().enumerate() {
        let cc = c + Vec2::new(sx * k.x, sy * k.y);
        for i in 0..=n {
            let a = (q as f32 + i as f32 / n as f32) * FRAC_PI_2;
            pts.push(cc + Vec2::new(a.cos(), a.sin()) * rr);
        }
    }
    // Fill in the straight runs between the corners.
    let mut out = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        let m = ((b - a).length() / 0.04).ceil().max(1.0) as usize;
        for j in 0..m {
            let p = a.lerp(b, j as f32 / m as f32);
            out.push((p.x, p.y / 3.4));
        }
    }
    out
}

/// The lining's distance to its window cut-outs: measured on the skin
/// radially outside it, less the trim (as plane.wgsl does).
pub fn lining_window(q: Vec3) -> f32 {
    let r = Vec2::new(q.x, q.y - 0.3).normalize_or_zero();
    window(q + Vec3::new(r.x, r.y, 0.0) * INSET) - TRIM
}

/// The station where the skin (or the lining) at height y is x wide, on the nose.
fn station_at(x: f32, y: f32, lining: bool) -> f32 {
    let (mut a, mut b) = (bz(3.0), bz(5.6));
    for _ in 0..32 {
        let m = (a + b) * 0.5;
        let s = if lining { lsec(m) } else { sec(m) };
        let w = if y >= s.top { 0.0 } else {
            let (mut p, mut q) = (0.0f32, PI);
            for _ in 0..24 {
                let t = (p + q) * 0.5;
                if sec_pt(&s, t).y > y { p = t } else { q = t }
            }
            sec_pt(&s, (p + q) * 0.5).x
        };
        if w < x { a = m } else { b = m }
    }
    (a + b) * 0.5
}
/// Window k's opening grown by e, as points on the skin (or the lining): the
/// windshield (0) drawn in the front view, the side windows round their section.
fn window_ring(k: usize, e: f32, lining: bool) -> Vec<Vec3> {
    if k == 0 {
        let v: Vec<Vec2> = WSHIELD.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        let n = v.len();
        let area: f32 = (0..n).map(|i| v[i].perp_dot(v[(i + 1) % n])).sum();
        let out = |a: Vec2, b: Vec2| { let d = (b - a).normalize(); if area > 0.0 { Vec2::new(d.y, -d.x) } else { Vec2::new(-d.y, d.x) } };
        let r = 0.06 + e;
        let mut pts = Vec::new();
        for i in 0..n {
            let (pv, c, nx) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            let (n0, n1) = (out(pv, c), out(c, nx));
            let (a0, mut a1) = (n0.y.atan2(n0.x), n1.y.atan2(n1.x));
            if area > 0.0 { while a1 < a0 { a1 += TAU } } else { while a1 > a0 { a1 -= TAU } }
            for j in 0..=6 {
                let a = a0 + (a1 - a0) * j as f32 / 6.0;
                pts.push(c + Vec2::new(a.cos(), a.sin()) * r);
            }
            // The straight run to the next corner.
            let (s, t) = (c + n1 * r, nx + n1 * r);
            let m = ((t - s).length() / 0.04).ceil() as usize;
            for j in 1..m {
                pts.push(s.lerp(t, j as f32 / m as f32));
            }
        }
        // Past the aft limit (5.26 m from the nose) the cut is a station; clamp.
        return pts.iter().map(|p| { let z = station_at(p.x, p.y, lining).min(bz(5.26 + e)); Vec3::new(p.x, p.y, z) }).collect();
    }
    outline(WIN[k], e, 6)
        .iter()
        .map(|&(d, th)| {
            let z = bz(d);
            let p = ray_pt(&if lining { lsec(z) } else { sec(z) }, th);
            Vec3::new(p.x, p.y, z)
        })
        .collect()
}

// ---------------------------------------------------------------- a panel face and what sits on it

/// A flat face: origin, right and up axes; it faces `ax × ay` (toward the crew).
#[derive(Clone, Copy)]
pub(crate) struct Face {
    pub(crate) o: Vec3,
    pub(crate) ax: Vec3,
    pub(crate) ay: Vec3,
}
impl Face {
    pub(crate) fn new(o: Vec3, ax: Vec3, ay: Vec3) -> Face {
        let ax = ax.normalize();
        let ay = (ay - ax * ay.dot(ax)).normalize();
        Face { o, ax, ay }
    }
    pub(crate) fn n(&self) -> Vec3 {
        self.ax.cross(self.ay).normalize()
    }
    pub(crate) fn at(&self, u: f32, v: f32) -> Vec3 {
        self.o + self.ax * u + self.ay * v
    }
    pub(crate) fn at_h(&self, u: f32, v: f32, h: f32) -> Vec3 {
        self.at(u, v) + self.n() * h
    }
    pub(crate) fn rot(&self) -> Quat {
        Quat::from_mat3(&Mat3::from_cols(self.ax, self.ay, self.n()))
    }
    /// The same face moved to (u, v), raised by h.
    pub(crate) fn sub(&self, u: f32, v: f32, h: f32) -> Face {
        Face { o: self.at_h(u, v, h), ..*self }
    }
}

/// Quad with uv in units of its half height (u ±hx/hy, v ±1), carrying its aspect in `wear`.
pub(crate) fn rect(m: &mut Mesh, mm: &Mat, c: Vec3, ax: Vec3, ay: Vec3, hx: f32, hy: f32) {
    let n = ax.cross(ay).normalize();
    // A legend's made-up text is seeded by where it is (the fraction of its code).
    let mut mm = *mm;
    if mm.pat == pat::LEGEND {
        mm.extra = mm.extra.floor() + 0.05 + 0.9 * hash(c.x * 17.3 + c.y * 31.1 + c.z * 7.7);
    }
    let mm = &mm;
    let asp = hx / hy;
    let base = m.v.len() as u32;
    for (u, v) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        m.push(c + ax * (u * hx) + ay * (v * hy), n, [u * asp, v], mm, asp);
    }
    m.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}
/// Quad with uv the local frame scaled to [-1, 1] (displays).
pub(crate) fn screen_quad(m: &mut Mesh, mm: &Mat, c: Vec3, ax: Vec3, ay: Vec3, hx: f32, hy: f32) {
    let n = ax.cross(ay).normalize();
    let base = m.v.len() as u32;
    for (u, v) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        m.push(c + ax * (u * hx) + ay * (v * hy), n, [u, v], mm, 0.0);
    }
    m.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A printed legend on a face, centred at (u, v): `hw` x `hh` half size.
fn label(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, st: u32, s: u32) {
    let col = panel_col();
    rect(m, &legend_mat(st, s, col), f.at_h(u, v, 0.0004), f.ax, f.ay, hw, hh);
}
fn label_c(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, st: u32, s: u32, col: Vec3) {
    rect(m, &legend_mat(st, s, col), f.at_h(u, v, 0.0004), f.ax, f.ay, hw, hh);
}
/// A word's worth of legend sized for its text (glyph height `h`).
fn word(m: &mut Mesh, f: &Face, u: f32, v: f32, h: f32, s: &str) {
    let n = s.chars().count().max(1) as f32;
    let hw = (n * 1.45 - 0.45) * h * 0.25 + h * 0.3;
    label(m, f, u, v, hw, h * 0.72, style::TEXT, sid(s));
}

/// A Dzus fastener: the quarter-turn stud every Boeing panel is held in by.
fn dzus(m: &mut Mesh, f: &Face, u: f32, v: f32, seed: f32) {
    let c = f.at(u, v);
    m.lathe(&mt(mat::PLASTIC, 0.12, 0.10, 0.085), c, f.n(), &[[0.0042, 0.0], [0.0042, 0.0008], [0.0034, 0.0016], [0.0, 0.0018]], 10, true);
    m.rbox(&BTN, c + f.n() * 0.0017, Vec3::new(0.0032, 0.0005, 0.0003), 0.0002, f.rot() * Quat::from_rotation_z(hash(seed) * PI));
}
/// A panel plate standing proud of its face with a fastener at each corner.
fn plate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let (cu, cv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let (hu, hv) = ((u1 - u0) * 0.5 - 0.0008, (v1 - v0) * 0.5 - 0.0008);
    m.rbox(&PANEL, f.at_h(cu, cv, 0.0015), Vec3::new(hu, hv, 0.0028), 0.0012, f.rot());
    let g = f.sub(0.0, 0.0, 0.0043);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        dzus(m, &g, cu + du * (hu - 0.007), cv + dv * (hv - 0.0065), seed + k as f32);
    }
}

/// A Boeing rotary knob: cream skirt and grip with a pointer line, a scale round it.
fn knob(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, seed: f32, scale: bool) {
    let c = f.at(u, v);
    let n = f.n();
    m.lathe(&CREAM, c, n, &[[r * 1.25, 0.0], [r * 1.25, r * 0.25], [r * 1.05, r * 0.4], [r * 0.95, r * 0.45], [r * 0.95, r * 1.3], [r * 0.85, r * 1.4], [0.0, r * 1.42]], 14, false);
    // The pointer: a dark line across the top of the grip.
    let a = (hash(seed) - 0.5) * 2.4;
    let rot = f.rot() * Quat::from_rotation_z(a);
    m.rbox(&BTN, c + n * (r * 1.42), Vec3::new(r * 0.12, r * 0.8, 0.0004), 0.0002, rot);
    if scale {
        let col = panel_col();
        rect(m, &legend_mat(style::SCALE, (hash(seed + 3.0) * 9.0) as u32, col), c + n * 0.0004, f.ax, f.ay, r * 2.6, r * 2.6);
    }
}
/// Bat-handle toggle switch.
fn toggle(m: &mut Mesh, f: &Face, u: f32, v: f32, up: bool) {
    let c = f.at(u, v);
    let n = f.n();
    m.lathe(&CHROME, c, n, &[[0.0055, 0.0], [0.0055, 0.0018], [0.0042, 0.0022], [0.0042, 0.0042], [0.0, 0.0044]], 6, false);
    let tip = c + n * 0.016 + f.ay * if up { 0.006 } else { -0.006 };
    m.tube(&CHROME, &[c + n * 0.004, tip], &|t| 0.0018 + 0.0006 * t, 8, true);
    m.dome(&CHROME, tip, (tip - c).normalize(), 0.0026, 0.002, 8, 0.0);
}
/// A guarded switch: the toggle under a hinged cover (red for the ones that matter).
fn guarded(m: &mut Mesh, f: &Face, u: f32, v: f32, red: bool) {
    toggle(m, f, u, v, false);
    let col = if red { mt(mat::PAINT, 0.36, 0.025, 0.02) } else { mt(mat::PLASTIC, 0.03, 0.03, 0.032) };
    let rot = f.rot() * Quat::from_rotation_x(-0.15);
    m.rbox(&col, f.at_h(u, v - 0.002, 0.012), Vec3::new(0.009, 0.016, 0.009), 0.0025, rot);
}
/// Square push-button switch with its legend lit or dark (`st` a BUTTON style).
fn pbutton(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, st: u32, s: u32) {
    m.rbox(&BTN, f.at_h(u, v, 0.004), Vec3::new(hw + 0.0015, hh + 0.0015, 0.004), 0.0012, f.rot());
    let col = Vec3::new(0.022, 0.022, 0.024);
    rect(m, &legend_mat(st, s, col), f.at_h(u, v, 0.0082), f.ax, f.ay, hw, hh);
}
/// A small round push button (the MCP's, the CDU's line keys).
fn rbutton(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, col: &Mat) {
    m.lathe(col, f.at(u, v), f.n(), &[[r * 1.3, 0.0], [r * 1.3, r * 0.3], [r, r * 0.35], [r, r * 1.2], [0.0, r * 1.25]], 12, false);
}
/// A display unit: the bezel, the black mask, the screen under its glass and
/// the brightness knob.
fn display(m: &mut Mesh, f: &Face, u: f32, v: f32, half: f32, id: f32, seed: f32) {
    let rot = f.rot();
    m.rbox(&BEZEL, f.at_h(u, v, 0.006), Vec3::new(half, half, 0.010), 0.005, rot);
    // The bezel's chamfered inner lip and the black mask behind the glass.
    let s = half * 0.74;
    m.rbox(&FRAME, f.at_h(u, v, 0.0145), Vec3::new(s + 0.012, s + 0.012, 0.0022), 0.008, rot);
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(u, v, 0.0168), f.ax, f.ay, s, s);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (half - 0.008), v + dv * (half - 0.008), 0.016), f.n(), 0.0024, seed + k as f32);
    }
    // Brightness knob in the lower corner.
    let g = f.sub(0.0, 0.0, 0.016);
    m.lathe(&BTN, g.at(u + half - 0.016, v - half + 0.014), f.n(), &[[0.006, 0.0], [0.006, 0.006], [0.005, 0.007], [0.0, 0.007]], 10, false);
}
/// An instrument (standby gauge) with its bezel and screws.
fn instrument(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, id: f32, seed: f32) {
    let rot = f.rot();
    m.rbox(&BEZEL, f.at_h(u, v, 0.004), Vec3::new(r + 0.009, r + 0.009, 0.006), 0.004, rot);
    let c = f.at_h(u, v, 0.0105);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, f.n(), f.ay, r, 36);
    m.lathe(&BTN, c, f.n(), &[[r, 0.0], [r, 0.006], [r + 0.003, 0.0075], [r + 0.0055, 0.007]], 36, false);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (r + 0.004), v + dv * (r + 0.004), 0.0102), f.n(), 0.0022, seed + k as f32);
    }
}
/// A small window showing a readout (MCP counters, radio frequencies).
fn readout(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, id: f32) {
    m.rbox(&BTN, f.at_h(u, v, 0.0006), Vec3::new(hw + 0.003, hh + 0.003, 0.0012), 0.001, f.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(u, v, 0.0022), f.ax, f.ay, hw, hh);
}
fn annunciator(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, s: u32, lit: bool) {
    let col = Vec3::new(0.03, 0.028, 0.026);
    m.rbox(&BTN, f.at_h(u, v, 0.002), Vec3::new(hw + 0.002, hh + 0.002, 0.002), 0.001, f.rot());
    rect(m, &legend_mat(if lit { style::BUTTON_LIT } else { style::BUTTON }, s, col), f.at_h(u, v, 0.0042), f.ax, f.ay, hw, hh);
}

/// Fill a panel with controls: rows of knobs, switches and buttons with
/// their legends, made up from `seed` the way Boeing panels look.
fn populate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let w = u1 - u0;
    let h = v1 - v0;
    let rows = ((h / 0.052).floor() as usize).max(1);
    let rh = h / rows as f32;
    let g = f.sub(0.0, 0.0, 0.0043);
    // A title along the top of some panels.
    if hash(seed * 1.7) > 0.4 && h > 0.06 {
        label(m, &g, (u0 + u1) * 0.5, v1 - 0.0075, (w * 0.22).min(0.04), 0.0038, style::TEXT, 0);
    }
    for r in 0..rows {
        let vy = v1 - (r as f32 + 0.6) * rh - 0.003;
        let kind = (hash(seed + r as f32 * 3.1) * 6.0) as u32;
        let pitch = match kind {
            0 | 1 => 0.040,
            2 => 0.030,
            3 => 0.034,
            _ => 0.038,
        };
        let n = ((w - 0.02) / pitch).floor().max(1.0) as usize;
        let start = (u0 + u1) * 0.5 - (n as f32 - 1.0) * pitch * 0.5;
        for i in 0..n {
            let x = start + i as f32 * pitch;
            let s2 = seed + r as f32 * 13.0 + i as f32 * 1.37;
            let pick = hash(s2);
            if pick < 0.12 {
                continue;
            }
            match kind {
                0 | 1 => {
                    knob(m, &g, x, vy - 0.004, 0.0062, s2, true);
                    label(m, &g, x, vy + 0.016, 0.014, 0.0032, style::TEXT, 0);
                }
                2 => {
                    toggle(m, &g, x, vy - 0.003, pick > 0.55);
                    label(m, &g, x, vy + 0.013, 0.012, 0.003, style::TEXT, 0);
                    label(m, &g, x, vy - 0.016, 0.009, 0.0028, style::TEXT, 0);
                }
                3 => {
                    let st = if pick > 0.93 { style::BUTTON_AMBER } else if pick > 0.88 { style::BUTTON_ON } else { style::BUTTON };
                    pbutton(m, &g, x, vy - 0.003, 0.0115, 0.0105, st, 0);
                    label(m, &g, x, vy + 0.0155, 0.013, 0.003, style::TEXT, 0);
                }
                4 => {
                    if pick > 0.6 {
                        guarded(m, &g, x, vy - 0.003, pick > 0.85);
                    } else {
                        toggle(m, &g, x, vy - 0.003, pick > 0.35);
                    }
                    label(m, &g, x, vy + 0.016, 0.013, 0.003, style::TEXT, 0);
                }
                _ => {
                    annunciator(m, &g, x, vy, 0.012, 0.0065, 0, pick > 0.95);
                    label(m, &g, x, vy - 0.013, 0.012, 0.0028, style::TEXT, 0);
                }
            }
        }
    }
}

/// A stretch of panel made of plates, each populated.
fn panel_run(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, cols: usize, rows: usize, seed: f32) {
    let (dw, dh) = ((u1 - u0) / cols as f32, (v1 - v0) / rows as f32);
    for i in 0..cols {
        for j in 0..rows {
            let (a, b) = (u0 + i as f32 * dw, v0 + j as f32 * dh);
            let s = seed + (i * 7 + j * 3) as f32;
            plate(m, f, a, b, a + dw, b + dh, s);
            populate(m, f, a + 0.004, b + 0.004, a + dw - 0.004, b + dh - 0.004, s);
        }
    }
}

// ---------------------------------------------------------------- the deck

pub fn build(b: &mut Builder) {
    let mut shell = Mesh::default();
    lining(&mut shell);
    floor(&mut shell);
    b.fixed("flight deck lining", G_ENGINE, shell);

    let mut m = Mesh::default();
    let mf = main_panel(&mut m);
    glareshield(&mut m, &mf);
    pedestal_fixed(&mut m);
    b.fixed("flight deck panels", G_ENGINE, m);

    let mut o = Mesh::default();
    overhead(&mut o);
    b.fixed("overhead panel", G_ENGINE, o);

    let mut s = Mesh::default();
    seats(&mut s);
    side_consoles(&mut s);
    aft_deck(&mut s);
    b.fixed("flight deck furnishings", G_ENGINE, s);

    levers(b);
    columns(b);
}

// ---------------------------------------------------------------- lining, windows, floor

const D_FRONT: f32 = 3.85;
const D_BACK: f32 = 7.85;

fn lining(m: &mut Mesh) {
    let (z0, z1) = (bz(D_FRONT), bz(D_BACK));
    let nz = 120;
    let nt = 72;
    let mut side = Mesh::default();
    let rows: Vec<Vec<SP>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            let s = lsec(z);
            // Down to just under the floor.
            let (mut a, mut c) = (0.0f32, PI);
            for _ in 0..30 {
                let mid = (a + c) * 0.5;
                if sec_pt(&s, mid).y > FLOOR - 0.02 { a = mid } else { c = mid }
            }
            let mut arc = 0.0;
            let mut prev = sec_pt(&s, 0.0);
            (0..=nt)
                .map(|k| {
                    let p = sec_pt(&s, k as f32 / nt as f32 * a);
                    arc += (p - prev).length();
                    prev = p;
                    (Vec3::new(p.x, p.y, z), [z, arc], 0.0)
                })
                .collect()
        })
        .collect();
    side.grid(&LINER, &rows, false, true);
    // Off-white overhead, a warmer grey on the walls, darker kick panels.
    for v in &mut side.v {
        let y = v.pos[1];
        let z = v.pos[2];
        let mut c = if y > 3.75 {
            Vec3::new(0.40, 0.38, 0.34)
        } else if y < FLOOR + 0.45 {
            Vec3::new(0.12, 0.105, 0.09)
        } else {
            Vec3::new(0.31, 0.285, 0.25)
        };
        // Ahead of the glareshield and under the windshield: anti-glare black.
        let k = smoothstep(bz(4.70), bz(4.58), z) * smoothstep(3.75, 3.55, y);
        c = c.lerp(Vec3::new(0.03, 0.028, 0.027), k);
        v.col = [c.x, c.y, c.z, 1.0];
        if k > 0.5 {
            v.mat[0] = mat::FABRIC;
        }
    }
    m.append(&side.mirror_x());
    m.append(&side);

    // The window frames and the post covers between the windows.
    window_frames(m);

    // The bulkheads: in front, under the windshield; behind, round the door.
    let zf = bz(D_FRONT);
    let s = lsec(zf);
    let ring: Vec<Vec3> = (0..64)
        .map(|k| {
            let th = k as f32 / 64.0 * TAU;
            let q = sec_pt(&s, if th > PI { TAU - th } else { th });
            Vec3::new(if th > PI { -q.x } else { q.x }, q.y.max(FLOOR - 0.02), zf)
        })
        .collect();
    m.fan(&mt(mat::PLASTIC, 0.10, 0.09, 0.08), Vec3::new(0.0, FLOOR + 0.5, zf), &ring, Vec3::Z);
}

// ---------------------------------------------------------------- window frames and posts

/// The moulded window surrounds, the covers over the posts between the
/// windows, and the seals where they meet the glass.
const SURROUND: Mat = mt(mat::PLASTIC, 0.285, 0.262, 0.226);
const POST: Mat = mt(mat::PLASTIC, 0.262, 0.240, 0.205);
const SEAL: Mat = mt(mat::RUBBER, 0.022, 0.022, 0.024);

/// Signed distance to one window's opening, as `window` measures it (0 the
/// right windshield, 1 and 2 the right side windows; negative inside).
fn win_k(k: usize, p: Vec3) -> f32 {
    let d = p.z + ZN;
    if k == 0 {
        let poly = WSHIELD.map(|v| Vec2::new(v[0], v[1]));
        return (sd_poly(Vec2::new(p.x, p.y), &poly) - 0.06).max(d - 5.26);
    }
    let w = WIN[k];
    let th = p.x.atan2(p.y - 0.3);
    let q = Vec2::new((d - (w[0] + w[1]) * 0.5).abs() - (w[1] - w[0]) * 0.5, ((th - (w[2] + w[3]) * 0.5).abs() - (w[3] - w[2]) * 0.5) * 3.4);
    let q = q + Vec2::splat(0.06);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - 0.06
}
/// The same for the lining's cut round window k (`lining_window`).
fn lin_k(k: usize, q: Vec3) -> f32 {
    let r = Vec2::new(q.x, q.y - 0.3).normalize_or_zero();
    win_k(k, q + Vec3::new(r.x, r.y, 0.0) * INSET) - TRIM
}
/// The opening that is not window k nearest to p (the left windshield counts).
fn win_other(k: usize, p: Vec3) -> f32 {
    let mut d = win_k(0, Vec3::new(-p.x, p.y, p.z));
    for j in 0..3 {
        if j != k {
            d = d.min(win_k(j, p));
        }
    }
    d
}

/// Angle round the section (from the crown, about 0.3 m up) of a point.
fn polar(p: Vec3) -> f32 {
    p.x.atan2(p.y - 0.3)
}
/// The skin (or the lining) at station z, polar angle a.
fn shell_pt(z: f32, a: f32, lining: bool) -> Vec3 {
    let q = ray_pt(&if lining { lsec(z) } else { sec(z) }, a);
    Vec3::new(q.x, q.y, z)
}
/// The point of the skin (or the lining) radially in line with p.
fn snap(p: Vec3, lining: bool) -> Vec3 {
    shell_pt(p.z, polar(p), lining)
}
/// Outward normal of the skin (or the lining) at p.
fn shell_n(p: Vec3, lining: bool) -> Vec3 {
    let (z, a, e) = (p.z, polar(p), 0.002);
    let dz = shell_pt(z + e, a, lining) - shell_pt(z - e, a, lining);
    let da = shell_pt(z, a + e, lining) - shell_pt(z, a - e, lining);
    let n = dz.cross(da).normalize_or_zero();
    if n.dot(Vec3::new(p.x, p.y - 0.3, 0.0)) < 0.0 { -n } else { n }
}
/// Direction along a shell (normal n) in which f grows fastest.
fn grad_on(p: Vec3, n: Vec3, f: &dyn Fn(Vec3) -> f32) -> Vec3 {
    let e = 0.002;
    let g = Vec3::new(f(p + Vec3::X * e) - f(p - Vec3::X * e), f(p + Vec3::Y * e) - f(p - Vec3::Y * e), f(p + Vec3::Z * e) - f(p - Vec3::Z * e));
    (g - n * g.dot(n)).normalize_or_zero()
}
/// From p, along the shell in direction g, the point where f reaches `level`.
fn walk(p: Vec3, g: Vec3, lining: bool, f: &dyn Fn(Vec3) -> f32, level: f32) -> Vec3 {
    let at = |t: f32| snap(p + g * t, lining);
    let (mut t0, mut v0) = (0.0f32, f(p));
    let mut t1 = level - v0;
    let mut v1 = f(at(t1));
    for _ in 0..10 {
        if (v1 - level).abs() < 2e-5 || (v1 - v0).abs() < 1e-7 {
            break;
        }
        let t2 = t1 + ((level - v1) * (t1 - t0) / (v1 - v0)).clamp(-0.05, 0.05);
        (t0, v0) = (t1, v1);
        t1 = t2;
        v1 = f(at(t1));
    }
    at(t1)
}
/// Whether p is inside the lining's section (in the cabin).
fn in_cabin(p: Vec3) -> bool {
    let s = lsec(p.z);
    let up = p.y > s.mid;
    let n = if up { s.nt } else { s.nb };
    let h = if up { s.top - s.mid } else { s.mid - s.bot };
    (p.x / s.w).abs().powf(n) + ((p.y - s.mid) / h).abs().powf(n) < 1.0
}
/// A closed ring resampled every `step` metres.
fn even(ring: &[Vec3], step: f32) -> Vec<Vec3> {
    let n = ring.len();
    let mut acc = vec![0.0f32];
    for k in 0..n {
        acc.push(acc[k] + (ring[(k + 1) % n] - ring[k]).length());
    }
    let total = acc[n];
    let m = (total / step).round().max(3.0) as usize;
    let mut j = 0;
    (0..m)
        .map(|i| {
            let s = total * i as f32 / m as f32;
            while acc[j + 1] < s {
                j += 1;
            }
            let t = (s - acc[j]) / (acc[j + 1] - acc[j]).max(1e-9);
            ring[j].lerp(ring[(j + 1) % n], t)
        })
        .collect()
}
/// Window k's opening in the skin, every centimetre, exactly on its edge.
fn edge_ring(k: usize) -> Vec<Vec3> {
    let f = |p: Vec3| win_k(k, p);
    even(&window_ring(k, 0.0, false), 0.01)
        .into_iter()
        .map(|p| {
            let p = snap(p, false);
            walk(p, grad_on(p, shell_n(p, false), &f), false, &f, 0.0)
        })
        .collect()
}
/// Moving average along an open run, ends held.
fn smooth<T: Copy + std::ops::Add<Output = T> + std::ops::Mul<f32, Output = T>>(v: &mut [T], passes: usize) {
    for _ in 0..passes {
        let o = v.to_vec();
        for i in 1..v.len().saturating_sub(1) {
            v[i] = o[i - 1] * 0.25 + o[i] * 0.5 + o[i + 1] * 0.25;
        }
    }
}

/// A post between two windows: its centre line on the skin and half width.
struct Post {
    c: Vec<Vec3>,
    half: Vec<f32>,
    /// 0 standing proud of the lining, 1 sunk behind it (the ends).
    sink: Vec<f32>,
    /// Along the line, across it, and into the cabin.
    ax: Vec<(Vec3, Vec3, Vec3)>,
    /// How far the face stands in from the skin.
    depth: Vec<f32>,
}
impl Post {
    /// Whether a point of the skin lies at least `margin` inside the cover where it stands proud.
    fn covers(&self, p: Vec3, margin: f32) -> bool {
        (0..self.c.len()).any(|i| self.sink[i] < 0.5 && (self.c[i] - p).length() + margin < self.half[i])
    }
}

/// The post where the openings `a` and `b` (edge rings) come within `maxgap`
/// of each other; the cover laps `over` onto the glass either side, at least
/// `min_half` wide, and runs on `ext` past where the gap opens up.
fn post_between(a: &[Vec3], b: &[Vec3], maxgap: f32, over: f32, min_half: f32, ext: f32) -> Post {
    let n = a.len();
    let near: Vec<(Vec3, f32)> = a
        .iter()
        .map(|&p| {
            let q = b.iter().copied().min_by(|x, y| (*x - p).length().total_cmp(&(*y - p).length())).unwrap();
            (snap((p + q) * 0.5, false), (q - p).length())
        })
        .collect();
    // The longest run (round the ring) where the openings are close.
    let start = (0..n).find(|&i| near[i].1 >= maxgap).unwrap_or(0);
    let (mut best, mut cur) = (Vec::new(), Vec::new());
    for k in 1..=n {
        let i = (start + k) % n;
        if near[i].1 < maxgap {
            cur.push(near[i]);
        } else if !cur.is_empty() {
            if cur.len() > best.len() {
                best = std::mem::take(&mut cur);
            }
            cur.clear();
        }
    }
    if cur.len() > best.len() {
        best = cur;
    }
    let mut c: Vec<Vec3> = best.iter().map(|x| x.0).collect();
    let mut gap: Vec<f32> = best.iter().map(|x| x.1).collect();
    smooth(&mut c, 12);
    smooth(&mut gap, 12);
    // Even spacing along the line, then the ends carried on.
    let mut acc = vec![0.0f32];
    for k in 1..c.len() {
        acc.push(acc[k - 1] + (c[k] - c[k - 1]).length());
    }
    let total = *acc.last().unwrap();
    let m = (total / 0.012).ceil().max(2.0) as usize;
    let mut j = 0;
    let mut line = Vec::new();
    let mut half = Vec::new();
    for i in 0..=m {
        let s = total * i as f32 / m as f32;
        while j + 2 < acc.len() && acc[j + 1] < s {
            j += 1;
        }
        let t = ((s - acc[j]) / (acc[j + 1] - acc[j]).max(1e-9)).clamp(0.0, 1.0);
        line.push(snap(c[j].lerp(c[j + 1], t), false));
        half.push((gap[j] + (gap[j + 1] - gap[j]) * t) * 0.5 + over);
    }
    // Carried on past each end, sinking into the lining as it goes.
    let mut sink = vec![0.0f32; line.len()];
    for end in [0usize, 1] {
        let (p, q) = if end == 0 { (line[0], line[3]) } else { (line[line.len() - 1], line[line.len() - 4]) };
        let d = (p - q).normalize();
        let steps = (ext / 0.012).ceil() as usize;
        for s in 1..=steps {
            let f = s as f32 / steps as f32;
            let e = snap(p + d * (ext * f), false);
            let h = if end == 0 { half[0] } else { *half.last().unwrap() };
            let k = smoothstep(0.15, 0.85, f);
            if end == 0 {
                line.insert(0, e);
                half.insert(0, h);
                sink.insert(0, k);
            } else {
                line.push(e);
                half.push(h);
                sink.push(k);
            }
        }
    }
    for h in &mut half {
        *h = h.max(min_half);
    }
    smooth(&mut half, 6);
    let n = line.len();
    let ax: Vec<(Vec3, Vec3, Vec3)> = (0..n)
        .map(|i| {
            let t = (line[(i + 1).min(n - 1)] - line[i.saturating_sub(1)]).normalize();
            let t = if t.y < 0.0 { -t } else { t };
            let out = shell_n(line[i], false);
            let d = (-out + t * out.dot(t)).normalize();
            (t, t.cross(d).normalize(), d)
        })
        .collect();
    // How deep the lining is along the cover's own axis: it stands 2 cm proud.
    let mut depth: Vec<f32> = (0..n)
        .map(|i| {
            let (c, d) = (line[i], ax[i].2);
            let mut t = 0.0;
            while t < 0.25 && !in_cabin(c + d * t) {
                t += 0.002;
            }
            let t = t.clamp(0.04, 0.16);
            t + 0.02 - 0.034 * sink[i]
        })
        .collect();
    smooth(&mut depth, 10);
    Post { c: line, half, sink, ax, depth }
}
impl Post {
    /// A face on the cover's crown at fraction f of its proud length, upright
    /// along the post.
    fn face(&self, f: f32) -> Face {
        let mut proud: Vec<usize> = (0..self.c.len()).filter(|&i| self.sink[i] < 0.5).collect();
        if self.c[proud[0]].y > self.c[*proud.last().unwrap()].y {
            proud.reverse();
        }
        let i = proud[((proud.len() - 1) as f32 * f.clamp(0.0, 1.0)).round() as usize];
        let (t, _, d) = self.ax[i];
        Face::new(self.c[i] + d * (self.depth[i] + 0.0048), t.cross(d), t)
    }
}

/// The moulded cover over a post: a shallow box section from the glass on
/// one side to the glass on the other, standing a little proud of the
/// lining, with a seal along each edge and its ends closed.
fn post_cover(m: &mut Mesh, p: &Post) {
    let n = p.c.len();
    let (frames, depth) = (&p.ax, &p.depth);
    let mut rows: Vec<Vec<SP>> = Vec::new();
    let mut seals = [Vec::new(), Vec::new()];
    let mut arc = 0.0;
    for i in 0..n {
        if i > 0 {
            arc += (p.c[i] - p.c[i - 1]).length();
        }
        let (c, (_, s, d), h, dd) = (p.c[i], frames[i], p.half[i], depth[i]);
        let side = [(h, 0.002), (h - 0.002, 0.008), (h - 0.009, dd * 0.45), (h - 0.018, dd - 0.011), (h - 0.025, dd - 0.003), (h - 0.036, dd), (h * 0.45, dd + 0.004)];
        let mut prof: Vec<(f32, f32)> = side.iter().map(|&(x, y)| (-x, y)).collect();
        prof.push((0.0, dd + 0.0048));
        prof.extend(side.iter().rev().copied());
        let mut u = 0.0;
        let mut row: Vec<SP> = Vec::new();
        for (j, &(x, y)) in prof.iter().enumerate() {
            if j > 0 {
                let (px, py) = prof[j - 1];
                u += Vec2::new(x - px, y - py).length();
            }
            row.push((c + s * x + d * y, [arc, u], 0.0));
        }
        rows.push(row);
        for (k, sx) in [-1.0f32, 1.0].iter().enumerate() {
            seals[k].push(c + s * (sx * (h - 0.0005)) + d * 0.0025);
        }
    }
    let start = m.v.len();
    let nc = rows[0].len();
    m.grid(&POST, &rows, false, false);
    let probe = &m.v[start + (n / 2) * nc + nc / 2];
    if Vec3::from(probe.nrm).dot(frames[n / 2].2) < 0.0 {
        for v in &mut m.v[start..] {
            v.nrm = (-Vec3::from(v.nrm)).into();
        }
    }
    for (i, sgn) in [(0usize, -1.0f32), (n - 1, 1.0)] {
        let ring: Vec<Vec3> = rows[i].iter().map(|r| r.0).collect();
        let cen = ring.iter().copied().sum::<Vec3>() / ring.len() as f32;
        m.fan(&POST, cen, &ring, frames[i].0 * sgn);
    }
    for s in &seals {
        m.tube(&SEAL, s, &|_| 0.0042, 8, true);
    }
}

/// One window's frame from inside: a rubber seal round the glass, the reveal
/// straight in from the skin to the lining, and a moulded surround whose
/// rounded lip laps over the lining's cut edge. Skips what a post covers.
/// Returns where screws go (point, normal, seed).
fn frame_window(m: &mut Mesh, k: usize, ring: &[Vec3], posts: &[&Post]) -> Vec<(Vec3, Vec3, f32)> {
    let fs = |p: Vec3| win_k(k, p);
    let fl = |p: Vec3| lin_k(k, p);
    let n = ring.len();
    // How wide the surround can be before it meets the next window's.
    let mut w: Vec<f32> = ring.iter().map(|&p| ((win_other(k, p) - 2.0 * TRIM) * 0.5 - 0.004).clamp(0.012, 0.042)).collect();
    for _ in 0..8 {
        let o = w.clone();
        for i in 0..n {
            w[i] = o[(i + n - 1) % n].min(o[i]).min(o[(i + 1) % n]) * 0.5 + (o[(i + n - 1) % n] + o[i] + o[(i + 1) % n]) / 6.0;
        }
    }
    let prof: [(f32, f32); 9] = [(-0.004, 0.0065), (-0.001, 0.0105), (0.004, 0.0125), (0.010, 0.0118), (0.30, 0.0085), (0.55, 0.0055), (0.78, 0.003), (0.93, 0.0015), (1.0, 0.0006)];
    // Where a post hides the whole frame (out to the lining's cut) leave it out;
    // where it hides only part, the frame runs on under the post's side.
    let hidden: Vec<bool> = ring.iter().map(|&p| posts.iter().any(|q| q.covers(p, TRIM + 0.006))).collect();
    // Start the loop where it is hidden, so runs don't wrap.
    let first = (0..n).find(|&i| hidden[i]).map(|i| i + 1).unwrap_or(0);
    let mut runs: Vec<(Vec<Vec<SP>>, Vec<Vec<SP>>)> = Vec::new();
    let (mut seal, mut trim): (Vec<Vec<SP>>, Vec<Vec<SP>>) = (Vec::new(), Vec::new());
    let mut screws = Vec::new();
    let mut arc = 0.0;
    let closed = !hidden.iter().any(|&h| h);
    let count = if closed { n + 1 } else { n };
    for s in 0..count {
        let i = (first + s) % n;
        let p = ring[i];
        if s > 0 {
            arc += (p - ring[(first + s - 1) % n]).length();
        }
        if hidden[i] {
            if seal.len() > 1 {
                runs.push((std::mem::take(&mut seal), std::mem::take(&mut trim)));
            }
            seal.clear();
            trim.clear();
            continue;
        }
        let ns = shell_n(p, false);
        let g = grad_on(p, ns, &fs);
        let l0 = snap(p, true);
        let nl = shell_n(l0, true);
        let gl = grad_on(l0, nl, &fl);
        seal.push(vec![
            (p - g * 0.010 - ns * 0.0012, [arc, 0.0], 0.0),
            (p - g * 0.008 - ns * 0.0042, [arc, 0.003], 0.0),
            (p - g * 0.004 - ns * 0.0065, [arc, 0.007], 0.0),
            (p + g * 0.001 - ns * 0.0075, [arc, 0.012], 0.0),
        ]);
        let on_lin = |a: f32, h: f32| walk(l0, gl, true, &fl, a) - nl * h;
        let lip = on_lin(-0.004, 0.0065);
        let a0 = p + g * 0.0005 - ns * 0.006;
        let mut row: Vec<SP> = vec![(a0, [arc, 0.0], 0.0)];
        for t in [0.33f32, 0.66] {
            row.push((a0.lerp(lip, t), [arc, 0.1 * t], 0.0));
        }
        for &(a, h) in &prof {
            let a = if a <= 0.02 { a } else { a * w[i] };
            row.push((on_lin(a, h), [arc, 0.12 + a], 0.0));
        }
        trim.push(row);
        if (arc / 0.32).fract() < (0.01 / 0.32) && arc > 0.05 {
            screws.push((on_lin(0.55 * w[i], 0.0061), -nl, arc + k as f32 * 7.0));
        }
    }
    if seal.len() > 1 {
        runs.push((seal, trim));
    }
    for (g, t) in runs {
        for (rows, mat, probe, lining) in [(g, SEAL, 2usize, false), (t, SURROUND, 5usize, true)] {
            let start = m.v.len();
            let nc = rows[0].len();
            m.grid(&mat, &rows, false, false);
            // Face the cabin.
            let v = &m.v[start + (rows.len() / 2) * nc + probe];
            let into = -shell_n(Vec3::from(v.pos), lining);
            if Vec3::from(v.nrm).dot(into) < 0.0 {
                for v in &mut m.v[start..] {
                    v.nrm = (-Vec3::from(v.nrm)).into();
                }
            }
        }
    }
    screws
}

/// A screw in a cup washer, painted over with the trim.
fn trim_screw(m: &mut Mesh, p: Vec3, n: Vec3, seed: f32) {
    m.lathe(&mt(mat::PLASTIC, 0.27, 0.25, 0.215), p, n, &[[0.0048, 0.0], [0.0048, 0.0005], [0.0035, 0.0013], [0.0, 0.0013]], 14, false);
    m.screw(&mt(mat::SCREW, 0.30, 0.28, 0.25).x(1.0), p + n * 0.001, n, 0.0027, seed);
}

fn window_frames(m: &mut Mesh) {
    let rings: Vec<Vec<Vec3>> = (0..3).map(edge_ring).collect();
    let left: Vec<Vec3> = rings[0].iter().map(|p| Vec3::new(-p.x, p.y, p.z)).collect();
    // The centre post between the windshields, and on each side the post
    // between the windshield and the first side window and the one between
    // the side windows.
    let centre = post_between(&rings[0], &left, 0.17, 0.04, 0.085, 0.10);
    let fwd = post_between(&rings[0], &rings[1], 0.12, 0.045, 0.072, 0.10);
    let aft = post_between(&rings[1], &rings[2], 0.16, 0.042, 0.07, 0.14);
    post_cover(m, &centre);
    let mut side = Mesh::default();
    post_cover(&mut side, &fwd);
    post_cover(&mut side, &aft);
    let mut screws = Vec::new();
    for k in 0..3 {
        screws.extend(frame_window(&mut side, k, &rings[k], &[&centre, &fwd, &aft]));
    }
    for (p, n, seed) in screws {
        trim_screw(&mut side, p, n, seed);
    }
    window_fittings(&mut side, &rings, &fwd, &aft);
    m.append(&side.mirror_x());
    m.append(&side);
    // A deviation card under the standby compass at the top of the centre post.
    let f = centre.face(0.94);
    rect(m, &mt(mat::PLASTIC, 0.05, 0.05, 0.05), f.at_h(0.0, 0.0, 0.0008), f.ax, f.ay, 0.034, 0.026);
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.80, 0.79, 0.74)), f.at_h(0.0, 0.0, 0.0016), f.ax, f.ay, 0.030, 0.022);
    for (u, v) in [(-0.028f32, 0.020f32), (0.028, 0.020), (-0.028, -0.020), (0.028, -0.020)] {
        m.screw(&SCREW_BLACK, f.at_h(u, v, 0.0012), f.n(), 0.0018, u * 31.0 + v);
    }
}

/// The lining `a` metres out beyond window k's cut, out from its edge point
/// p, and the way into the cabin there.
fn beyond(k: usize, p: Vec3, a: f32) -> (Vec3, Vec3) {
    let fl = |q: Vec3| lin_k(k, q);
    let l0 = snap(p, true);
    let nl = shell_n(l0, true);
    let q = walk(l0, grad_on(l0, nl, &fl), true, &fl, a);
    (q, -shell_n(q, true))
}
/// A face lying on the lining at q (normal `n` into the cabin), its x axis
/// along the station (aft), raised by h.
fn lining_face(q: Vec3, n: Vec3, h: f32) -> Face {
    let ax = (Vec3::Z - n * n.z).normalize();
    Face::new(q + n * h, ax, n.cross(ax))
}

/// What hangs round the windows: flood lights along the windshield header,
/// the sun visors stowed above the side windows on their tracks, the No. 2
/// window's slide rails and its opening handle, gaspers on the posts.
fn window_fittings(m: &mut Mesh, rings: &[Vec<Vec3>], fwd: &Post, aft: &Post) {
    let black = mt(mat::PLASTIC, 0.035, 0.034, 0.033);
    // Flood lights: four swivelling cans on the header over each windshield,
    // aimed down at the glareshield and the panel.
    for (k, xt) in [0.50f32, 0.60, 0.70, 0.80].iter().enumerate() {
        let Some(&p) = rings[0].iter().filter(|p| (p.x - xt).abs() < 0.012).max_by(|a, b| a.y.total_cmp(&b.y)) else { continue };
        let (q, n) = beyond(0, p, 0.062);
        m.lathe(&black, q, n, &[[0.016, -0.002], [0.016, 0.002], [0.012, 0.005], [0.0, 0.006]], 16, false);
        let ball = q + n * 0.008;
        m.dome(&black, ball - n * 0.003, n, 0.0075, 0.0075, 10, 0.0);
        let aim = (Vec3::new(xt * 0.55, 3.10, EYE.z - 0.80 + k as f32 * 0.05) - ball).normalize();
        let can = ball + aim * 0.007;
        m.lathe(&black, can, aim, &[[0.0, 0.0], [0.010, 0.0], [0.012, 0.008], [0.012, 0.027], [0.0125, 0.0285], [0.0125, 0.031], [0.0102, 0.031]], 16, false);
        m.disc(&mt(mat::PLASTIC, 0.16, 0.15, 0.13), can + aim * 0.0295, aim, Vec3::Y, 0.0103, 16);
        // The cooling ribs round the back of the can.
        for r in 0..3 {
            m.lathe(&black, can + aim * (0.010 + r as f32 * 0.005), aim, &[[0.0121, 0.0], [0.0129, 0.001], [0.0121, 0.002]], 16, true);
        }
    }

    // Sun visor stowed flat against the ceiling over the No. 2 window, on a
    // track running fore and aft.
    let c1 = rings[1].iter().copied().sum::<Vec3>() / rings[1].len() as f32;
    let top = rings[1].iter().copied().filter(|p| (p.z - c1.z).abs() < 0.02 && p.y > c1.y).max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    let (q, n) = beyond(1, top, 0.13);
    let f = lining_face(q, n, 0.016);
    let visor: Vec<Vec2> = outline_rr(0.15, 0.08, 0.025);
    let rim: Vec<Vec2> = outline_rr(0.156, 0.086, 0.029);
    m.slab(&black, &rim, &[visor.clone()], f.at_h(0.0, 0.0, 0.0025), f.ax, f.ay, 0.006);
    m.slab(&mt(mat::PAINT, 0.075, 0.068, 0.058), &visor, &[], f.at_h(0.0, 0.0, 0.001), f.ax, f.ay, 0.003);
    // Its arm to the carriage on the track, the track screwed to the ceiling.
    let (tq, tn) = beyond(1, top, 0.225);
    let tf = lining_face(tq, tn, 0.0);
    let rail: Vec<Vec3> = (0..=8).map(|j| {
        let p = snap(tf.at(-0.13 + j as f32 * 0.04, 0.0), true);
        p - shell_n(p, true) * 0.006
    }).collect();
    m.tube(&mt(mat::ALU, 0.36, 0.36, 0.37), &rail, &|_| 0.005, 8, true);
    for j in [1usize, 4, 7] {
        m.screw(&SCREW_STEEL, rail[j] + tf.n() * 0.0048, tf.n(), 0.0018, j as f32);
    }
    let car = tf.at_h(-0.07, 0.0, 0.010);
    m.rbox(&black, car, Vec3::new(0.02, 0.011, 0.009), 0.004, tf.rot());
    let hinge = f.at_h(-0.07, 0.083, 0.004);
    m.tube(&black, &[car, hinge], &|_| 0.004, 8, true);
    m.lathe(&CHROME, hinge, f.ax, &[[0.0055, -0.018], [0.0055, 0.018]], 10, false);

    // The No. 2 window slides aft to open: rails along its sill and head.
    let fs = |p: Vec3| win_k(1, p);
    for upper in [false, true] {
        let pts: Vec<Vec3> = rings[1]
            .iter()
            .copied()
            .filter(|p| (p.z - c1.z).abs() < 0.20 && (p.y > c1.y) == upper)
            .collect();
        let mut pts = pts;
        pts.sort_by(|a, b| a.z.total_cmp(&b.z));
        let rail: Vec<Vec3> = pts.iter().step_by(3).map(|&p| {
            let ns = shell_n(p, false);
            let g = grad_on(p, ns, &fs);
            p - ns * 0.022 - g * 0.006
        }).collect();
        if rail.len() > 2 {
            m.tube(&mt(mat::ALU, 0.42, 0.42, 0.43), &rail, &|_| 0.0045, 8, true);
        }
    }
    // Its handle low on the post ahead of it: a cast housing, the crank
    // folded down, the red-tipped lock lever and the placard.
    let pf = fwd.face(0.30);
    let side = (c1 - pf.o).dot(pf.ax).signum();
    let hf = pf.sub(side * 0.004, 0.0, 0.0);
    let at = |u: f32, v: f32, h: f32| hf.at_h(u * side, v, h);
    m.rbox(&black, at(0.0, 0.0, 0.010), Vec3::new(0.026, 0.045, 0.012), 0.008, hf.rot());
    let hub = at(0.0, 0.012, 0.024);
    m.lathe(&CHROME, hub - hf.n() * 0.004, hf.n(), &[[0.013, 0.0], [0.013, 0.006], [0.010, 0.009], [0.0, 0.009]], 14, false);
    let tip = at(0.008, -0.085, 0.030);
    m.tube(&black, &[hub + hf.n() * 0.004, at(0.004, -0.04, 0.030), tip], &|t| 0.0058 - 0.0015 * t, 8, true);
    m.lathe(&black, tip, hf.n(), &[[0.0075, 0.0], [0.0085, 0.012], [0.0078, 0.026], [0.0, 0.028]], 12, true);
    let lock = [at(-0.012, -0.02, 0.020), at(-0.026, -0.035, 0.024)];
    m.tube(&black, &lock, &|_| 0.0042, 8, true);
    m.dome(&mt(mat::PAINT, 0.55, 0.05, 0.04), lock[1], (lock[1] - lock[0]).normalize(), 0.0055, 0.008, 10, 0.0);
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.80, 0.79, 0.74)), at(0.0, 0.075, 0.0012), hf.ax, hf.ay, 0.022, 0.016);

    // Gaspers on the posts, at shoulder height.
    for (post, f) in [(fwd, 0.42f32), (aft, 0.55)] {
        let pf = post.face(f);
        let c = pf.at_h(0.0, 0.0, 0.0);
        let n = pf.n();
        m.lathe(&mt(mat::PLASTIC, 0.20, 0.19, 0.17), c, n, &[[0.026, -0.001], [0.026, 0.004], [0.022, 0.010], [0.016, 0.012]], 18, true);
        let aim = (n * 0.85 + (EYE - c).normalize() * 0.5).normalize();
        m.dome(&mt(mat::PLASTIC, 0.16, 0.155, 0.14), c + n * 0.006, aim, 0.0155, 0.012, 14, 0.0);
        m.lathe(&CHROME, c + n * 0.006 + aim * 0.010, aim, &[[0.0065, 0.0], [0.0065, 0.008], [0.0045, 0.009], [0.0, 0.009]], 12, false);
    }
}

/// A rounded rectangle outline, half sizes hx, hy and corner radius r.
fn outline_rr(hx: f32, hy: f32, r: f32) -> Vec<Vec2> {
    let mut v = Vec::new();
    for (q, (sx, sy)) in [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].iter().enumerate() {
        let c = Vec2::new(sx * (hx - r), sy * (hy - r));
        for i in 0..=6 {
            let a = (q as f32 + i as f32 / 6.0) * FRAC_PI_2;
            v.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    v
}

fn floor(m: &mut Mesh) {
    let (z0, z1) = (bz(D_FRONT) + 0.01, bz(D_BACK));
    m.patch(&CARPET, 60, 24, false, |a, c| {
        let z = z0 + (z1 - z0) * a;
        let x = (c - 0.5) * 2.0 * (lin_x(z, FLOOR) + 0.01);
        (Vec3::new(x, FLOOR, z), [z, x], 0.0)
    });
    // Seat tracks either side of each pilot, scuff plates at the side walls.
    for sx in [-1.0f32, 1.0] {
        for dx in [-0.17f32, 0.17] {
            m.rbox(&ALU.pat(pat::KNURL), Vec3::new(sx * EYE.x.abs() + dx, FLOOR + 0.006, EYE.z - 0.05), Vec3::new(0.016, 0.006, 0.42), 0.002, Quat::IDENTITY);
        }
    }
}

// ---------------------------------------------------------------- main instrument panel

/// The main panel's face: displays centred at v = 0.
fn main_face() -> Face {
    let tilt: f32 = 0.17;
    Face::new(Vec3::new(0.0, EYE.y - 0.47, EYE.z - 0.82), Vec3::X, Vec3::new(0.0, tilt.cos(), -tilt.sin()))
}
/// Display units: half size of the bezel, and where they sit (u).
const DU: f32 = 0.11;
const PFD_U: f32 = 0.55;
const ND_U: f32 = 0.33;

fn main_panel(m: &mut Mesh) -> Face {
    let f = main_face();
    // The panel itself: a wide band behind the displays, the centre running
    // down to meet the pedestal, angled wings at the ends.
    let top = 0.175;
    let bot = -0.20;
    let outline: Vec<Vec2> = [(-0.90, top), (0.90, top), (0.97, bot), (0.145, bot), (0.145, -0.43), (-0.145, -0.43), (-0.145, bot), (-0.97, bot)].iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    m.slab(&PANEL_DK, &outline, &[], f.o, f.ax, f.ay, 0.03);
    // A lip along the bottom edge of the outboard panels.
    for sx in [-1.0f32, 1.0] {
        let a = f.at_h(sx * 0.15, bot, 0.005);
        let c = f.at_h(sx * 0.96, bot, 0.005);
        m.tube(&PANEL, &[a, c], &|_| 0.012, 10, true);
    }

    let g = f.sub(0.0, 0.0, 0.0);
    // Six displays.
    for sx in [-1.0f32, 1.0] {
        display(m, &g, sx * PFD_U, 0.0, DU, screen::PFD, sx * 4.0);
        display(m, &g, sx * ND_U, 0.0, DU, screen::ND, sx * 8.0);
    }
    display(m, &g, 0.0, 0.0, DU, screen::EICAS, 20.0);
    display(m, &g, 0.0, -0.285, DU * 0.98, screen::EICAS2, 24.0);

    // Standby instruments between the captain's ND and the EICAS: airspeed,
    // attitude, altimeter, stacked.
    let su = -0.168;
    plate(m, &g, -0.222, -0.112, -0.114, 0.112, 31.0);
    let gi = g.sub(0.0, 0.0, 0.0043);
    instrument(m, &gi, su, 0.073, 0.028, 0.0, 32.0);
    instrument(m, &gi, su, 0.0, 0.031, 1.0, 33.0);
    instrument(m, &gi, su, -0.074, 0.028, 2.0, 34.0);
    // Right of the EICAS: the gear lever panel with its lock override and
    // the alternate gear extension switches, the flap limit placard over it.
    plate(m, &g, 0.114, -0.112, 0.222, 0.112, 41.0);
    let gp = g.sub(0.0, 0.0, 0.0043);
    rect(m, &mt(mat::PLASTIC, 0.012, 0.012, 0.012), gp.at_h(0.150, 0.0, 0.0005), f.ax, f.ay, 0.011, 0.075);
    label(m, &gp, 0.198, 0.089, 0.018, 0.016, style::TEXT2, sid("FLAPS"));
    word(m, &gp, 0.135, 0.068, 0.0045, "UP");
    word(m, &gp, 0.135, 0.0, 0.0045, "OFF");
    word(m, &gp, 0.135, -0.068, 0.0045, "DN");
    rbutton(m, &gp, 0.195, 0.03, 0.006, &BTN);
    word(m, &gp, 0.195, 0.045, 0.0035, "LOCK OVRD");
    pbutton(m, &gp, 0.195, -0.02, 0.011, 0.01, style::BUTTON, sid("NOSE/BODY"));
    pbutton(m, &gp, 0.195, -0.055, 0.011, 0.01, style::BUTTON, sid("WING"));
    word(m, &gp, 0.195, -0.075, 0.0032, "ALTN");

    // Above the displays: heading reference and display source switches over
    // each pilot's pair, the clocks outboard, warning lights.
    for sx in [-1.0f32, 1.0] {
        let s = sx * 10.0;
        plate(m, &g, sx * 0.665 - 0.0, 0.112, sx * 0.22, 0.172, 50.0 + s);
        let gq = g.sub(0.0, 0.0, 0.0043);
        for k in 0..4 {
            let u = sx * (0.29 + k as f32 * 0.09);
            knob(m, &gq, u, 0.136, 0.0058, s + k as f32, true);
            label(m, &gq, u, 0.159, 0.017, 0.0033, style::TEXT, [sid("EIU"), sid("FMC"), sid("IRS"), sid("NAV")][k]);
        }
        // Outboard: the clock, master warning and caution, the panel light knobs.
        let oo = sx * 0.78;
        plate(m, &g, sx * 0.665, -0.19, sx * 0.88, 0.172, 60.0 + s);
        let (ua, ub) = (oo - 0.05, oo + 0.05);
        let (ua, ub) = (ua.min(ub), ua.max(ub));
        let _ = (ua, ub);
        instrument(m, &gq, oo, 0.075, 0.032, 14.0, 61.0 + s);
        readout(m, &gq.sub(0.0, 0.0, 0.011), oo, 0.062, 0.014, 0.0055, screen::CLOCK);
        pbutton(m, &gq, oo - 0.03, -0.012, 0.016, 0.011, style::BUTTON_LIT, sid("WARNING"));
        pbutton(m, &gq, oo + 0.03, -0.012, 0.016, 0.011, style::BUTTON, sid("CAUTION"));
        for k in 0..3 {
            knob(m, &gq, oo + (k as f32 - 1.0) * 0.04, -0.085, 0.0055, s + 20.0 + k as f32, true);
            label(m, &gq, oo + (k as f32 - 1.0) * 0.04, -0.062, 0.016, 0.003, style::TEXT, [sid("PANEL"), sid("FLOOD"), sid("DSPL")][k]);
        }
        pbutton(m, &gq, oo, -0.15, 0.02, 0.012, style::BUTTON, sid("GND PROX"));
        // Under each pilot's pair: the ground proximity panel and the
        // display select switches.
        plate(m, &g, sx * 0.665, -0.19, sx * 0.222, -0.112, 70.0 + s);
        for k in 0..5 {
            let u = sx * (0.27 + k as f32 * 0.085);
            if k % 2 == 0 {
                pbutton(m, &gq, u, -0.145, 0.012, 0.0105, style::BUTTON, 0);
            } else {
                toggle(m, &gq, u, -0.148, k == 1);
            }
            label(m, &gq, u, -0.126, 0.018, 0.003, style::TEXT, 0);
        }
    }
    // Over the EICAS: the EICAS event record button and the brake pressure gauge.
    plate(m, &g, -0.114, 0.112, 0.114, 0.172, 80.0);
    let gq = g.sub(0.0, 0.0, 0.0043);
    for k in 0..4 {
        let u = -0.075 + k as f32 * 0.05;
        annunciator(m, &gq, u, 0.142, 0.016, 0.009, 0, false);
    }
    // Under the upper EICAS, the autobrake selector.
    plate(m, &g, -0.145, -0.17, 0.145, -0.112, 90.0);
    knob(m, &gq, -0.06, -0.142, 0.009, 91.0, true);
    label(m, &gq, -0.06, -0.12, 0.03, 0.0032, style::TEXT, sid("AUTOBRAKES"));
    pbutton(m, &gq, 0.05, -0.142, 0.013, 0.011, style::BUTTON, sid("EICAS"));
    pbutton(m, &gq, 0.09, -0.142, 0.013, 0.011, style::BUTTON, sid("STAT"));
    // Either side of the lower EICAS: the EICAS display select panel and the
    // standby engine indicator.
    for sx in [-1.0f32, 1.0] {
        let _ = sx;
    }

    f
}

// ---------------------------------------------------------------- glareshield and the mode control panel

fn glareshield(m: &mut Mesh, f: &Face) {
    // The MCP's face: under the glareshield's lip, leaning back.
    let lip = f.at_h(0.0, 0.175, 0.0);
    let mcp_t: f32 = 0.38;
    let mcp = Face::new(lip + Vec3::new(0.0, 0.006, 0.135), Vec3::X, Vec3::new(0.0, mcp_t.cos(), -mcp_t.sin()));
    let mh = 0.058; // half height of the MCP strip
    let top_edge = mcp.at(0.0, mh + 0.004);

    // The shroud: a padded top from the MCP's top edge forward to the
    // windshield, the overhang either side over the displays.
    let z_back = top_edge.z + 0.02;
    let z_front = bz(3.95);
    let y_back = top_edge.y + 0.012;
    m.patch(&GLARE, 48, 16, false, |a, c| {
        let t = c;
        let z = z_back + (z_front - z_back) * t;
        let x = (a - 0.5) * 2.0;
        // Lower toward the sides (the shroud curves down round the panel's wings) and forward.
        let y = y_back - 0.10 * t * t - 0.05 * x.powi(4) + 0.01 * (1.0 - x * x);
        let half = lin_x(z, y) - 0.012;
        let p = inside(Vec3::new(x * half.min(1.05), y, z), 0.012);
        (p, [x, z], 0.0)
    });
    // The rounded aft edge of the shroud, over the MCP and out over the panel.
    let edge: Vec<Vec3> = (0..=24)
        .map(|k| {
            let x = (k as f32 / 24.0 - 0.5) * 2.0;
            let y = y_back - 0.012 - 0.05 * x.powi(4) + 0.01 * (1.0 - x * x);
            inside(Vec3::new(x * 1.02, y, z_back + 0.005 + 0.03 * x * x), 0.025)
        })
        .collect();
    m.tube(&GLARE, &edge, &|_| 0.018, 10, true);
    // Its underside: a dark band from the edge down to the panel top.
    m.patch(&GLARE, 24, 3, true, |a, c| {
        let x = (a - 0.5) * 2.0;
        let y = y_back - 0.03 - 0.05 * x.powi(4) + 0.01 * (1.0 - x * x);
        let e = inside(Vec3::new(x * 1.02, y, z_back + 0.005 + 0.03 * x * x), 0.025);
        let p = f.at_h(x * 0.9, 0.175, 0.0);
        (e.lerp(Vec3::new(e.x, p.y.min(e.y), p.z), c), [x, c], 0.0)
    });

    // The MCP box and its face.
    m.rbox(&PANEL_DK, mcp.at_h(0.0, 0.0, -0.03), Vec3::new(0.69, mh + 0.004, 0.03), 0.004, mcp.rot());
    let g = mcp.sub(0.0, 0.0, 0.0);
    // EFIS control panels at the ends, the MCP proper in the middle.
    for sx in [-1.0f32, 1.0] {
        let c = sx * 0.375;
        plate(m, &g, c - 0.093, -mh, c + 0.093, mh, 200.0 + sx);
        let e = g.sub(0.0, 0.0, 0.0043);
        // Minimums and baro knobs (concentric), mode and range selectors, map option buttons.
        knob(m, &e, c - 0.062, 0.012, 0.0072, 201.0 + sx, false);
        readout(m, &e, c - 0.062, -0.025, 0.012, 0.0045, screen::EFIS_MINS);
        label(m, &e, c - 0.062, 0.042, 0.012, 0.0032, style::TEXT, sid("MINS"));
        knob(m, &e, c + 0.062, 0.012, 0.0072, 202.0 + sx, false);
        label(m, &e, c + 0.062, 0.042, 0.012, 0.0032, style::TEXT, sid("BARO"));
        knob(m, &e, c - 0.02, 0.008, 0.0068, 203.0 + sx, true);
        label(m, &e, c - 0.02, 0.042, 0.012, 0.0032, style::TEXT, sid("MAP"));
        knob(m, &e, c + 0.022, 0.008, 0.0068, 204.0 + sx, true);
        label(m, &e, c + 0.022, 0.042, 0.014, 0.0032, style::TEXT, sid("RANGE"));
        for (k, s) in ["WXR", "STA", "WPT", "ARPT", "DATA", "POS", "TERR"].iter().enumerate() {
            let u = c - 0.075 + k as f32 * 0.025;
            rbutton(m, &e, u, -0.037, 0.0052, &BTN);
            label(m, &e, u, -0.024, 0.011, 0.0028, style::TEXT, sid(s));
        }
    }
    plate(m, &g, -0.282, -mh, 0.282, mh, 210.0);
    let e = g.sub(0.0, 0.0, 0.0043);
    // Left to right: F/D switch, A/T arm, THR / SPD, IAS/MACH window and
    // knob, LNAV / VNAV / FL CH, HDG window, bank limit and HDG knob, HOLD,
    // V/S window and wheel, ALT window and knob, ALT HOLD, autopilot CMD
    // buttons, disengage bar, LOC / APP, F/D on the right.
    toggle(m, &e, -0.268, -0.005, true);
    label(m, &e, -0.268, 0.024, 0.008, 0.0045, style::TEXT2, sid("F/D"));
    toggle(m, &e, -0.238, 0.014, true);
    label(m, &e, -0.238, 0.041, 0.012, 0.003, style::TEXT, sid("A/T ARM"));
    pbutton(m, &e, -0.238, -0.021, 0.0095, 0.0085, style::BUTTON, sid("THR"));
    pbutton(m, &e, -0.238, -0.044, 0.0095, 0.0085, style::BUTTON, sid("SPD"));
    readout(m, &e, -0.19, 0.022, 0.022, 0.0085, screen::MCP_IAS);
    label(m, &e, -0.19, 0.043, 0.017, 0.0032, style::TEXT, sid("IAS/MACH"));
    knob(m, &e, -0.19, -0.022, 0.0088, 211.0, false);
    rbutton(m, &e, -0.215, -0.004, 0.0035, &BTN);
    pbutton(m, &e, -0.142, 0.026, 0.0095, 0.0085, style::BUTTON_LIT, sid("LNAV"));
    pbutton(m, &e, -0.142, 0.002, 0.0095, 0.0085, style::BUTTON_LIT, sid("VNAV"));
    pbutton(m, &e, -0.142, -0.028, 0.0095, 0.0085, style::BUTTON, sid("FL CH"));
    readout(m, &e, -0.09, 0.022, 0.02, 0.0085, screen::MCP_HDG);
    label(m, &e, -0.09, 0.043, 0.008, 0.0032, style::TEXT, sid("HDG"));
    knob(m, &e, -0.09, -0.014, 0.0085, 212.0, true);
    pbutton(m, &e, -0.09, -0.044, 0.0095, 0.0075, style::BUTTON, sid("HOLD"));
    readout(m, &e, -0.03, 0.022, 0.022, 0.0085, screen::MCP_VS);
    label(m, &e, -0.03, 0.043, 0.016, 0.0032, style::TEXT, sid("VERT SPD"));
    // The V/S thumbwheel.
    m.lathe(&CREAM.pat(pat::KNURL), e.at_h(-0.03, -0.014, 0.004) - Vec3::X * 0.006, Vec3::X, &[[0.0, 0.0], [0.011, 0.0], [0.011, 0.012], [0.0, 0.012]], 18, false);
    pbutton(m, &e, -0.03, -0.044, 0.0095, 0.0075, style::BUTTON, sid("V/S"));
    readout(m, &e, 0.035, 0.022, 0.026, 0.0085, screen::MCP_ALT);
    label(m, &e, 0.035, 0.043, 0.008, 0.0032, style::TEXT, sid("ALT"));
    knob(m, &e, 0.035, -0.014, 0.0088, 213.0, false);
    pbutton(m, &e, 0.035, -0.044, 0.0095, 0.0075, style::BUTTON, sid("HOLD"));
    for (k, s) in ["CMD L", "CMD C", "CMD R"].iter().enumerate() {
        let u = 0.1 + k as f32 * 0.03;
        pbutton(m, &e, u, 0.018, 0.0105, 0.009, if k == 1 { style::BUTTON_LIT } else { style::BUTTON }, sid(s));
    }
    label(m, &e, 0.13, 0.04, 0.02, 0.0032, style::TEXT, sid("CMD"));
    // The disengage bar.
    m.rbox(&CREAM, e.at_h(0.13, -0.022, 0.006), Vec3::new(0.04, 0.005, 0.004), 0.002, e.rot());
    label(m, &e, 0.13, -0.036, 0.022, 0.0028, style::TEXT, sid("DISENGAGE"));
    pbutton(m, &e, 0.2, 0.014, 0.0095, 0.0085, style::BUTTON, sid("LOC"));
    pbutton(m, &e, 0.2, -0.018, 0.0095, 0.0085, style::BUTTON, sid("APP"));
    toggle(m, &e, 0.248, -0.005, true);
    label(m, &e, 0.248, 0.024, 0.008, 0.0045, style::TEXT2, sid("F/D"));
    // Display select panels at the strip's ends.
    for sx in [-1.0f32, 1.0] {
        let c = sx * 0.585;
        plate(m, &g, c - 0.104, -mh, c + 0.104, mh, 220.0 + sx);
        let dd = g.sub(0.0, 0.0, 0.0043);
        for (k, s) in ["EICAS", "ENG", "STAT", "ELEC/HYD", "FUEL", "ECS"].iter().enumerate() {
            let u = c - 0.075 + k as f32 * 0.03;
            pbutton(m, &dd, u, 0.012, 0.0105, 0.0095, style::BUTTON, sid(s));
        }
        pbutton(m, &dd, c + 0.075, -0.026, 0.012, 0.009, style::BUTTON, sid("CANC"));
        pbutton(m, &dd, c + 0.045, -0.026, 0.012, 0.009, style::BUTTON, sid("RCL"));
        label(m, &dd, c - 0.04, -0.03, 0.04, 0.004, style::TEXT, sid("EICAS"));
        label(m, &dd, c, 0.043, 0.05, 0.0035, style::TEXT, 0);
    }
    // Sunlight round the windshield's lower frame: the standby compass on the
    // centre post, the lights under the shroud.
}

// ---------------------------------------------------------------- pedestal

/// The forward pedestal's CDU faces (right one; the left mirrors it).
fn cdu_face(sx: f32) -> Face {
    let a: f32 = 0.70;
    Face::new(Vec3::new(sx * 0.218, EYE.y - 0.775, EYE.z - 0.475), Vec3::X, Vec3::new(0.0, a.sin(), -a.cos()))
}
fn cdu(m: &mut Mesh, f: &Face, id: f32, seed: f32) {
    let (hw, hh) = (0.073, 0.114);
    m.rbox(&PANEL_DK, f.at_h(0.0, 0.0, -0.02), Vec3::new(hw, hh, 0.02), 0.004, f.rot());
    plate(m, f, -hw, -hh, hw, hh, seed);
    let g = f.sub(0.0, 0.0, 0.0043);
    // Screen, its line select keys either side, the keypad below.
    let sv = 0.052;
    m.rbox(&FRAME, g.at_h(0.0, sv, 0.001), Vec3::new(0.05, 0.044, 0.002), 0.003, g.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), g.at_h(0.0, sv, 0.0032), g.ax, g.ay, 0.044, 0.038);
    for k in 0..6 {
        let v = sv + 0.031 - k as f32 * 0.0125;
        for sx in [-1.0f32, 1.0] {
            m.rbox(&BTN, g.at_h(sx * 0.061, v, 0.002), Vec3::new(0.005, 0.0035, 0.0025), 0.001, g.rot());
            rect(m, &mt(mat::PLASTIC, 0.6, 0.6, 0.6), g.at_h(sx * 0.061, v, 0.0046), g.ax, g.ay, 0.003, 0.0006);
        }
    }
    // Function keys: three rows of five.
    let fkeys = [["INIT", "RTE", "DEP ARR", "ALTN", "VNAV"], ["FIX", "LEGS", "HOLD", "FMC", "PROG"], ["MENU", "NAV RAD", "PREV PAGE", "NEXT PAGE", "EXEC"]];
    let _ = fkeys;
    for r in 0..3 {
        for c in 0..5 {
            let u = -0.052 + c as f32 * 0.026;
            let v = 0.0 - r as f32 * 0.017;
            let s = [[sid("INIT"), sid("REF"), sid("DEP ARR"), sid("ALTN"), sid("VNAV")], [sid("FIX"), sid("LEGS"), sid("HOLD"), sid("FMC"), sid("PROG")], [sid("MENU"), sid("NAV RAD"), sid("PREV PAGE"), sid("NEXT PAGE"), sid("EXEC")]][r][c];
            m.rbox(&BTN, g.at_h(u, v, 0.0025), Vec3::new(0.011, 0.0065, 0.0028), 0.0015, g.rot());
            rect(m, &legend_mat(style::KEY, s, Vec3::new(0.025, 0.025, 0.027)), g.at_h(u, v, 0.0054), g.ax, g.ay, 0.0105, 0.006);
        }
    }
    // The alphabet and number keys.
    for r in 0..6 {
        for c in 0..5 {
            let u = -0.030 + c as f32 * 0.017;
            let v = -0.056 - r as f32 * 0.0105;
            m.rbox(&BTN, g.at_h(u, v, 0.0025), Vec3::new(0.0065, 0.0042, 0.0028), 0.0015, g.rot());
            rect(m, &legend_mat(style::KEY, 0, Vec3::new(0.025, 0.025, 0.027)), g.at_h(u, v, 0.0054), g.ax, g.ay, 0.006, 0.004);
        }
    }
    for r in 0..4 {
        for c in 0..3 {
            let u = -0.062 + c as f32 * 0.011;
            let v = -0.056 - r as f32 * 0.013;
            m.rbox(&mt(mat::PLASTIC, 0.5, 0.48, 0.42), g.at_h(u, v, 0.0025), Vec3::new(0.0045, 0.0052, 0.0028), 0.002, g.rot());
        }
    }
    // Annunciators down the side (EXEC, MSG, OFST...)
    for k in 0..4 {
        rect(m, &legend_mat(style::BUTTON, 0, Vec3::new(0.02, 0.02, 0.02)), g.at_h(0.066, -0.06 - k as f32 * 0.012, 0.0005), g.ax, g.ay, 0.004, 0.0035);
    }
}

/// The pedestal: its forward part (CDUs and the thrust lever quadrant) and
/// the aft part with the fuel control switches and the radio panels.
fn pedestal_fixed(m: &mut Mesh) {
    let f = main_face();
    let base = f.at(0.0, -0.43);
    // Forward pedestal body.
    let zf = base.z + 0.01;
    let zm = EYE.z - 0.33;
    let za = EYE.z + 0.86;
    let ytq = EYE.y - 0.80;
    let yaft = EYE.y - 0.86;
    // Sides and top as one shape swept from the panel to the aft end.
    let w_at = |z: f32| if z < zm - 0.06 { 0.30 } else if z < zm { 0.30 - (z - (zm - 0.06)) / 0.06 * 0.07 } else { 0.23 };
    let top_at = |z: f32| if z < zm { ytq } else { ytq + (yaft - ytq) * ((z - zm) / (za - zm)) };
    let n = 40;
    let rows: Vec<Vec<SP>> = (0..=n)
        .map(|i| {
            let z = zf + (za - zf) * i as f32 / n as f32;
            let (w, t) = (w_at(z), top_at(z));
            let pts = [Vec3::new(-w, FLOOR, z), Vec3::new(-w, t - 0.015, z), Vec3::new(-w + 0.015, t, z), Vec3::new(w - 0.015, t, z), Vec3::new(w, t - 0.015, z), Vec3::new(w, FLOOR, z)];
            pts.iter().enumerate().map(|(k, p)| (*p, [z, k as f32 * 0.2], 0.0)).collect()
        })
        .collect();
    m.grid(&PANEL_DK, &rows, false, true);
    // The aft end.
    let w = w_at(za);
    let t = top_at(za);
    m.quad(&PANEL_DK, [Vec3::new(-w, FLOOR, za), Vec3::new(w, FLOOR, za), Vec3::new(w, t, za), Vec3::new(-w, t, za)]);

    // The CDUs.
    for sx in [-1.0f32, 1.0] {
        let cf = cdu_face(sx);
        // The wedge they sit on.
        m.rbox(&PANEL_DK, cf.at_h(0.0, 0.0, -0.06), Vec3::new(0.08, 0.12, 0.06), 0.004, cf.rot());
        cdu(m, &cf, screen::CDU, 300.0 + sx);
    }

    // The thrust lever quadrant: a hump between the CDUs with the lever slots.
    let tq0 = zf + 0.03;
    let tq1 = zm + 0.02;
    let qc = Vec3::new(0.0, ytq - 0.16, (tq0 + tq1) * 0.5);
    let qr = 0.21;
    let mut arc = Vec::new();
    for k in 0..=16 {
        let a = (k as f32 / 16.0 - 0.5) * 1.15;
        arc.push(qc + Vec3::new(0.0, a.cos() * qr, a.sin() * qr));
    }
    let tqh = 0.145;
    let rows: Vec<Vec<SP>> = arc.iter().map(|p| vec![(*p + Vec3::new(-tqh, -0.25, 0.0) * Vec3::new(1.0, 0.0, 1.0) + Vec3::new(0.0, 0.0, 0.0), [p.z, 0.0], 0.0), (*p - Vec3::X * tqh, [p.z, 0.1], 0.0), (*p + Vec3::X * tqh, [p.z, 0.4], 0.0)]).collect();
    let _ = rows;
    m.patch(&mt(mat::PLASTIC, 0.27, 0.235, 0.19).pat(pat::PANEL), 16, 8, true, |a, c| {
        let ang = (a - 0.5) * 1.15;
        let x = (c - 0.5) * 2.0 * tqh;
        let p = qc + Vec3::new(x, ang.cos() * qr, ang.sin() * qr);
        (p, [x, ang], 0.0)
    });
    for sx in [-1.0f32, 1.0] {
        let ring: Vec<Vec3> = (0..=16).map(|k| { let a = (k as f32 / 16.0 - 0.5) * 1.15; qc + Vec3::new(sx * tqh, a.cos() * qr, a.sin() * qr) }).collect();
        let mut r2 = ring.clone();
        r2.push(Vec3::new(sx * tqh, ytq - 0.01, ring[16].z));
        r2.push(Vec3::new(sx * tqh, ytq - 0.01, ring[0].z));
        m.fan(&mt(mat::PLASTIC, 0.27, 0.235, 0.19), Vec3::new(sx * tqh, ytq + 0.02, qc.z), &r2, Vec3::X * sx);
    }
    // Slots for the four levers, the speedbrake and the flaps.
    for x in [-0.075f32, -0.025, 0.025, 0.075, -0.125, 0.125] {
        m.patch(&BTN, 12, 1, true, |a, c| {
            let ang = (a - 0.5) * 0.95;
            let p = qc + Vec3::new(x + (c - 0.5) * 0.008, ang.cos() * (qr + 0.0006), ang.sin() * (qr + 0.0006));
            (p, [a, c], 0.0)
        });
    }
    // Scale marks along the flap gate.
    let gate = Face::new(qc + Vec3::new(0.137, qr - 0.02, 0.0), Vec3::Z, Vec3::Y);
    let _ = gate;
    for (k, ang) in [-0.40f32, -0.27, -0.14, -0.01, 0.12, 0.25, 0.38].iter().enumerate() {
        let p = qc + Vec3::new(0.137, ang.cos() * (qr + 0.001), -ang.sin() * (qr + 0.001));
        let nrm = (p - qc - Vec3::X * 0.137).normalize();
        let ax = Vec3::X;
        let ay = nrm.cross(ax);
        rect(m, &legend_mat(style::TEXT, sid(["UP", "1", "2", "3", "4", "25", "30"][k.min(6)]), Vec3::new(0.27, 0.235, 0.19)), p, ax, -ay, 0.006, 0.0035);
    }

    // Fuel control switches and the stabiliser trim cutouts behind the quadrant.
    let ft = Face::new(Vec3::new(0.0, top_at(zm + 0.08) + 0.0005, zm + 0.08), Vec3::X, Vec3::new(0.0, 0.0, -1.0));
    let ft = Face::new(ft.o, Vec3::X, Vec3::new(0.0, (yaft - ytq) / (za - zm), -1.0));
    plate(m, &ft, -0.225, -0.05, 0.225, 0.055, 400.0);
    let g = ft.sub(0.0, 0.0, 0.0043);
    label(m, &g, -0.035, 0.038, 0.05, 0.0045, style::TEXT, sid("FUEL CONTROL"));
    for k in 0..4 {
        let u = -0.11 + k as f32 * 0.05;
        let c = g.at(u, 0.0);
        m.lathe(&CHROME, c, g.n(), &[[0.009, 0.0], [0.009, 0.004], [0.004, 0.006], [0.004, 0.018]], 12, false);
        m.dome(&BTN, c + g.n() * 0.02 + g.ay * 0.006, g.n(), 0.011, 0.014, 14, 0.0);
        label(m, &g, u, -0.032, 0.008, 0.005, style::TEXT, sid(["1", "2", "3", "4"][k]));
    }
    word(m, &g, -0.035, 0.022, 0.004, "RUN");
    word(m, &g, -0.035, -0.04, 0.004, "CUTOFF");
    for k in 0..2 {
        guarded(m, &g, 0.09 + k as f32 * 0.03, 0.0, true);
    }
    label(m, &g, 0.105, 0.035, 0.03, 0.004, style::TEXT, sid("STAB TRIM"));
    toggle(m, &g, 0.175, 0.0, false);
    label(m, &g, 0.175, 0.03, 0.02, 0.0035, style::TEXT, sid("ALTN"));

    // The aft electronic panels: radio tuning, audio, the third CDU, weather
    // radar, transponder, the evacuation panel.
    let z0 = zm + 0.17;
    let pa = Face::new(Vec3::new(0.0, top_at(z0) + 0.0005, z0), Vec3::X, Vec3::new(0.0, (yaft - ytq) / (za - zm), -1.0));
    let pa = Face::new(pa.o, pa.ax, pa.ay);
    // Panels are 146 mm wide in three columns; heights in the usual multiples.
    let cols = [-0.149f32, 0.0, 0.149];
    let heights = [[0.095, 0.115, 0.085, 0.095, 0.10], [0.24, 0.07, 0.095, 0.085, 0.0], [0.095, 0.115, 0.085, 0.095, 0.10]];
    for (ci, &cu) in cols.iter().enumerate() {
        let mut v = 0.0f32;
        for (k, &h) in heights[ci].iter().enumerate() {
            if h <= 0.0 {
                break;
            }
            let (v0, v1) = (-(v + h), -v);
            v += h;
            if -v0 > za - z0 + 0.0 {
                break;
            }
            plate(m, &pa, cu - 0.072, v0, cu + 0.072, v1, 500.0 + (ci * 10 + k) as f32);
            let gg = pa.sub(0.0, 0.0, 0.0043);
            let mid = (v0 + v1) * 0.5;
            match (ci, k) {
                (1, 0) => {
                    // The centre CDU, flush with the pedestal.
                    let cf = Face::new(gg.at(cu, mid), gg.ax, gg.ay);
                    cdu(m, &cf, screen::CDU_C, 520.0);
                }
                (0, 0) | (2, 0) => {
                    // Radio tuning panel: two frequency windows, transfer switch, knobs, mode buttons.
                    readout(m, &gg, cu - 0.034, v1 - 0.022, 0.026, 0.0075, screen::RADIO);
                    readout(m, &gg, cu + 0.034, v1 - 0.022, 0.026, 0.0075, screen::RADIO);
                    pbutton(m, &gg, cu, v1 - 0.022, 0.0065, 0.006, style::BUTTON, 0);
                    knob(m, &gg, cu + 0.052, mid - 0.018, 0.0075, 530.0 + ci as f32, false);
                    for b in 0..5 {
                        pbutton(m, &gg, cu - 0.06 + b as f32 * 0.02, mid - 0.016, 0.0072, 0.0058, style::BUTTON, [sid("VHF L"), sid("VHF C"), sid("VHF R"), sid("HF L"), sid("HF R")][b]);
                    }
                }
                (0, 1) | (2, 1) => {
                    // Audio control panel: mic select buttons with their volume knobs.
                    for b in 0..6 {
                        let u = cu - 0.055 + b as f32 * 0.022;
                        pbutton(m, &gg, u, v1 - 0.02, 0.0075, 0.0065, style::BUTTON, 0);
                        m.lathe(&CREAM, gg.at(u, mid - 0.005), gg.n(), &[[0.0055, 0.0], [0.0055, 0.008], [0.004, 0.0095], [0.0, 0.0095]], 10, false);
                        m.lathe(&CREAM, gg.at(u, v0 + 0.025), gg.n(), &[[0.0055, 0.0], [0.0055, 0.008], [0.004, 0.0095], [0.0, 0.0095]], 10, false);
                    }
                    knob(m, &gg, cu + 0.05, v0 + 0.012, 0.006, 540.0 + ci as f32, true);
                }
                (1, 1) => {
                    readout(m, &gg, cu - 0.02, mid, 0.022, 0.007, screen::XPDR);
                    knob(m, &gg, cu + 0.04, mid, 0.0065, 550.0, true);
                    pbutton(m, &gg, cu - 0.06, mid, 0.0065, 0.006, style::BUTTON, sid("TEST"));
                }
                (1, 2) => {
                    // Weather radar: mode buttons, tilt and gain knobs.
                    for b in 0..5 {
                        pbutton(m, &gg, cu - 0.05 + b as f32 * 0.02, mid + 0.02, 0.0075, 0.0065, style::BUTTON, 0);
                    }
                    knob(m, &gg, cu - 0.03, mid - 0.018, 0.0075, 560.0, true);
                    knob(m, &gg, cu + 0.03, mid - 0.018, 0.0075, 561.0, true);
                    label(m, &gg, cu - 0.03, mid + 0.002, 0.01, 0.003, style::TEXT, sid("TILT"));
                    label(m, &gg, cu + 0.03, mid + 0.002, 0.01, 0.003, style::TEXT, sid("GAIN"));
                }
                (1, 3) => {
                    // Evacuation command: a red guarded switch, the horn shutoff, its light.
                    guarded(m, &gg, cu - 0.03, mid, true);
                    annunciator(m, &gg, cu + 0.015, mid + 0.01, 0.013, 0.007, sid("EVAC"), false);
                    rbutton(m, &gg, cu + 0.05, mid, 0.006, &BTN);
                }
                _ => populate(m, &gg, cu - 0.068, v0 + 0.004, cu + 0.068, v1 - 0.004, 570.0 + (ci * 10 + k) as f32),
            }
        }
    }
    // The cabin interphone handset at the very aft end.
    let hz = za - 0.06;
    let hy = top_at(hz) + 0.012;
    m.rbox(&mt(mat::PLASTIC, 0.55, 0.53, 0.5), Vec3::new(0.0, hy, hz), Vec3::new(0.10, 0.012, 0.028), 0.01, Quat::IDENTITY);
    m.tube(&BTN, &[Vec3::new(0.09, hy, hz), Vec3::new(0.12, hy - 0.03, hz + 0.02), Vec3::new(0.15, hy - 0.12, hz + 0.01)], &|_| 0.003, 6, true);
}

// ---------------------------------------------------------------- moving controls

fn levers(b: &mut Builder) {
    let f = main_face();
    let base = f.at(0.0, -0.43);
    let zf = base.z + 0.01;
    let zm = EYE.z - 0.33;
    let ytq = EYE.y - 0.80;
    let qc = Vec3::new(0.0, ytq - 0.16, (zf + 0.03 + zm + 0.02) * 0.5);
    // The four thrust levers: a forward-bent arm, the cream knob, the
    // reverse lever riding on its front; numbered 1 to 4 left to right.
    for k in 0..4 {
        let x = -0.075 + k as f32 * 0.05;
        let mut m = Mesh::default();
        let piv = Vec3::new(x, qc.y, qc.z);
        let arm = [piv + Vec3::new(0.0, 0.20, 0.0), piv + Vec3::new(0.0, 0.30, 0.012), piv + Vec3::new(0.0, 0.345, -0.005)];
        m.rbox(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), piv + Vec3::new(0.0, 0.25, 0.006), Vec3::new(0.007, 0.06, 0.03), 0.004, Quat::from_rotation_x(0.1));
        m.tube(&ALU, &arm, &|_| 0.0055, 8, false);
        let kc = piv + Vec3::new(0.0, 0.352, -0.01);
        m.lathe(&mt(mat::PLASTIC, 0.70, 0.67, 0.58), kc - Vec3::X * 0.024, Vec3::X, &[[0.0, 0.0], [0.016, 0.0], [0.018, 0.004], [0.014, 0.012], [0.013, 0.024], [0.014, 0.036], [0.018, 0.044], [0.016, 0.048], [0.0, 0.048]], 18, true);
        b.add("thrust lever", G_ENGINE, m, Anim::hinge(Drive::new(Input::Throttle, -0.55).offset(0.28)), piv, Vec3::X, None);
        let mut r = Mesh::default();
        let rp = piv + Vec3::new(0.0, 0.27, -0.03);
        r.rbox(&BTN, rp + Vec3::new(0.0, 0.02, -0.005), Vec3::new(0.006, 0.03, 0.006), 0.003, Quat::IDENTITY);
        r.rbox(&BTN.pat(pat::KNURL), rp + Vec3::new(0.0, 0.05, -0.012), Vec3::new(0.012, 0.01, 0.008), 0.004, Quat::IDENTITY);
        b.add("reverse lever", G_ENGINE, r, Anim::hinge2(Drive::new(Input::Throttle, -0.55).offset(0.28), Drive::new(Input::Reverser, 0.9)), piv, Vec3::X, None);
    }
    // Speedbrake lever (left of the quadrant) and flap lever (right).
    let mut s = Mesh::default();
    let sp = Vec3::new(-0.125, qc.y, qc.z);
    s.tube(&ALU, &[sp + Vec3::Y * 0.2, sp + Vec3::new(0.0, 0.32, 0.0)], &|_| 0.004, 8, false);
    s.rbox(&mt(mat::PLASTIC, 0.72, 0.70, 0.66), sp + Vec3::new(0.0, 0.335, -0.004), Vec3::new(0.008, 0.032, 0.008), 0.006, Quat::from_rotation_x(-0.2));
    b.add("speedbrake lever", G_ENGINE, s, Anim::hinge(Drive::new(Input::Spoiler(-1.0), 0.75).offset(-0.38)), sp, Vec3::X, None);
    let mut fl = Mesh::default();
    let fp = Vec3::new(0.125, qc.y, qc.z);
    fl.tube(&ALU, &[fp + Vec3::Y * 0.2, fp + Vec3::new(0.0, 0.31, 0.0)], &|_| 0.0045, 8, false);
    fl.rbox(&mt(mat::PLASTIC, 0.72, 0.70, 0.66), fp + Vec3::new(0.0, 0.322, 0.0), Vec3::new(0.016, 0.012, 0.022), 0.006, Quat::IDENTITY);
    b.add("flap lever", G_ENGINE, fl, Anim::hinge(Drive::new(Input::FlapLever, 0.78).offset(-0.38)), fp, Vec3::X, None);

    // The gear lever: a wheel-shaped knob on an arm that moves up and down.
    let f2 = main_face();
    let gp = f2.at_h(0.150, 0.0, 0.03);
    let mut g = Mesh::default();
    g.tube(&ALU, &[gp - f2.n() * 0.02, gp + f2.n() * 0.03], &|_| 0.005, 8, false);
    g.lathe(&mt(mat::PLASTIC, 0.08, 0.075, 0.07), gp + f2.n() * 0.03 - Vec3::X * 0.014, Vec3::X, &[[0.0, 0.0], [0.016, 0.0], [0.02, 0.004], [0.02, 0.024], [0.016, 0.028], [0.0, 0.028]], 20, false);
    b.add("gear lever", G_ENGINE, g, Anim::slide(Drive::new(Input::GearHandle, -0.055), f2.ay), gp, Vec3::X, None);
}

fn columns(b: &mut Builder) {
    for sx in [-1.0f32, 1.0] {
        let x = sx * EYE.x.abs();
        let base = Vec3::new(x, FLOOR, EYE.z - 0.62);
        // The column, hinged at the floor.
        let mut c = Mesh::default();
        let top = Vec3::new(x, EYE.y - 0.58, EYE.z - 0.50);
        c.lathe(&COLUMN, base, Vec3::Y, &[[0.065, 0.0], [0.065, 0.02], [0.05, 0.04], [0.045, 0.06]], 18, false);
        c.tube(&COLUMN, &[base + Vec3::Y * 0.05, top - Vec3::Y * 0.03], &|t| 0.042 - 0.006 * t, 16, false);
        c.lathe(&mt(mat::PLASTIC, 0.06, 0.06, 0.06).pat(pat::KNURL), base + Vec3::Y * 0.06, Vec3::Y, &[[0.047, 0.0], [0.047, 0.08], [0.044, 0.09]], 18, false);
        c.rbox(&COLUMN, top, Vec3::new(0.04, 0.035, 0.045), 0.02, Quat::IDENTITY);
        let ci = b.add("control column", G_ENGINE, c, Anim::hinge(Drive::new(Input::Pitch, 0.09)), base, Vec3::X, None);
        // The yoke: a wheel of two horns on a hub, a clipboard on top.
        let mut y = Mesh::default();
        let hub = top + Vec3::new(0.0, 0.03, 0.04);
        let n = Vec3::new(0.0, 0.25, 1.0).normalize();
        let rot = Quat::from_rotation_arc(Vec3::Z, n);
        y.rbox(&COLUMN, hub, Vec3::new(0.06, 0.05, 0.03), 0.02, rot);
        for s in [-1.0f32, 1.0] {
            let pts: Vec<Vec3> = [(0.03, -0.035), (0.10, -0.05), (0.15, -0.035), (0.172, 0.01), (0.168, 0.06), (0.152, 0.095)].iter().map(|&(a, c)| hub + rot * Vec3::new(s * a, c, 0.0)).collect();
            y.tube(&COLUMN, &pts, &|t| 0.017 + 0.004 * (t * PI).sin(), 12, true);
            // Push-to-talk, autopilot disconnect, trim switches on the grips.
            y.dome(&mt(mat::PLASTIC, 0.35, 0.03, 0.03), hub + rot * Vec3::new(s * 0.156, 0.098, 0.012), rot * Vec3::new(0.0, 1.0, 0.4), 0.006, 0.005, 10, 0.0);
            y.rbox(&BTN, hub + rot * Vec3::new(s * 0.152, 0.06, 0.018), Vec3::new(0.005, 0.009, 0.004), 0.002, rot);
        }
        // Checklist holder and the card.
        y.rbox(&mt(mat::ALU, 0.55, 0.55, 0.56), hub + rot * Vec3::new(0.0, 0.06, 0.03), Vec3::new(0.04, 0.008, 0.006), 0.003, rot);
        y.rbox(&mt(mat::PLASTIC, 0.78, 0.77, 0.72), hub + rot * Vec3::new(0.0, 0.10, 0.036), Vec3::new(0.038, 0.055, 0.0012), 0.001, rot);
        rect(&mut y, &legend_mat(style::PLACARD, 0, Vec3::new(0.78, 0.77, 0.72)), hub + rot * Vec3::new(0.0, 0.10, 0.0375), rot * Vec3::X, rot * Vec3::Y, 0.036, 0.053);
        y.disc(&mt(mat::ALU, 0.6, 0.6, 0.62), hub + rot * Vec3::new(0.0, 0.0, 0.031), n, rot * Vec3::Y, 0.016, 18);
        b.add("yoke", G_ENGINE, y, Anim::hinge(Drive::new(Input::Roll, -0.8)), hub, n, Some(ci));
        // Rudder pedals with their brake tops.
        for dx in [-0.10f32, 0.10] {
            let mut p = Mesh::default();
            let pv = Vec3::new(x + dx, FLOOR + 0.42, EYE.z - 1.02);
            let pc = Vec3::new(x + dx, FLOOR + 0.18, EYE.z - 0.95);
            p.tube(&STEEL, &[pv, pc], &|_| 0.011, 8, true);
            let tilt = Quat::from_rotation_x(-0.35);
            p.rbox(&mt(mat::ALU, 0.35, 0.35, 0.36), pc + Vec3::new(0.0, 0.0, 0.01), Vec3::new(0.045, 0.085, 0.008), 0.006, tilt);
            p.rbox(&RUBBER.pat(pat::KNURL), pc + Vec3::new(0.0, 0.0, 0.019), Vec3::new(0.04, 0.075, 0.0025), 0.002, tilt);
            b.add("rudder pedal", G_ENGINE, p, Anim::hinge(Drive::new(Input::Yaw, dx.signum() * 0.18)), pv, Vec3::X, None);
        }
    }
    // The nose wheel tiller on the captain's side console.
    let mut t = Mesh::default();
    let tc = Vec3::new(-0.93, FLOOR + 0.62, EYE.z - 0.30);
    t.lathe(&mt(mat::PLASTIC, 0.05, 0.045, 0.04), tc, Vec3::Y, &[[0.012, 0.0], [0.012, 0.03], [0.07, 0.035], [0.075, 0.045], [0.07, 0.05], [0.0, 0.052]], 24, false);
    t.tube(&mt(mat::PLASTIC, 0.05, 0.045, 0.04), &[tc + Vec3::new(0.05, 0.05, 0.0), tc + Vec3::new(0.05, 0.09, 0.0)], &|_| 0.008, 8, true);
    b.add("tiller", G_ENGINE, t, Anim::hinge(Drive::new(Input::Steer, -1.8)), tc, Vec3::Y, None);
}

// ---------------------------------------------------------------- overhead panel

fn overhead(m: &mut Mesh) {
    // A box hanging from the ceiling from just behind the windshield to over
    // the seats: its forward face sloping, the aft one nearly level.
    let hw = 0.40;
    let z0 = bz(5.22);
    let zk = EYE.z + 0.18;
    let z1 = EYE.z + 0.95;
    let y0 = lin_top(z0, hw) - 0.06;
    let yk = (lin_top(zk, hw) - 0.13).min(y0 + 0.30);
    let y1 = yk + 0.04;
    // Seen from the seats the panels read with their tops aft (+X right,
    // up toward the tail), so they face down and back at the crew.
    let fwd = Face::new(Vec3::new(0.0, (y0 + yk) * 0.5, (z0 + zk) * 0.5), Vec3::X, Vec3::new(0.0, yk - y0, zk - z0));
    let aft = Face::new(Vec3::new(0.0, (yk + y1) * 0.5, (zk + z1) * 0.5), Vec3::X, Vec3::new(0.0, y1 - yk, z1 - zk));
    let lf = ((zk - z0).powi(2) + (yk - y0).powi(2)).sqrt() * 0.5;
    let la = ((z1 - zk).powi(2) + (y1 - yk).powi(2)).sqrt() * 0.5;
    // The box: sides up to the ceiling.
    for sx in [-1.0f32, 1.0] {
        let x = sx * (hw + 0.012);
        let pts = [Vec3::new(x, y0, z0), Vec3::new(x, yk, zk), Vec3::new(x, y1, z1)];
        for k in 0..2 {
            let (a, c) = (pts[k], pts[k + 1]);
            let (ta, tc) = (lin_top(a.z, x) + 0.02, lin_top(c.z, x) + 0.02);
            m.quad(&PANEL_DK, [a, c, Vec3::new(x, tc, c.z), Vec3::new(x, ta, a.z)]);
        }
    }
    m.quad(&PANEL_DK, [Vec3::new(-hw - 0.012, y1, z1), Vec3::new(hw + 0.012, y1, z1), Vec3::new(hw + 0.012, lin_top(z1, hw) + 0.02, z1), Vec3::new(-hw - 0.012, lin_top(z1, hw) + 0.02, z1)]);
    m.quad(&PANEL_DK, [Vec3::new(-hw - 0.012, y0, z0), Vec3::new(hw + 0.012, y0, z0), Vec3::new(hw + 0.012, lin_top(z0, hw) + 0.02, z0), Vec3::new(-hw - 0.012, lin_top(z0, hw) + 0.02, z0)]);
    m.rbox(&PANEL_DK, fwd.at_h(0.0, 0.0, -0.01), Vec3::new(hw + 0.012, lf, 0.01), 0.004, fwd.rot());
    m.rbox(&PANEL_DK, aft.at_h(0.0, 0.0, -0.01), Vec3::new(hw + 0.012, la, 0.01), 0.004, aft.rot());

    // Forward overhead, in the three columns of the real one (seen from the
    // captain's seat: electrical and hydraulics on the left, engine start,
    // fire handles, fuel and anti-ice in the middle, air conditioning and
    // pressurisation on the right), and the lights along its forward edge.
    // u runs right as the crew see it (the face's ax points left, flipped).
    let cw = 0.255;
    for (ci, cu) in [-0.265f32, 0.0, 0.265].iter().enumerate() {
        let (u0, u1) = (cu - cw * 0.5, cu + cw * 0.5);
        if ci == 1 {
            // Fire handles across the top, engine start knobs, fuel jettison, the fuel panel.
            plate(m, &fwd, u0, lf - 0.10, u1, lf - 0.005, 600.0);
            let g = fwd.sub(0.0, 0.0, 0.0043);
            for k in 0..4 {
                let u = u0 + 0.035 + k as f32 * 0.0615;
                m.rbox(&mt(mat::PLASTIC, 0.04, 0.04, 0.042), g.at_h(u, lf - 0.045, 0.012), Vec3::new(0.011, 0.028, 0.012), 0.005, g.rot());
                rect(m, &legend_mat(style::BUTTON, sid("DISCH"), Vec3::new(0.03, 0.03, 0.03)), g.at_h(u, lf - 0.045, 0.0242), g.ax, g.ay, 0.008, 0.02);
                label(m, &g, u, lf - 0.085, 0.014, 0.005, style::TEXT, sid(["1", "2", "3", "4"][k]));
            }
            plate(m, &fwd, u0, lf - 0.20, u1, lf - 0.10, 601.0);
            for k in 0..4 {
                let u = u0 + 0.035 + k as f32 * 0.0615;
                m.lathe(&BTN, g.at(u, lf - 0.125), g.n(), &[[0.011, 0.0], [0.011, 0.006], [0.009, 0.008], [0.009, 0.02], [0.0, 0.021]], 14, false);
                label(m, &g, u, lf - 0.108, 0.006, 0.0045, style::TEXT, sid(["1", "2", "3", "4"][k]));
                knob(m, &g, u, lf - 0.17, 0.0062, 610.0 + k as f32, true);
            }
            label(m, &g, *cu, lf - 0.145, 0.03, 0.004, style::TEXT, sid("ENG START"));
            // Fuel panel with its flow lines.
            plate(m, &fwd, u0, -lf + 0.11, u1, lf - 0.20, 602.0);
            label(m, &g, *cu, (-lf + 0.11 + lf - 0.20) * 0.5, cw * 0.46, (2.0 * lf - 0.31) * 0.46, style::LINES, 0);
            for r in 0..3 {
                for k in 0..6 {
                    let u = u0 + 0.03 + k as f32 * 0.039;
                    let v = lf - 0.235 - r as f32 * 0.045;
                    if v < -lf + 0.13 {
                        continue;
                    }
                    pbutton(m, &g, u, v, 0.0105, 0.0095, if (r * 6 + k) % 7 == 3 { style::BUTTON_ON } else { style::BUTTON }, if r == 0 { sid("X FEED") } else { sid("PUMPS") });
                }
            }
            plate(m, &fwd, u0, -lf + 0.005, u1, -lf + 0.11, 603.0);
            populate(m, &g, u0 + 0.004, -lf + 0.009, u1 - 0.004, -lf + 0.106, 604.0);
        } else {
            panel_run(m, &fwd, u0, -lf + 0.005, u1, lf - 0.005, 1, 5, 620.0 + ci as f32 * 31.0);
        }
    }
    // The lights strip along the forward edge.
    let g = fwd.sub(0.0, 0.0, 0.0043);
    let _ = g;
    // Aft overhead: circuit breakers and the less used panels.
    for (ci, cu) in [-0.265f32, 0.0, 0.265].iter().enumerate() {
        let (u0, u1) = (cu - cw * 0.5, cu + cw * 0.5);
        if ci == 1 {
            panel_run(m, &aft, u0, -la + 0.005, u1, la - 0.005, 1, 4, 700.0);
        } else {
            plate(m, &aft, u0, -la + 0.005, u1, la - 0.005, 710.0 + ci as f32);
            let g = aft.sub(0.0, 0.0, 0.0043);
            breakers(m, &g, u0 + 0.012, -la + 0.02, u1 - 0.012, la - 0.02, 720.0 + ci as f32);
        }
    }
    // The standby compass hanging from the top of the centre post.
    let cp = Vec3::new(0.0, y0 - 0.02, z0 - 0.025);
    m.rbox(&BTN, cp, Vec3::new(0.035, 0.028, 0.03), 0.01, Quat::IDENTITY);
    rect(m, &mt(mat::GAUGE, 0.02, 0.02, 0.02).x(17.0), cp + Vec3::new(0.0, 0.0, 0.0302), Vec3::X, Vec3::Y, 0.024, 0.016);
    m.tube(&BTN, &[cp + Vec3::Y * 0.02, cp + Vec3::new(0.0, 0.07, -0.01)], &|_| 0.006, 8, false);
    // Air outlets, flood lights and speakers beside the overhead.
    for sx in [-1.0f32, 1.0] {
        for (k, dz) in [0.0f32, 0.35, 0.7].iter().enumerate() {
            let z = zk - 0.2 + dz;
            let x = sx * (hw + 0.12);
            let y = lin_top(z, x) - 0.012;
            let p = Vec3::new(x, y, z);
            if k == 1 {
                m.lathe(&mt(mat::PLASTIC, 0.35, 0.33, 0.30), p, Vec3::NEG_Y, &[[0.032, 0.0], [0.032, 0.008], [0.026, 0.016], [0.0, 0.017]], 18, true);
                m.lathe(&CHROME, p - Vec3::Y * 0.016, Vec3::NEG_Y, &[[0.012, 0.0], [0.012, 0.01], [0.0, 0.012]], 12, true);
            } else {
                m.lathe(&mt(mat::PLASTIC, 0.30, 0.28, 0.26), p, Vec3::new(-sx * 0.3, -1.0, 0.0), &[[0.022, 0.0], [0.022, 0.012], [0.018, 0.02], [0.0, 0.02]], 16, false);
            }
        }
        // A map light and a speaker grille in the ceiling behind the side windows.
        let p = Vec3::new(sx * 0.9, lin_top(EYE.z + 0.3, 0.9) - 0.01, EYE.z + 0.3);
        let p = inside(p, 0.012);
        m.rbox(&mt(mat::PLASTIC, 0.26, 0.24, 0.22), p, Vec3::new(0.07, 0.008, 0.05), 0.006, Quat::IDENTITY);
    }
}

/// Rows of circuit breakers with their labels.
fn breakers(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let (pu, pv) = (0.0175, 0.028);
    let nu = ((u1 - u0) / pu) as usize;
    let nv = ((v1 - v0) / pv) as usize;
    for j in 0..nv {
        let v = v1 - (j as f32 + 0.6) * pv;
        label(m, f, (u0 + u1) * 0.5, v + 0.013, (u1 - u0) * 0.48, 0.0026, style::TEXT, 0);
        for i in 0..nu {
            let u = u0 + (i as f32 + 0.5) * pu;
            if hash(seed + (j * 31 + i) as f32) < 0.08 {
                continue;
            }
            let c = f.at(u, v);
            m.lathe(&CB_BLACK, c, f.n(), &[[0.0052, 0.0], [0.0052, 0.0016], [0.0038, 0.0022], [0.0038, 0.0085], [0.0032, 0.0095], [0.0, 0.0095]], 8, false);
            // The white collar shows when a breaker has popped.
            if hash(seed * 3.3 + (j * 31 + i) as f32) < 0.015 {
                m.lathe(&PAINT_WHITE, c + f.n() * 0.004, f.n(), &[[0.0039, 0.0], [0.0039, 0.003]], 8, false);
            }
        }
    }
}

// ---------------------------------------------------------------- seats and side consoles

fn seats(m: &mut Mesh) {
    for sx in [-1.0f32, 1.0] {
        let x = sx * EYE.x.abs();
        let zc = EYE.z;
        // Pedestal on the tracks, the pan, the cushion with its sheepskin.
        m.rbox(&SEAT_SHELL, Vec3::new(x, FLOOR + 0.13, zc + 0.05), Vec3::new(0.17, 0.12, 0.22), 0.03, Quat::IDENTITY);
        m.rbox(&SEAT_SHELL, Vec3::new(x, FLOOR + 0.30, zc + 0.02), Vec3::new(0.25, 0.05, 0.25), 0.03, Quat::IDENTITY);
        let tilt = Quat::from_rotation_x(-0.06);
        m.rbox(&FLEECE, Vec3::new(x, FLOOR + 0.415, zc - 0.02), Vec3::new(0.24, 0.06, 0.24), 0.05, tilt);
        // The back, leaning back a little, and the headrest.
        let lean = Quat::from_rotation_x(0.14);
        let bc = Vec3::new(x, FLOOR + 0.82, zc + 0.29);
        m.rbox(&SEAT_SHELL, bc + Vec3::new(0.0, 0.0, 0.055), Vec3::new(0.25, 0.37, 0.04), 0.035, lean);
        m.rbox(&FLEECE, bc, Vec3::new(0.235, 0.35, 0.05), 0.045, lean);
        m.rbox(&SEAT_SIDE, Vec3::new(x, FLOOR + 1.30, zc + 0.33), Vec3::new(0.14, 0.09, 0.05), 0.04, lean);
        // Armrests, inboard and outboard.
        for ax in [-1.0f32, 1.0] {
            let p = Vec3::new(x + ax * 0.27, FLOOR + 0.62, zc + 0.02);
            m.rbox(&SEAT_SIDE, p, Vec3::new(0.035, 0.035, 0.17), 0.03, Quat::IDENTITY);
            m.rbox(&SEAT_SHELL, p + Vec3::new(0.0, -0.08, 0.14), Vec3::new(0.025, 0.08, 0.03), 0.015, Quat::IDENTITY);
        }
        // The harness: shoulder straps over the back, the lap belts on the cushion.
        for dx in [-0.09f32, 0.09] {
            let a = bc + Vec3::new(dx, 0.36, -0.02);
            let c = Vec3::new(x + dx * 1.6, FLOOR + 0.47, zc - 0.05);
            m.tube(&mt(mat::FABRIC, 0.08, 0.07, 0.05), &[a, a + Vec3::new(0.0, -0.02, -0.06), c], &|_| 0.012, 6, true);
        }
        m.rbox(&CHROME, Vec3::new(x, FLOOR + 0.48, zc - 0.10), Vec3::new(0.05, 0.008, 0.04), 0.008, Quat::IDENTITY);
    }
}

fn side_consoles(m: &mut Mesh) {
    for sx in [-1.0f32, 1.0] {
        let (z0, z1) = (EYE.z - 0.60, EYE.z + 0.55);
        let x0 = sx * 0.82;
        let x1 = sx * 1.14;
        let yt = FLOOR + 0.58;
        let ys = FLOOR + 0.64;
        // The console's top, its inboard face, the wall behind it up to the
        // window sill and the sill out to the lining.
        m.quad(&PANEL, [Vec3::new(x0, yt, z0), Vec3::new(x1, yt, z0), Vec3::new(x1, yt, z1), Vec3::new(x0, yt, z1)]);
        m.quad(&PANEL_DK, [Vec3::new(x0, FLOOR, z0), Vec3::new(x0, FLOOR, z1), Vec3::new(x0, yt, z1), Vec3::new(x0, yt, z0)]);
        m.quad(&PANEL_DK, [Vec3::new(x0, FLOOR, z0), Vec3::new(x1, FLOOR, z0), Vec3::new(x1, yt, z0), Vec3::new(x0, yt, z0)]);
        m.tube(&PANEL, &[Vec3::new(x0, yt - 0.01, z0), Vec3::new(x0, yt - 0.01, z1)], &|_| 0.012, 8, true);
        let wall = mt(mat::PLASTIC, 0.26, 0.245, 0.225).pat(pat::PANEL);
        m.quad(&wall, [Vec3::new(x1, yt, z0), Vec3::new(x1, ys, z0), Vec3::new(x1, ys, z1), Vec3::new(x1, yt, z1)]);
        m.patch(&wall, 24, 4, sx < 0.0, |a, c| {
            let z = z0 - 0.15 + (z1 - z0 + 0.35) * a;
            let xw = lin_x(z, ys) - 0.004;
            let x = x1 + (sx * xw - x1) * c;
            (Vec3::new(x, ys, z), [z, x], 0.0)
        });
        m.tube(&wall, &[Vec3::new(x1, ys, z0 - 0.15), Vec3::new(x1, ys, z1 + 0.2)], &|_| 0.012, 8, true);
        // On it: the oxygen mask box with its doors, the hand mic, the chart
        // holder, a cup holder; the flight deck light controls.
        let f = Face::new(Vec3::new(x0 + sx * 0.16, yt, EYE.z), Vec3::new(0.0, 0.0, -sx), Vec3::new(sx, 0.0, 0.0));
        let f = if f.n().y < 0.0 { Face::new(f.o, -f.ax, f.ay) } else { f };
        m.rbox(&mt(mat::PLASTIC, 0.12, 0.10, 0.085), f.at_h(-0.33, 0.0, 0.04), Vec3::new(0.10, 0.08, 0.04), 0.008, f.rot());
        label_c(m, &f, -0.33, 0.0, 0.05, 0.012, style::RED_PLACARD, sid("PUSH"), Vec3::new(0.12, 0.10, 0.085));
        rect(m, &legend_mat(style::BUTTON, sid("TEST"), Vec3::new(0.02, 0.02, 0.02)), f.at_h(-0.33, -0.05, 0.0804), f.ax, f.ay, 0.014, 0.008);
        plate(m, &f, 0.05, -0.08, 0.25, 0.08, 800.0 + sx);
        populate(m, &f.sub(0.0, 0.0, 0.0043), 0.054, -0.076, 0.246, 0.076, 801.0 + sx);
        m.lathe(&BTN, f.at(-0.15, 0.06), f.n(), &[[0.035, 0.0], [0.035, 0.06], [0.03, 0.06], [0.03, 0.005], [0.0, 0.005]], 18, false);
        m.rbox(&BTN, f.at_h(-0.10, -0.05, 0.02), Vec3::new(0.025, 0.04, 0.02), 0.012, f.rot());
    }
}

// ---------------------------------------------------------------- behind the seats

fn aft_deck(m: &mut Mesh) {
    let zb = bz(D_BACK);
    // The bulkhead at the back with the door to the upper deck.
    let s = lsec(zb);
    let ring: Vec<Vec3> = (0..64)
        .map(|k| {
            let th = k as f32 / 64.0 * TAU;
            let q = sec_pt(&s, if th > PI { TAU - th } else { th });
            Vec3::new(if th > PI { -q.x } else { q.x }, q.y.max(FLOOR - 0.01), zb)
        })
        .collect();
    m.fan(&mt(mat::PLASTIC, 0.40, 0.38, 0.34), Vec3::new(0.0, FLOOR + 1.0, zb), &ring, Vec3::NEG_Z);
    let door = Face::new(Vec3::new(0.25, FLOOR + 0.98, zb - 0.004), Vec3::NEG_X, Vec3::Y);
    m.rbox(&mt(mat::PLASTIC, 0.30, 0.28, 0.25), door.at_h(0.0, 0.0, 0.01), Vec3::new(0.40, 0.96, 0.012), 0.01, door.rot());
    m.rbox(&CHROME, door.at_h(0.32, 0.0, 0.03), Vec3::new(0.012, 0.06, 0.012), 0.008, door.rot());
    m.disc(&mt(mat::GLASS, 0.0, 0.0, 0.0), door.at_h(0.0, 0.45, 0.023), door.n(), Vec3::Y, 0.03, 16);
    m.lathe(&CHROME, door.at_h(0.0, 0.45, 0.022), door.n(), &[[0.035, 0.0], [0.035, 0.003], [0.03, 0.004]], 16, false);

    // Observer seats: the first behind the pedestal, the second against the right wall.
    for (k, p) in [Vec3::new(-0.05, FLOOR, EYE.z + 1.12), Vec3::new(0.95, FLOOR, EYE.z + 1.35)].iter().enumerate() {
        let q = if k == 1 { Quat::from_rotation_y(-0.9) } else { Quat::IDENTITY };
        m.rbox(&SEAT_SHELL, *p + q * Vec3::new(0.0, 0.22, 0.0), Vec3::new(0.12, 0.2, 0.12), 0.02, q);
        m.rbox(&SEAT_SIDE, *p + q * Vec3::new(0.0, 0.46, 0.0), Vec3::new(0.22, 0.05, 0.21), 0.04, q);
        m.rbox(&SEAT_SIDE, *p + q * Vec3::new(0.0, 0.85, 0.2), Vec3::new(0.21, 0.32, 0.05), 0.04, q * Quat::from_rotation_x(0.1));
        m.rbox(&SEAT_SIDE, *p + q * Vec3::new(0.0, 1.25, 0.24), Vec3::new(0.12, 0.08, 0.05), 0.035, q);
    }
    // The circuit breaker panels on the walls behind each pilot.
    for sx in [-1.0f32, 1.0] {
        let (za, zb2) = (EYE.z + 0.62, EYE.z + 1.55);
        let y0 = FLOOR + 0.55;
        let y1 = FLOOR + 1.45;
        let zc = (za + zb2) * 0.5;
        let yc = (y0 + y1) * 0.5;
        let xw = lin_x(zc, yc) - 0.03;
        let f = Face::new(Vec3::new(sx * xw, yc, zc), Vec3::new(0.0, 0.0, -sx), Vec3::Y);
        let f = if f.n().x * sx > 0.0 { Face::new(f.o, -f.ax, f.ay) } else { f };
        let hz = (zb2 - za) * 0.5;
        let hy = (y1 - y0) * 0.5;
        m.rbox(&PANEL_DK, f.at_h(0.0, 0.0, -0.02), Vec3::new(hz, hy, 0.025), 0.006, f.rot());
        for r in 0..3 {
            let (v0, v1) = (-hy + r as f32 * hy * 2.0 / 3.0, -hy + (r + 1) as f32 * hy * 2.0 / 3.0);
            plate(m, &f, -hz, v0, hz, v1, 900.0 + sx * 10.0 + r as f32);
            breakers(m, &f.sub(0.0, 0.0, 0.0043), -hz + 0.015, v0 + 0.012, hz - 0.015, v1 - 0.012, 910.0 + sx + r as f32);
        }
    }
    // On the bulkhead: a halon extinguisher in its bracket, the crash axe,
    // a portable oxygen bottle, smoke goggles; placards by the door.
    let zf = zb - 0.06;
    let ext = Vec3::new(-0.55, FLOOR + 0.55, zf);
    m.tube(&mt(mat::PAINT, 0.42, 0.03, 0.02), &[ext, ext + Vec3::Y * 0.32], &|t| if t > 0.9 { 0.04 } else { 0.055 }, 18, true);
    m.lathe(&BTN, ext + Vec3::Y * 0.32, Vec3::Y, &[[0.03, 0.0], [0.02, 0.05], [0.012, 0.06]], 12, true);
    m.rbox(&BTN, ext + Vec3::new(0.02, 0.39, 0.0), Vec3::new(0.04, 0.008, 0.012), 0.004, Quat::IDENTITY);
    for t in [0.08f32, 0.24] {
        m.rbox(&ALU, ext + Vec3::new(0.0, t, 0.03), Vec3::new(0.065, 0.012, 0.012), 0.004, Quat::IDENTITY);
    }
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.85, 0.83, 0.75)), ext + Vec3::new(0.0, 0.17, -0.057), Vec3::NEG_X, Vec3::Y, 0.03, 0.05);
    let axe = Vec3::new(-0.80, FLOOR + 0.9, zf + 0.01);
    m.tube(&mt(mat::PAINT, 0.6, 0.45, 0.05), &[axe, axe + Vec3::new(0.0, 0.38, 0.0)], &|_| 0.014, 10, true);
    m.rbox(&mt(mat::PAINT, 0.45, 0.03, 0.02), axe + Vec3::new(0.05, 0.36, 0.0), Vec3::new(0.06, 0.035, 0.006), 0.004, Quat::IDENTITY);
    let ox = Vec3::new(0.62, FLOOR + 0.5, zf);
    m.tube(&mt(mat::PAINT, 0.12, 0.35, 0.12), &[ox, ox + Vec3::Y * 0.36], &|t| if t > 0.92 { 0.03 } else { 0.05 }, 18, true);
    m.lathe(&CHROME, ox + Vec3::Y * 0.36, Vec3::Y, &[[0.022, 0.0], [0.022, 0.03], [0.0, 0.035]], 10, true);
    for (k, y) in [1.55f32, 1.45].iter().enumerate() {
        rect(m, &legend_mat(if k == 0 { style::RED_PLACARD } else { style::TEXT }, 0, Vec3::new(0.82, 0.8, 0.74)), Vec3::new(0.25, FLOOR + y, zb - 0.02), Vec3::NEG_X, Vec3::Y, 0.09, 0.025);
    }
    // Door frame trim.
    let dz = zb - 0.004;
    for (a, b) in [(Vec3::new(-0.17, FLOOR, dz), Vec3::new(-0.17, FLOOR + 1.96, dz)), (Vec3::new(0.67, FLOOR, dz), Vec3::new(0.67, FLOOR + 1.96, dz)), (Vec3::new(-0.17, FLOOR + 1.96, dz), Vec3::new(0.67, FLOOR + 1.96, dz))] {
        m.tube(&mt(mat::PLASTIC, 0.22, 0.21, 0.19), &[a, b], &|_| 0.02, 8, true);
    }
    // Life vests and flashlights in pouches under the observer seats.
    m.rbox(&mt(mat::FABRIC, 0.55, 0.42, 0.05), Vec3::new(-0.05, FLOOR + 0.06, EYE.z + 1.12), Vec3::new(0.12, 0.05, 0.08), 0.03, Quat::IDENTITY);
    // Coat hooks and a flashlight on the left wall behind the captain.
    let z = EYE.z + 0.75;
    let x = -(lin_x(z, FLOOR + 1.6) - 0.02);
    m.tube(&mt(mat::PAINT, 0.4, 0.38, 0.05), &[Vec3::new(x + 0.03, FLOOR + 1.55, z), Vec3::new(x + 0.03, FLOOR + 1.75, z)], &|_| 0.02, 12, true);
}

// ---------------------------------------------------------------- outside

/// The windshield wipers, parked along the bottom of each windshield.
pub fn wipers(m: &mut Mesh) {
    let a = Vec2::new(WSHIELD[0][0], WSHIELD[0][1]);
    let b = Vec2::new(WSHIELD[1][0], WSHIELD[1][1]);
    let on_skin = |t: f32, lift: f32| {
        let p = a.lerp(b, t) - Vec2::new(0.0, 0.085);
        let z = station_at(p.x, p.y, false);
        let s = sec(z);
        // Outward normal of the section there.
        let th = {
            let (mut lo, mut hi) = (0.0f32, PI);
            for _ in 0..24 {
                let m = (lo + hi) * 0.5;
                if sec_pt(&s, m).y > p.y { lo = m } else { hi = m }
            }
            (lo + hi) * 0.5
        };
        let n = fuse_n(z, th);
        Vec3::new(p.x, p.y, z) + n * lift
    };
    let mut w = Mesh::default();
    let piv = on_skin(0.86, 0.0);
    let n = (on_skin(0.86, 0.1) - piv).normalize();
    w.lathe(&BLACK, piv, n, &[[0.035, 0.0], [0.035, 0.03], [0.025, 0.045], [0.0, 0.05]], 14, false);
    let arm: Vec<Vec3> = [0.84, 0.6, 0.35, 0.18].iter().map(|&t| on_skin(t, 0.045)).collect();
    w.tube(&BLACK, &arm, &|t| 0.012 - 0.004 * t, 8, true);
    let blade: Vec<Vec3> = [0.66, 0.4, 0.14, 0.06].iter().map(|&t| on_skin(t, 0.022)).collect();
    w.tube(&RUBBER, &blade, &|_| 0.009, 6, true);
    m.append(&w.mirror_x());
    m.append(&w);
}

// ---------------------------------------------------------------- what the eye sees

/// Whether a look from `eye` along `d` (body frame) ends on something on the
/// flight deck (true) or goes out through a window (false).
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let f = main_face();
    let n = f.n();
    let mut t = 0.02;
    while t < 4.0 {
        let p = eye + d * t;
        if p.y < FLOOR + 0.01 {
            return true;
        }
        // The panel and the glareshield, the pedestal.
        let top = EYE.y - 0.17 - 0.2 * (EYE.z - 0.70 - p.z).max(0.0);
        if (p - f.o).dot(n) < 0.02 && p.y < top {
            return true;
        }
        if p.x.abs() < 0.30 && p.z < EYE.z + 0.86 && p.y < EYE.y - 0.78 {
            return true;
        }
        // The overhead panel.
        if p.x.abs() < 0.41 && p.z > bz(5.22) && p.z < EYE.z + 0.95 && p.y > EYE.y + 0.42 {
            return true;
        }
        let s = lsec(p.z);
        if p.y >= s.top || p.x.abs() >= lin_x(p.z, p.y) || p.z < bz(D_FRONT) || p.z > bz(D_BACK) {
            return window(p) >= TRIM * 0.5;
        }
        t += 0.02 + t * 0.012;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_resolve() {
        for s in STRINGS.iter().skip(1) {
            assert!(s.chars().count() <= 15, "{s} too long");
            for c in s.chars() {
                assert!(glyph(c) != 15 || c == ' ', "no glyph for {c:?} in {s}");
            }
        }
        let w = strings_wgsl();
        assert!(w.contains("vec4u("));
    }

    #[test]
    fn plane_shader_validates() {
        let src = format!("{}\n{}\n{}\n{}\n{}", include_str!("../../shaders/common.wgsl"), include_str!("../../shaders/plane.wgsl"), strings_wgsl(), include_str!("../../shaders/deck747.wgsl"), include_str!("../../shaders/pit18.wgsl"));
        let m = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        if let Err(e) = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m) {
            panic!("{}", e.emit_to_string(&src));
        }
    }

    #[test]
    fn view_over_the_nose() {
        // Straight ahead from the captain's eye: the windshield, and below it
        // the glareshield; the over-the-nose view.
        let mut down = 0.0f32;
        for k in 0..400 {
            let a = -(k as f32) * 0.001;
            let d = Vec3::new(0.0, a.sin(), -a.cos());
            if sees_inside(EYE, d) {
                down = a;
                break;
            }
        }
        let mut up = 0.0f32;
        for k in 0..900 {
            let a = k as f32 * 0.001;
            let d = Vec3::new(0.0, a.sin(), -a.cos());
            if sees_inside(EYE, d) {
                up = a;
                break;
            }
        }
        println!("captain sees out from {:.1} to {:.1} deg", down.to_degrees(), up.to_degrees());
        assert!(!sees_inside(EYE, Vec3::NEG_Z), "straight ahead is not out the windshield");
        assert!(down.to_degrees() < -10.0);
    }
}

