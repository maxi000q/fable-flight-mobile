//! Concorde's flight deck, for its crew of three: narrow, low under the
//! crown and walled with instruments. The two pilots sit either side of the
//! pedestal with its four throttles and the reheat switches behind them, each
//! behind a ram's-horn yoke and a panel of round dials in the classic T
//! (airspeed, attitude, altitude; Mach, heading, vertical speed); between
//! them the centre panel with the engines' instruments in four columns (rpm,
//! exhaust temperature, fuel flow, nozzle area), the landing gear lever and,
//! on its right, the lever for the nose and visor; the autopilot's controls
//! along the glare shield, an overhead panel between the pilots' heads. Behind
//! the first officer the flight engineer's station: a seat turned to face the
//! great panel of gauges and switches on the right wall (fuel, engines,
//! electrics, hydraulics, air), a circuit breaker panel on the wall opposite,
//! and the door to the cabin.
//!
//! The dials are faces of plane.wgsl's `gauge`, from the instrument readings
//! (one reading for all four engines); the switches, knobs, plates and
//! legends are the Hornet's (`fa18::pit`). The lining follows the skin of the
//! model (concorde.py, mirrored by `fuse_at`), with the windscreen's and the
//! side windows' openings left in it. Everything here is body frame (+X
//! right, +Y up, -Z forward).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, sid, style, Face};
use crate::aircraft::c172::cabin::gauge;
use crate::aircraft::fa18::pit as hp;
use glam::{Mat4, Quat, Vec2, Vec3};
use hp::{BEZEL, BLK, GLARE, GREEN_LENS, RED_LENS};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

// ---------------------------------------------------------------- materials

const LINER: Mat = mt(mat::SATIN, 0.36, 0.345, 0.30).pat(pat::PIT);
const LINER_DK: Mat = mt(mat::SATIN, 0.15, 0.148, 0.135).pat(pat::PIT);
const CARPET: Mat = mt(mat::FABRIC, 0.030, 0.040, 0.070).pat(pat::CARPET);
const FLEECE: Mat = mt(mat::FABRIC, 0.36, 0.34, 0.31).pat(pat::FLEECE);
const SEAT_SIDE: Mat = mt(mat::FABRIC, 0.05, 0.05, 0.055).pat(pat::SEAT);
const SEAT_SHELL: Mat = mt(mat::PLASTIC, 0.10, 0.095, 0.09);
const COLUMN: Mat = mt(mat::PLASTIC, 0.030, 0.030, 0.032);
const AMBER_LENS: Mat = mt(mat::LAMP, 1.0, 0.55, 0.05).x(lamp::ANNUNC);

// ---------------------------------------------------------------- where things are

/// The pilots' seats, either side of the centreline (the captain's is the eye's).
const SEAT_X: f32 = 0.42;
/// The floor: a seated pilot's eye is 1.30 m above it.
pub const FLOOR: f32 = EYE.y - 1.30;
/// The wall in front of the pedals, and the bulkhead with the cabin door.
const Z0: f32 = -25.22;
const Z1: f32 = -20.90;
/// How far the lining stands inside the skin.
const INSET: f32 = 0.045;
/// The openings in the skin (concorde.py): the windscreen's, over the crown
/// from this angle round the section to its mirror image between these
/// stations; two side windows each side, between these angles.
const WS_A: f32 = FRAC_PI_4;
const WS_Z: [f32; 2] = [-25.3, -23.7];
const SW_A: [f32; 2] = [0.3927, 0.8836];
const SW_Z: [[f32; 2]; 2] = [[-23.45, -23.02], [-22.94, -22.5]];

/// A point `inset` inside the skin at station z, at angle a round the section
/// (0 on the right at its widest, a quarter turn at the crown).
fn shell(z: f32, a: f32, inset: f32) -> Vec3 {
    let [_, hw, top, mid, bot] = fuse_at(-z);
    let (c, s) = (a.cos(), a.sin());
    let h = if s >= 0.0 { top - mid } else { mid - bot };
    Vec3::new((hw - inset) * c, mid + (h - inset) * s, z)
}
/// The angle at which the lining is at height y.
fn ang_at(z: f32, y: f32) -> f32 {
    let [_, _, top, mid, bot] = fuse_at(-z);
    let h = if y >= mid { top - mid } else { mid - bot };
    ((y - mid) / (h - INSET)).clamp(-1.0, 1.0).asin()
}
/// Half width of the lining at station z, height y; its height x out.
fn lin_x(z: f32, y: f32) -> f32 {
    shell(z, ang_at(z, y), INSET).x
}
fn lin_top(z: f32, x: f32) -> f32 {
    let [_, hw, top, mid, _] = fuse_at(-z);
    let c = (x.abs() / (hw - INSET)).min(1.0);
    mid + (top - mid - INSET) * (1.0 - c * c).sqrt()
}
/// How far outside the skin a point is, as a fraction of the section (negative: inside).
fn outside(p: Vec3) -> f32 {
    let [_, hw, top, mid, bot] = fuse_at(-p.z);
    let h = if p.y >= mid { top - mid } else { mid - bot };
    ((p.x / hw).powi(2) + ((p.y - mid) / h).powi(2)).sqrt() - 1.0
}

// ---------------------------------------------------------------- what sits on a panel

/// A round instrument in its black case, its face under the bezel's lip.
fn dial(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, id: f32, seed: f32) {
    // (The small ones, of which there are a hundred, are cut plainer.)
    let big = r > 0.03;
    let segs = if big { 32 } else { 18 };
    m.rbox(&BLK, f.at_h(u, v, 0.005), Vec3::new(r + 0.007, r + 0.007, 0.005), if big { 0.004 } else { 0.0 }, f.rot());
    let c = f.at_h(u, v, 0.0105);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, f.n(), f.ay, r, segs);
    m.lathe(&BLK, c, f.n(), &[[r, 0.0], [r, 0.005], [r + 0.003, 0.0065], [r + 0.006, 0.006], [r + 0.006, -0.0005]], segs, !big);
    if big {
        for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
            m.screw(&SCREW_BLACK, f.at_h(u + du * (r + 0.004), v + dv * (r + 0.004), 0.010), f.n(), 0.0018, seed + k as f32);
        }
    }
}
/// A panel's plate, standing proud with a screw in each corner (the Hornet's
/// `plate`, cut plainer: there are sixty of them here).
fn plate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let (cu, cv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let (hu, hv) = ((u1 - u0) * 0.5 - 0.0007, (v1 - v0) * 0.5 - 0.0007);
    m.rbox(&BLK, f.at_h(cu, cv, 0.0015), Vec3::new(hu, hv, 0.0028), 0.0, f.rot());
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(cu + du * (hu - 0.0055), cv + dv * (hv - 0.0055), 0.0044), f.n(), 0.0030, seed + k as f32);
    }
}
/// A row of n toggle switches on a plate, each with its legend above it; a
/// few under a red guard.
fn switches(m: &mut Mesh, f: &Face, u0: f32, u1: f32, v: f32, n: usize, seed: f32) {
    let g = f.sub(0.0, 0.0, 0.0043);
    let nrm = g.n();
    for i in 0..n {
        let u = u0 + (u1 - u0) * (i as f32 + 0.5) / n as f32;
        let h = hash(seed + i as f32 * 1.7);
        let c = g.at(u, v);
        m.lathe(&CHROME, c, nrm, &[[0.0046, 0.0], [0.0038, 0.0034], [0.0, 0.0038]], 6, true);
        let tip = c + nrm * 0.0140 + g.ay * if h > 0.4 { 0.0052 } else { -0.0052 };
        m.tube(&CHROME, &[c + nrm * 0.003, tip], &|t| 0.0016 + 0.0008 * t, 5, true);
        if h > 0.965 {
            m.rbox(&mt(mat::PAINT, 0.26, 0.025, 0.018), g.at_h(u, v - 0.002, 0.010), Vec3::new(0.0065, 0.012, 0.0070), 0.0, g.rot() * Quat::from_rotation_x(-0.12));
        }
        hp::fake(m, &g, u, v + 0.0125, 0.009, 0.0026);
    }
}
/// A row of n rotary knobs on a plate, each with its pointer and legend.
fn knobs(m: &mut Mesh, f: &Face, u0: f32, u1: f32, v: f32, n: usize, seed: f32) {
    let g = f.sub(0.0, 0.0, 0.0043);
    let nrm = g.n();
    for i in 0..n {
        let u = u0 + (u1 - u0) * (i as f32 + 0.5) / n as f32;
        let r = 0.0075;
        let c = g.at(u, v);
        m.lathe(&hp::KNOB, c, nrm, &[[r * 1.3, 0.0], [r * 1.3, r * 0.3], [r, r * 0.45], [r * 0.92, r * 1.5], [0.0, r * 1.6]], 12, true);
        let rot = g.rot() * Quat::from_rotation_z((hash(seed + i as f32 * 2.3) - 0.5) * 2.6);
        m.rbox(&hp::INK, c + nrm * (r * 1.6) + rot * Vec3::new(0.0, r * 0.45, 0.0), Vec3::new(r * 0.12, r * 0.45, 0.0003), 0.0, rot);
        hp::fake(m, &g, u, v + 0.0165, 0.011, 0.0026);
    }
}
/// A plate of knobs and switches in alternate rows, under its title.
fn mixed_panel(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, rows: usize, cols: usize, seed: f32, title: &str) {
    plate(m, f, u0, v0, u1, v1, seed);
    hp::word(m, &f.sub(0.0, 0.0, 0.0043), (u0 + u1) * 0.5, v1 - 0.008, 0.0034, title);
    let h = (v1 - 0.016 - v0) / rows as f32;
    for r in 0..rows {
        let v = v0 + h * (r as f32 + 0.42);
        if r % 2 == 0 {
            knobs(m, f, u0 + 0.008, u1 - 0.008, v, (cols * 2).div_ceil(3), seed + r as f32 * 11.0);
        } else {
            switches(m, f, u0 + 0.008, u1 - 0.008, v, cols, seed + r as f32 * 11.0);
        }
    }
}
/// A plate of switches in rows, under its title.
fn switch_panel(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, rows: usize, cols: usize, seed: f32, title: &str) {
    plate(m, f, u0, v0, u1, v1, seed);
    hp::word(m, &f.sub(0.0, 0.0, 0.0043), (u0 + u1) * 0.5, v1 - 0.008, 0.0034, title);
    let h = (v1 - 0.016 - v0) / rows as f32;
    for r in 0..rows {
        switches(m, f, u0 + 0.008, u1 - 0.008, v0 + h * (r as f32 + 0.42), cols, seed + r as f32 * 11.0);
    }
}

// ---------------------------------------------------------------- the flight deck

pub fn build(b: &mut Builder) {
    let mut l = Mesh::default();
    lining(&mut l);
    floor_and_walls(&mut l);
    b.fixed("flight deck lining", G_ENGINE, l);

    let mut p = Mesh::default();
    let f = panel_face();
    main_panel(&mut p, &f);
    glare_shield(&mut p);
    b.fixed("instrument panel", G_ENGINE, p);

    let mut c = Mesh::default();
    pedestal(&mut c);
    side_consoles(&mut c);
    b.fixed("pedestal", G_ENGINE, c);

    let mut o = Mesh::default();
    overhead(&mut o);
    b.fixed("overhead panel", G_ENGINE, o);

    let mut e = Mesh::default();
    engineer(&mut e);
    breakers(&mut e);
    b.fixed("engineer's panel", G_ENGINE, e);

    let mut s = Mesh::default();
    seat(&mut s, Vec3::new(-SEAT_X, FLOOR, EYE.z), 0.0, -1.0);
    seat(&mut s, Vec3::new(SEAT_X, FLOOR, EYE.z), 0.0, 1.0);
    // The engineer's, turned to face his panel.
    seat(&mut s, Vec3::new(ENG_SEAT.x, FLOOR, ENG_SEAT.z), -FRAC_PI_2, 0.0);
    b.fixed("crew seats", G_ENGINE, s);

    controls(b, &f);
}

// ---------------------------------------------------------------- lining, floor, walls

/// A band of the lining between two stations, from angle a0(z) round to a1(z).
fn band(m: &mut Mesh, z0: f32, z1: f32, a0: &dyn Fn(f32) -> f32, a1: &dyn Fn(f32) -> f32, nz: usize, na: usize) {
    m.patch(&LINER, nz, na, false, |u, v| {
        let z = z0 + (z1 - z0) * u;
        let a = a0(z) + (a1(z) - a0(z)) * v;
        (shell(z, a, INSET), [z, a], 0.0)
    });
}
/// The edge of an opening: from the lining out to the skin, along (station, angle) points.
fn reveal(m: &mut Mesh, pts: &[(f32, f32)], toward: Vec3) {
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        hp::quad_to(m, &LINER_DK, [shell(a.0, a.1, INSET), shell(b.0, b.1, INSET), shell(b.0, b.1, 0.004), shell(a.0, a.1, 0.004)], toward);
    }
}
fn run(a: (f32, f32), b: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
    }).collect()
}

fn lining(m: &mut Mesh) {
    // Where it meets the floor (a little under it), each side.
    let lo = |z: f32| ang_at(z, FLOOR - 0.03);
    let hi = |z: f32| PI - ang_at(z, FLOOR - 0.03);
    // Under the windscreen: the side walls up to its sill.
    band(m, Z0, WS_Z[1], &lo, &|_| WS_A, 22, 8);
    band(m, Z0, WS_Z[1], &|_| PI - WS_A, &hi, 22, 8);
    // Behind it, all the way over the crown, but for the side windows.
    let mut z = WS_Z[1];
    for w in SW_Z {
        band(m, z, w[0], &lo, &hi, 3, 40);
        band(m, w[0], w[1], &lo, &|_| SW_A[0], 4, 6);
        band(m, w[0], w[1], &|_| SW_A[1], &|_| PI - SW_A[1], 4, 22);
        band(m, w[0], w[1], &|_| PI - SW_A[0], &hi, 4, 6);
        z = w[1];
    }
    band(m, z, Z1, &lo, &hi, 16, 40);
    // The openings' edges.
    let mid = Vec3::new(0.0, 0.55, -24.5);
    for s in [0.0f32, 1.0] {
        let a = |x: f32| if s > 0.5 { PI - x } else { x };
        reveal(m, &run((Z0, a(WS_A)), (WS_Z[1], a(WS_A)), 12), mid);
        for w in SW_Z {
            let c = shell((w[0] + w[1]) * 0.5, a((SW_A[0] + SW_A[1]) * 0.5), INSET * 0.5);
            reveal(m, &run((w[0], a(SW_A[0])), (w[1], a(SW_A[0])), 3), c);
            reveal(m, &run((w[0], a(SW_A[1])), (w[1], a(SW_A[1])), 3), c);
            reveal(m, &run((w[0], a(SW_A[0])), (w[0], a(SW_A[1])), 5), c);
            reveal(m, &run((w[1], a(SW_A[0])), (w[1], a(SW_A[1])), 5), c);
            // A grab handle over each window.
            let (p, q) = (shell(w[0] + 0.08, a(SW_A[1] + 0.12), INSET), shell(w[1] - 0.08, a(SW_A[1] + 0.12), INSET));
            let inw = (Vec3::new(0.0, 0.3, p.z) - p).normalize() * 0.035;
            m.tube(&LINER_DK, &[p, p + inw, q + inw, q], &|_| 0.008, 8, true);
        }
    }
    reveal(m, &run((WS_Z[1], WS_A), (WS_Z[1], PI - WS_A), 24), mid);
}

fn floor_and_walls(m: &mut Mesh) {
    m.patch(&CARPET, 8, 30, true, |u, v| {
        let z = Z0 + (Z1 - Z0) * v;
        let x = (u * 2.0 - 1.0) * (lin_x(z, FLOOR) + 0.02);
        (Vec3::new(x, FLOOR, z), [x, z], 0.0)
    });
    // The wall ahead of the pedals, up under the glare shield.
    m.patch(&LINER_DK, 10, 6, false, |u, v| {
        let y = FLOOR - 0.02 + (0.36 - FLOOR) * v;
        (Vec3::new((u * 2.0 - 1.0) * (lin_x(Z0, y) + 0.02), y, Z0), [u, y], 0.0)
    });
    // The bulkhead behind, with the door to the cabin.
    m.patch(&LINER, 16, 10, true, |u, v| {
        let y = FLOOR - 0.02 + (lin_top(Z1, 0.0) + 0.03 - FLOOR) * v;
        let x = (u * 2.0 - 1.0) * (lin_x(Z1, y) + 0.02);
        (Vec3::new(x, y, Z1), [x, y], 0.0)
    });
    let d = Face::new(Vec3::new(-0.10, FLOOR + 0.92, Z1 - 0.004), -Vec3::X, Vec3::Y);
    let frame = [d.at(-0.31, -0.90), d.at(-0.31, 0.86), d.at(0.31, 0.86), d.at(0.31, -0.90)];
    m.tube(&LINER_DK, &frame, &|_| 0.012, 8, true);
    m.rbox(&LINER_DK, d.at_h(0.0, -0.02, 0.003), Vec3::new(0.30, 0.87, 0.003), 0.002, d.rot());
    m.rbox(&CHROME, d.at_h(0.24, 0.0, 0.03), Vec3::new(0.012, 0.05, 0.012), 0.006, d.rot());
    rect(m, &legend_mat(style::RED_PLACARD, sid("LOCK OVRD"), LINER_DK.col), d.at_h(0.0, 0.55, 0.0065), d.ax, d.ay, 0.05, 0.014);
}

// ---------------------------------------------------------------- the pilots' panel

/// The main panel across the flight deck, its top leaning away 12 degrees.
fn panel_face() -> Face {
    let t = 12f32.to_radians();
    Face::new(Vec3::new(0.0, 0.13, -24.05), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}

/// The centre panel's right-hand column: the landing gear lever's slot, and
/// under it the nose and visor lever's.
const GEAR_U: f32 = 0.172;
const GEAR_V: f32 = 0.105;
const NOSE_V: f32 = -0.055;

/// One pilot's instruments, the T centred at u.
fn flight_instruments(m: &mut Mesh, f: &Face, cu: f32, seed: f32) {
    let g = f.sub(0.0, 0.0, 0.0004);
    dial(m, f, cu - 0.125, 0.165, 0.040, gauge::ASI, seed);
    dial(m, f, cu, 0.165, 0.050, gauge::AI, seed + 1.0);
    dial(m, f, cu + 0.125, 0.165, 0.040, gauge::ALT, seed + 2.0);
    dial(m, f, cu - 0.125, 0.048, 0.038, gauge::MACH, seed + 3.0);
    dial(m, f, cu, 0.045, 0.050, gauge::HI, seed + 4.0);
    dial(m, f, cu + 0.125, 0.048, 0.038, gauge::VSI, seed + 5.0);
    dial(m, f, cu - 0.125, -0.070, 0.030, gauge::CLOCK, seed + 6.0);
    dial(m, f, cu, -0.072, 0.036, gauge::TC, seed + 7.0);
    dial(m, f, cu + 0.125, -0.070, 0.034, gauge::CDI, seed + 8.0);
    // Outboard of the T: the standby altimeter (the radio altimeter's place) and the air temperature.
    let out = if cu < 0.0 { cu - 0.235 } else { cu + 0.235 };
    dial(m, f, out, 0.052, 0.030, gauge::ALT, seed + 9.0);
    dial(m, f, out, -0.040, 0.028, gauge::OAT, seed + 10.0);
    hp::word(m, &g, out, 0.096, 0.0026, "RA");
    hp::word(m, &g, out, 0.000, 0.0026, "TAT");
    // The master warning and caution over the attitude indicator.
    hp::light(m, f, cu - 0.030, 0.234, 0.022, 0.008, sid("WARNING"), &RED_LENS);
    hp::light(m, f, cu + 0.030, 0.234, 0.022, 0.008, sid("CAUTION"), &AMBER_LENS);
    // Under them: the flight director's and the compass's switches.
    switch_panel(m, f, cu - 0.17, -0.268, cu + 0.17, -0.150, 2, 7, seed + 20.0, if cu < 0.0 { "CAPT" } else { "F/O" });
}

fn main_panel(m: &mut Mesh, f: &Face) {
    // The panel: narrower toward the top with the fuselage, taller in the
    // middle where the autopilot's controls sit under the glare shield.
    let outline = vec![
        Vec2::new(0.93, -0.28), Vec2::new(-0.93, -0.28), Vec2::new(-0.89, -0.10), Vec2::new(-0.82, 0.05), Vec2::new(-0.73, 0.18), Vec2::new(-0.67, 0.25),
        Vec2::new(-0.36, 0.25), Vec2::new(-0.36, 0.30), Vec2::new(0.36, 0.30), Vec2::new(0.36, 0.25), Vec2::new(0.67, 0.25), Vec2::new(0.73, 0.18), Vec2::new(0.82, 0.05), Vec2::new(0.89, -0.10),
    ];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    flight_instruments(m, f, -0.47, 100.0);
    flight_instruments(m, f, 0.47, 200.0);
    let g = f.sub(0.0, 0.0, 0.0004);

    // The engines, a column each: rpm, exhaust temperature, fuel flow, nozzle area.
    let cols = [-0.105f32, -0.045, 0.015, 0.075];
    for (r, (id, name)) in [(gauge::JET_RPM, "N2"), (gauge::JET_EGT, "EGT"), (gauge::JET_FF, "FF"), (gauge::NOZZLE, "NOZ")].iter().enumerate() {
        let v = 0.195 - r as f32 * 0.064;
        for (k, u) in cols.iter().enumerate() {
            dial(m, f, *u, v, 0.026, *id, 300.0 + (r * 4 + k) as f32);
        }
        hp::word(m, &g, 0.118, v, 0.0028, name);
    }
    for (k, u) in cols.iter().enumerate() {
        hp::word(m, &g, *u, 0.2345, 0.0030, ["1", "2", "3", "4"][k]);
    }
    // Left of them: the fuel on board, the standby horizon and airspeed.
    dial(m, f, -0.180, 0.180, 0.030, gauge::JET_FUEL, 320.0);
    hp::word(m, &g, -0.180, 0.224, 0.0028, "FUEL");
    dial(m, f, -0.180, 0.092, 0.032, gauge::AI, 321.0);
    dial(m, f, -0.180, 0.008, 0.030, gauge::ASI, 322.0);
    // Right of them: the landing gear's lights and lever (a piece of its own),
    // and the lever for the nose and visor.
    hp::word(m, &g, GEAR_U, 0.238, 0.0028, "GEAR");
    for (i, s) in ["NOSE", "LEFT", "RIGHT"].iter().enumerate() {
        hp::light(m, f, GEAR_U, 0.214 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    let slot = mt(mat::BLACK, 0.004, 0.004, 0.004);
    m.rbox(&slot, f.at_h(GEAR_U, GEAR_V, 0.0), Vec3::new(0.005, 0.036, 0.002), 0.002, f.rot());
    hp::word(m, &g, GEAR_U - 0.024, GEAR_V + 0.030, 0.0024, "UP");
    hp::word(m, &g, GEAR_U - 0.024, GEAR_V - 0.030, 0.0024, "DN");
    m.rbox(&slot, f.at_h(GEAR_U, NOSE_V, 0.0), Vec3::new(0.005, 0.058, 0.002), 0.002, f.rot());
    hp::word(m, &g, GEAR_U, NOSE_V + 0.068, 0.0028, "NOSE");
    // (its four detents: nose and visor up, visor down, 5 degrees, down)
    for (i, s) in ["UP", "", "5", "DN"].iter().enumerate() {
        let v = NOSE_V + 0.045 - i as f32 * 0.030;
        m.rbox(&hp::INK, f.at_h(GEAR_U - 0.011, v, 0.0005), Vec3::new(0.004, 0.0007, 0.0003), 0.0001, f.rot());
        if !s.is_empty() {
            hp::word(m, &g, GEAR_U - 0.026, v, 0.0024, s);
        }
    }
    // Under the engines: the warning lights, a row of switches.
    plate(m, f, -0.145, -0.268, 0.135, -0.040, 330.0);
    let gg = f.sub(0.0, 0.0, 0.0043);
    let names = ["ENG FIRE", "ENG", "HYD", "ELEC", "FUEL", "CAB ALT", "PACKS", "BLEED", "ANTI-ICE", "STAB", "DUCT", "SMOKE EVAC"];
    for (i, s) in names.iter().enumerate() {
        let (c, r) = (i % 4, i / 4);
        hp::light(m, &gg, -0.108 + c as f32 * 0.069, -0.068 - r as f32 * 0.030, 0.027, 0.010, sid(s), if r == 0 { &RED_LENS } else { &AMBER_LENS });
    }
    hp::word(m, &gg, -0.005, -0.166, 0.0030, "ENG START");
    switches(m, f, -0.135, 0.125, -0.196, 8, 331.0);
    switches(m, f, -0.135, 0.125, -0.243, 8, 332.0);

    // The autopilot and autothrottle, along the top under the glare shield.
    plate(m, f, -0.35, 0.256, 0.35, 0.297, 340.0);
    for (i, s) in ["A/T ARM", "IAS/MACH", "HDG", "ALT", "VERT SPD", "LOC", "APP", "F/D"].iter().enumerate() {
        let u = -0.305 + i as f32 * 0.087;
        hp::pbutton(m, &gg, u + 0.018, 0.277, 0.016, 0.0085, style::BUTTON, sid(s));
        hp::knob(m, &gg, u - 0.020, 0.277, 0.0058, 341.0 + i as f32, false);
    }
}

/// The glare shield: from its padded lip over the panel forward to the foot
/// of the windscreen's glass, under which it stays.
fn glare_shield(m: &mut Mesh) {
    let hw = 0.70;
    let lip = |x: f32| Vec3::new(x, 0.458 - 0.088 * (x / hw).powi(2), -24.055);
    // (concorde.py: the glass stands on a curve 0.30 m further forward on the centreline than at its ends)
    let foot = |x: f32| {
        let k = 1.0 - (x * 0.964 / 0.682).powi(2);
        Vec3::new(x * 0.964, 0.244 + 0.06 * k, -(24.72 + 0.30 * k))
    };
    m.patch(&GLARE.pat(pat::PIT), 28, 10, false, |u, v| {
        let x = (u * 2.0 - 1.0) * hw;
        let (a, b) = (lip(x), foot(x));
        (a + (b - a) * v, [x, v], 0.0)
    });
    // Its rolled edge, and the face under it down to the panel.
    let edge: Vec<Vec3> = (0..=28).map(|i| lip((i as f32 / 14.0 - 1.0) * hw) + Vec3::new(0.0, -0.008, 0.004)).collect();
    m.tube(&GLARE, &edge, &|_| 0.0095, 10, true);
    m.patch(&GLARE, 28, 2, true, |u, v| {
        let x = (u * 2.0 - 1.0) * hw;
        (lip(x) + Vec3::new(0.0, -0.014 - 0.03 * v, -0.075 * v), [x, v], 0.0)
    });
}

// ---------------------------------------------------------------- pedestal and side consoles

/// The pedestal's half width; its top by the throttles, and behind them.
const PED_X: f32 = 0.135;
const PED_Y: f32 = -0.15;
const PED_Y1: f32 = -0.21;
/// Where it starts under the panel, steps down, and ends.
const PED_Z: [f32; 3] = [-23.99, -23.20, -22.76];
/// The four throttles (x), and their pivot inside the pedestal.
const THR_X: [f32; 4] = [-0.066, -0.022, 0.022, 0.066];
const THR_PIVOT: Vec3 = Vec3::new(0.0, PED_Y - 0.15, -23.55);

fn pedestal(m: &mut Mesh) {
    let c = Vec3::new(0.0, 0.0, -23.5);
    for (za, zb, y) in [(PED_Z[0], PED_Z[1], PED_Y), (PED_Z[1], PED_Z[2], PED_Y1)] {
        for s in [-1.0f32, 1.0] {
            let x = s * PED_X;
            hp::quad_to(m, &LINER_DK, [Vec3::new(x, FLOOR, za), Vec3::new(x, FLOOR, zb), Vec3::new(x, y, zb), Vec3::new(x, y, za)], c + Vec3::X * (s * 2.0));
        }
        hp::quad_to(m, &LINER_DK, [Vec3::new(-PED_X, y, za), Vec3::new(PED_X, y, za), Vec3::new(PED_X, y, zb), Vec3::new(-PED_X, y, zb)], c + Vec3::Y * 3.0);
        hp::quad_to(m, &LINER_DK, [Vec3::new(-PED_X, FLOOR, zb), Vec3::new(PED_X, FLOOR, zb), Vec3::new(PED_X, y, zb), Vec3::new(-PED_X, y, zb)], Vec3::new(0.0, 0.0, zb + 3.0));
    }
    // Looking down on it: u is x, v forward.
    let f = Face::new(Vec3::new(0.0, PED_Y, 0.0), Vec3::X, -Vec3::Z);
    let g = f.sub(0.0, 0.0, 0.0043);
    // Forward: the inertial navigation system's two keypads.
    for (k, s) in [-1.0f32, 1.0].iter().enumerate() {
        let (ua, ub) = if *s < 0.0 { (-PED_X + 0.004, -0.004) } else { (0.004, PED_X - 0.004) };
        plate(m, &f, ua, 23.74, ub, 23.98, 400.0 + k as f32);
        hp::word(m, &g, (ua + ub) * 0.5, 23.965, 0.0030, "INS");
        m.rbox(&BEZEL, g.at_h((ua + ub) * 0.5, 23.930, 0.002), Vec3::new(0.050, 0.014, 0.002), 0.001, g.rot());
        m.rbox(&mt(mat::GLASS, 0.02, 0.05, 0.03), g.at_h((ua + ub) * 0.5, 23.930, 0.0042), Vec3::new(0.046, 0.011, 0.0003), 0.0002, g.rot());
        for i in 0..12 {
            let (cx, cy) = (i % 3, i / 3);
            hp::pbutton(m, &g, (ua + ub) * 0.5 - 0.032 + cx as f32 * 0.032, 23.896 - cy as f32 * 0.036, 0.011, 0.011, style::KEY, sid(["1", "2", "3", "4", "5", "6", "7", "8", "9", "CLR", "0", "ENT"][i]));
        }
    }
    // The throttle quadrant: a housing with a slot for each lever, the
    // reheat switches behind them, the trim wheels' rims at its sides.
    m.rbox(&BLK, f.at_h(0.0, 23.55, 0.012), Vec3::new(0.108, 0.150, 0.012), 0.008, f.rot());
    let q = f.sub(0.0, 0.0, 0.024);
    for x in THR_X {
        m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), q.at_h(x, 23.57, 0.0003), Vec3::new(0.0045, 0.105, 0.0004), 0.0003, q.rot());
        hp::toggle(m, &q, x, 23.425, false);
    }
    hp::word(m, &q, 0.0, 23.445, 0.0030, "MAX");
    hp::word(m, &q, -0.092, 23.50, 0.0028, "IDLE");
    for s in [-1.0f32, 1.0] {
        m.lathe(&mt(mat::PLASTIC, 0.03, 0.03, 0.032).pat(pat::KNURL), Vec3::new(s * (PED_X - 0.012), PED_Y - 0.03, -23.55), Vec3::X * s, &[[0.0, 0.0], [0.075, 0.0], [0.078, 0.004], [0.078, 0.012], [0.075, 0.016], [0.0, 0.016]], 28, false);
    }
    // Behind: the radios, a step down.
    let f1 = Face::new(Vec3::new(0.0, PED_Y1, 0.0), Vec3::X, -Vec3::Z);
    for (k, (va, vb, a, b)) in [(22.99, 23.19, "VHF L", "VHF R"), (22.78, 22.98, "ADF", "XPDR")].iter().enumerate() {
        plate(m, &f1, -PED_X + 0.004, *va, -0.003, *vb, 410.0 + k as f32);
        hp::populate(m, &f1, -PED_X + 0.008, va + 0.004, -0.007, vb - 0.004, 411.0 + k as f32 * 3.0, Some(a));
        plate(m, &f1, 0.003, *va, PED_X - 0.004, *vb, 420.0 + k as f32);
        hp::populate(m, &f1, 0.007, va + 0.004, PED_X - 0.008, vb - 0.004, 421.0 + k as f32 * 3.0, Some(b));
    }
}

/// A shelf along the wall outboard of each pilot: the nose wheel's tiller on
/// the captain's (a piece of its own), switches, an oxygen mask's box.
const SIDE_Y: f32 = FLOOR + 0.58;
const SIDE_X: f32 = 0.74;

fn side_consoles(m: &mut Mesh) {
    let (z0, z1) = (EYE.z - 0.62, EYE.z + 0.40);
    for s in [-1.0f32, 1.0] {
        m.patch(&LINER_DK, 2, 12, s > 0.0, |u, v| {
            let z = z0 + (z1 - z0) * v;
            let x = SIDE_X + (lin_x(z, SIDE_Y) + 0.01 - SIDE_X) * u;
            (Vec3::new(s * x, SIDE_Y, z), [x, z], 0.0)
        });
        let x = s * SIDE_X;
        hp::quad_to(m, &LINER_DK, [Vec3::new(x, FLOOR, z0), Vec3::new(x, FLOOR, z1), Vec3::new(x, SIDE_Y, z1), Vec3::new(x, SIDE_Y, z0)], Vec3::new(0.0, 0.0, EYE.z));
        for z in [z0, z1] {
            let (xb, xt) = (s * (lin_x(z, FLOOR) + 0.02), s * (lin_x(z, SIDE_Y) + 0.01));
            hp::quad_to(m, &LINER_DK, [Vec3::new(x, FLOOR, z), Vec3::new(xb, FLOOR, z), Vec3::new(xt, SIDE_Y, z), Vec3::new(x, SIDE_Y, z)], Vec3::new(0.0, 0.0, z + (z - EYE.z) * 4.0));
        }
        m.tube(&LINER_DK, &[Vec3::new(x, SIDE_Y - 0.004, z0), Vec3::new(x, SIDE_Y - 0.004, z1)], &|_| 0.008, 8, true);
        let f = Face::new(Vec3::new(0.0, SIDE_Y, 0.0), Vec3::X, -Vec3::Z);
        let (ua, ub) = if s < 0.0 { (-lin_x(z1 - 0.3, SIDE_Y) + 0.03, -SIDE_X - 0.012) } else { (SIDE_X + 0.012, lin_x(z1 - 0.3, SIDE_Y) - 0.03) };
        switch_panel(m, &f, ua, -z1 + 0.06, ub, -z1 + 0.30, 2, 4, 500.0 + s, "INTERPHONE");
        m.rbox(&mt(mat::PLASTIC, 0.12, 0.11, 0.10), f.at_h((ua + ub) * 0.5, -z1 + 0.44, 0.03), Vec3::new((ub - ua) * 0.5, 0.09, 0.03), 0.008, f.rot());
        rect(m, &legend_mat(style::RED_PLACARD, sid("PULL"), Vec3::new(0.12, 0.11, 0.10)), f.at_h((ua + ub) * 0.5, -z1 + 0.44, 0.0604), f.ax, f.ay, 0.04, 0.012);
    }
}

// ---------------------------------------------------------------- overhead panel

fn overhead(m: &mut Mesh) {
    // Under the crown between the pilots, rising aft with the roof; read looking up.
    let f = Face::new(Vec3::new(0.0, 0.862, -23.10), Vec3::X, Vec3::new(0.0, 0.247, 0.969));
    m.slab(&LINER_DK, &hp::wind(hp::rrect(0.262, 0.485, 0.02), false), &[], f.o, f.ax, f.ay, 0.03);
    let titles = [["LANDING", "ANTI-ICE", "WIPER"], ["FUEL", "ENG", "HYD"], ["ELEC", "APU", "PACKS"], ["EMER LIGHTS", "IGN", "WINDOW HEAT"], ["FLOOD", "PANEL", "DOME"]];
    for (r, row) in titles.iter().enumerate() {
        let v1 = 0.47 - r as f32 * 0.19;
        for (c, t) in row.iter().enumerate() {
            let u0 = -0.252 + c as f32 * 0.169;
            switch_panel(m, &f, u0, v1 - 0.18, u0 + 0.165, v1, 3, 4, 600.0 + (r * 3 + c) as f32 * 17.0, t);
        }
    }
    // The engines' fire handles in a row at its front.
    for k in 0..4 {
        let p = f.at_h(-0.09 + k as f32 * 0.06, -0.455, 0.0);
        m.tube(&mt(mat::PLASTIC, 0.40, 0.03, 0.02), &[p, p + f.n() * 0.03], &|_| 0.011, 10, true);
    }
}

// ---------------------------------------------------------------- the flight engineer's station

/// His seat, facing the right wall.
const ENG_SEAT: Vec3 = Vec3::new(0.30, 0.0, -21.62);

/// The panel on the right wall: leaning in at the top and turned a little
/// toward the front, as the fuselage narrows.
fn engineer_face() -> Face {
    Face::new(Vec3::new(0.895, 0.45, -21.60), Vec3::new(0.166, 0.0, 0.986), Vec3::new(-0.41, 0.912, 0.0))
}

fn engineer(m: &mut Mesh) {
    let f = engineer_face();
    m.slab(&BLK, &hp::wind(hp::rrect(0.60, 0.50, 0.015), false), &[], f.o, f.ax, f.ay, 0.03);
    let g = f.sub(0.0, 0.0, 0.0004);
    // Along the top: electrics, hydraulics, the air conditioning and the intakes.
    for (k, t) in ["ELEC", "HYD", "BLEED", "PACKS"].iter().enumerate() {
        let u0 = -0.59 + k as f32 * 0.297;
        switch_panel(m, &f, u0, 0.345, u0 + 0.290, 0.490, 2, 8, 700.0 + k as f32 * 23.0, t);
    }
    // The engines' secondary instruments, a column each: oil, vibration, intake.
    for r in 0..3 {
        let v = 0.285 - r as f32 * 0.066;
        for k in 0..4 {
            dial(m, &f, -0.545 + k as f32 * 0.064, v, 0.025, [gauge::JET_EGT, 21.0, gauge::JET_RPM][r], 720.0 + (r * 4 + k) as f32);
        }
        hp::word(m, &g, -0.305, v, 0.0028, ["OIL T", "OIL P", "N1"][r]);
    }
    // The fuel: the tanks' contents in three rows, the flow to each engine under them.
    for r in 0..3 {
        let v = 0.285 - r as f32 * 0.066;
        for k in 0..5 {
            dial(m, &f, -0.215 + k as f32 * 0.064, v, 0.025, if r < 2 { gauge::JET_FUEL } else { gauge::JET_FF }, 740.0 + (r * 5 + k) as f32);
        }
    }
    hp::word(m, &g, -0.087, 0.327, 0.0030, "FUEL");
    // The cabin: its altitude and rate of climb, the temperatures.
    dial(m, &f, 0.235, 0.255, 0.042, gauge::ALT, 760.0);
    dial(m, &f, 0.345, 0.255, 0.042, gauge::VSI, 761.0);
    hp::word(m, &g, 0.235, 0.309, 0.0028, "CAB ALT");
    hp::word(m, &g, 0.345, 0.309, 0.0028, "RATE");
    for k in 0..3 {
        dial(m, &f, 0.455 + (k % 2) as f32 * 0.075, 0.275 - (k / 2) as f32 * 0.07 - (k % 2) as f32 * 0.035, 0.028, gauge::OAT, 762.0 + k as f32);
    }
    for k in 0..4 {
        dial(m, &f, 0.215 + k as f32 * 0.066, 0.150, 0.026, 21.0, 766.0 + k as f32);
    }
    // The fuel panel: pumps and valves for thirteen tanks, the trim transfer.
    mixed_panel(m, &f, -0.59, -0.130, -0.10, 0.075, 4, 13, 770.0, "PUMPS");
    switch_panel(m, &f, -0.095, -0.130, 0.205, 0.075, 3, 9, 771.0, "X FEED");
    mixed_panel(m, &f, 0.21, -0.130, 0.59, 0.075, 4, 10, 772.0, "TRIM AIR");
    // Below: engine starting and the intakes' ramps, generators, the clock.
    switch_panel(m, &f, -0.59, -0.300, -0.20, -0.137, 3, 10, 780.0, "ENG START");
    switch_panel(m, &f, -0.195, -0.300, 0.195, -0.137, 3, 10, 781.0, "GEN CONT");
    switch_panel(m, &f, 0.20, -0.300, 0.59, -0.137, 3, 10, 782.0, "ANTI-ICE");
    for k in 0..8 {
        dial(m, &f, -0.525 + k as f32 * 0.15, -0.355, 0.030, [gauge::JET_RPM, gauge::NOZZLE, gauge::JET_EGT, gauge::CLOCK, gauge::MACH, 21.0, gauge::OAT, gauge::JET_FUEL][k], 790.0 + k as f32);
    }
    switch_panel(m, &f, -0.59, -0.490, 0.59, -0.398, 1, 30, 795.0, "TEST");
    // His table under it, and a lamp on a stalk over it.
    let t = f.at(0.0, -0.50);
    m.rbox(&LINER_DK, Vec3::new(t.x - 0.21, t.y - 0.02, t.z), Vec3::new(0.15, 0.012, 0.54), 0.008, Quat::from_rotation_y(-0.167));
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.75, 0.74, 0.68)), Vec3::new(t.x - 0.21, t.y - 0.0075, t.z + 0.05), Vec3::Z, -Vec3::X, 0.15, 0.10);
    let l = f.at_h(-0.45, 0.50, 0.0);
    let pts = hp::catmull(&[l, l + f.n() * 0.10 + Vec3::Y * 0.02, l + f.n() * 0.20 - Vec3::Y * 0.03], 5);
    m.tube(&CHROME, &pts, &|_| 0.005, 8, false);
    m.lathe(&BLK, pts[pts.len() - 1], -Vec3::Y, &[[0.012, 0.0], [0.022, 0.03], [0.020, 0.03], [0.0, 0.004]], 14, false);
}

/// The circuit breakers, on the wall behind the captain.
fn breakers(m: &mut Mesh) {
    let f = Face::new(Vec3::new(-0.905, 0.45, -21.55), Vec3::new(0.166, 0.0, -0.986), Vec3::new(0.41, 0.912, 0.0));
    m.slab(&BLK, &hp::wind(hp::rrect(0.50, 0.44, 0.015), false), &[], f.o, f.ax, f.ay, 0.03);
    let g = f.sub(0.0, 0.0, 0.0004);
    for r in 0..11 {
        let v = 0.385 - r as f32 * 0.075;
        hp::fake(m, &g, 0.0, v + 0.026, 0.46, 0.0030);
        for k in 0..24 {
            let u = -0.46 + k as f32 * 0.04;
            let pulled = hash(r as f32 * 31.0 + k as f32 * 7.3) > 0.96;
            let h = if pulled { 0.014 } else { 0.007 };
            m.lathe(&hp::KNOB, f.at(u, v), f.n(), &[[0.0075, 0.0], [0.0075, h], [0.0055, h + 0.002], [0.0, h + 0.002]], 6, true);
            if pulled {
                m.lathe(&hp::INK, f.at(u, v), f.n(), &[[0.0050, 0.0], [0.0050, 0.006]], 6, true);
            }
        }
    }
}

// ---------------------------------------------------------------- seats

/// A crew seat on its tracks at `at` (its floor point under the occupant's
/// eye), turned by `yaw`; an armrest on side `arm` (0: both).
fn seat(m: &mut Mesh, at: Vec3, yaw: f32, arm: f32) {
    let mut s = Mesh::default();
    // Pedestal on the tracks, the pan, the cushion with its sheepskin.
    s.rbox(&SEAT_SHELL, Vec3::new(0.0, 0.13, 0.05), Vec3::new(0.17, 0.12, 0.22), 0.03, Quat::IDENTITY);
    s.rbox(&SEAT_SHELL, Vec3::new(0.0, 0.30, 0.02), Vec3::new(0.24, 0.05, 0.25), 0.03, Quat::IDENTITY);
    s.rbox(&FLEECE, Vec3::new(0.0, 0.415, -0.02), Vec3::new(0.23, 0.06, 0.24), 0.05, Quat::from_rotation_x(-0.06));
    for x in [-0.14f32, 0.14] {
        s.rbox(&mt(mat::STEEL, 0.30, 0.30, 0.31), Vec3::new(x, 0.012, 0.10), Vec3::new(0.02, 0.012, 0.42), 0.004, Quat::IDENTITY);
    }
    // The back, leaning a little, and the headrest.
    let lean = Quat::from_rotation_x(0.14);
    let bc = Vec3::new(0.0, 0.82, 0.29);
    s.rbox(&SEAT_SHELL, bc + Vec3::new(0.0, 0.0, 0.055), Vec3::new(0.24, 0.37, 0.04), 0.035, lean);
    s.rbox(&FLEECE, bc, Vec3::new(0.225, 0.35, 0.05), 0.045, lean);
    s.rbox(&SEAT_SIDE, Vec3::new(0.0, 1.26, 0.345), Vec3::new(0.13, 0.085, 0.045), 0.04, lean);
    for ax in [-1.0f32, 1.0] {
        if arm == 0.0 || arm == ax {
            let p = Vec3::new(ax * 0.265, 0.62, 0.02);
            s.rbox(&SEAT_SIDE, p, Vec3::new(0.033, 0.033, 0.17), 0.03, Quat::IDENTITY);
            s.rbox(&SEAT_SHELL, p + Vec3::new(0.0, -0.08, 0.14), Vec3::new(0.024, 0.08, 0.03), 0.015, Quat::IDENTITY);
        }
    }
    // The harness: shoulder straps over the back, the buckle on the cushion.
    for dx in [-0.09f32, 0.09] {
        let a = bc + Vec3::new(dx, 0.36, -0.02);
        let lap = Vec3::new(dx * 1.6, 0.478, 0.0);
        s.tube(&mt(mat::FABRIC, 0.08, 0.07, 0.05), &[a, a + Vec3::new(0.0, -0.015, -0.062), bc + lean * Vec3::new(dx, -0.30, -0.058), Vec3::new(dx * 1.2, 0.485, 0.17), lap, lap + Vec3::new(0.0, 0.0, -0.08)], &|_| 0.011, 6, true);
    }
    s.rbox(&CHROME, Vec3::new(0.0, 0.48, -0.10), Vec3::new(0.05, 0.008, 0.04), 0.008, Quat::IDENTITY);
    s.transform(0, Mat4::from_translation(at) * Mat4::from_rotation_y(yaw));
    m.append(&s);
}

// ---------------------------------------------------------------- what moves

fn controls(b: &mut Builder, f: &Face) {
    // The four throttles: an arm up through each slot to its knob, the
    // reverse thrust lever on its front.
    for x in THR_X {
        let piv = Vec3::new(x, THR_PIVOT.y, THR_PIVOT.z);
        let mut m = Mesh::default();
        m.tube(&ALU, &[piv + Vec3::new(0.0, 0.12, 0.0), piv + Vec3::new(0.0, 0.25, 0.010), piv + Vec3::new(0.0, 0.29, -0.004)], &|_| 0.005, 8, false);
        let kc = piv + Vec3::new(0.0, 0.297, -0.008);
        m.lathe(&mt(mat::PLASTIC, 0.035, 0.035, 0.037), kc - Vec3::X * 0.017, Vec3::X, &[[0.0, 0.0], [0.014, 0.0], [0.016, 0.004], [0.013, 0.010], [0.013, 0.024], [0.016, 0.030], [0.014, 0.034], [0.0, 0.034]], 16, true);
        m.rbox(&mt(mat::PLASTIC, 0.02, 0.02, 0.022), piv + Vec3::new(0.0, 0.25, -0.022), Vec3::new(0.005, 0.028, 0.005), 0.002, Quat::from_rotation_x(-0.15));
        b.add("thrust lever", G_ENGINE, m, Anim::hinge(Drive::new(Input::Throttle, -0.55).offset(0.28)), piv, Vec3::X, None);
    }

    for sx in [-1.0f32, 1.0] {
        let x = sx * SEAT_X;
        // The control column, hinged at the floor.
        let base = Vec3::new(x, FLOOR, EYE.z - 0.64);
        let top = Vec3::new(x, EYE.y - 0.60, EYE.z - 0.52);
        let mut c = Mesh::default();
        c.lathe(&COLUMN, base, Vec3::Y, &[[0.060, 0.0], [0.060, 0.02], [0.046, 0.04], [0.042, 0.06]], 16, false);
        c.tube(&COLUMN, &[base + Vec3::Y * 0.05, top - Vec3::Y * 0.03], &|t| 0.038 - 0.008 * t, 14, false);
        c.lathe(&mt(mat::RUBBER, 0.03, 0.03, 0.032), base + Vec3::Y * 0.06, Vec3::Y, &[[0.044, 0.0], [0.048, 0.03], [0.042, 0.06], [0.046, 0.09], [0.040, 0.10]], 16, false);
        c.rbox(&COLUMN, top, Vec3::new(0.035, 0.030, 0.040), 0.018, Quat::IDENTITY);
        let ci = b.add("control column", G_ENGINE, c, Anim::hinge(Drive::new(Input::Pitch, 0.09)), base, Vec3::X, None);
        // The yoke: Concorde's ram's horns, down and out from the hub and up into the grips.
        let hub = top + Vec3::new(0.0, 0.025, 0.035);
        let n = Vec3::new(0.0, 0.25, 1.0).normalize();
        let rot = Quat::from_rotation_arc(Vec3::Z, n);
        let mut y = Mesh::default();
        y.rbox(&COLUMN, hub, Vec3::new(0.045, 0.035, 0.028), 0.016, rot);
        y.disc(&mt(mat::ALU, 0.6, 0.6, 0.62), hub + rot * Vec3::new(0.0, 0.0, 0.029), n, rot * Vec3::Y, 0.014, 16);
        for s in [-1.0f32, 1.0] {
            let horn: Vec<Vec3> = hp::catmull(&[(0.030, -0.015), (0.075, -0.068), (0.125, -0.082), (0.162, -0.050), (0.172, 0.010), (0.168, 0.075), (0.158, 0.125)].map(|(a, c)| hub + rot * Vec3::new(s * a, c, 0.0)), 4);
            y.tube(&COLUMN, &horn, &|t| 0.013 + 0.005 * smoothstep(0.55, 0.8, t), 12, true);
            // Trim switch and the autopilot disconnect on the grips.
            y.dome(&mt(mat::PLASTIC, 0.35, 0.03, 0.03), hub + rot * Vec3::new(s * 0.158, 0.128, 0.010), rot * Vec3::new(0.0, 1.0, 0.4), 0.006, 0.005, 10, 0.0);
            y.rbox(&BLK, hub + rot * Vec3::new(s * 0.166, 0.085, 0.017), Vec3::new(0.005, 0.009, 0.004), 0.002, rot);
        }
        b.add("yoke", G_ENGINE, y, Anim::hinge(Drive::new(Input::Roll, -0.8)), hub, n, Some(ci));
        // Rudder pedals with their brake tops, hanging under the panel.
        for dx in [-0.10f32, 0.10] {
            let mut p = Mesh::default();
            let pv = Vec3::new(x + dx, FLOOR + 0.42, EYE.z - 1.04);
            let pc = Vec3::new(x + dx, FLOOR + 0.18, EYE.z - 0.97);
            p.tube(&STEEL, &[pv, pc], &|_| 0.011, 8, true);
            let tilt = Quat::from_rotation_x(-0.35);
            p.rbox(&mt(mat::ALU, 0.35, 0.35, 0.36), pc + Vec3::new(0.0, 0.0, 0.01), Vec3::new(0.045, 0.085, 0.008), 0.006, tilt);
            p.rbox(&RUBBER.pat(pat::KNURL), pc + Vec3::new(0.0, 0.0, 0.019), Vec3::new(0.04, 0.075, 0.0025), 0.002, tilt);
            b.add("rudder pedal", G_ENGINE, p, Anim::hinge(Drive::new(Input::Yaw, dx.signum() * 0.18)), pv, Vec3::X, None);
        }
    }

    // The landing gear lever: a wheel-shaped knob on an arm, up and down its slot.
    let w = f.sub(GEAR_U, GEAR_V, 0.0);
    let mut gh = Mesh::default();
    let arm_top = w.at_h(0.0, 0.0, 0.05);
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), arm_top], &|_| 0.0045, 10, false);
    let c = arm_top + w.n() * 0.004;
    gh.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.016, 0.0], [0.019, 0.003], [0.019, 0.021], [0.016, 0.024], [0.0, 0.024]], 20, false);
    gh.lathe(&RED_LENS, c - w.ax * 0.013, w.ax, &[[0.0, 0.0], [0.009, 0.0], [0.009, 0.026], [0.0, 0.026]], 12, false);
    b.add("gear lever", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, 0.028), w.ay), w.o, Vec3::X, None);

    // The nose and visor lever: hinged behind the panel, its knob in the slot
    // (up: nose and visor up; down by detents to the nose fully drooped).
    let v = f.sub(GEAR_U, NOSE_V, 0.0);
    let piv = v.at_h(0.0, 0.0, -0.075);
    let mut nl = Mesh::default();
    nl.tube(&STEEL, &[piv, v.at_h(0.0, 0.0, 0.035)], &|_| 0.004, 8, false);
    nl.rbox(&mt(mat::PLASTIC, 0.72, 0.70, 0.66), v.at_h(0.0, 0.0, 0.043), Vec3::new(0.014, 0.009, 0.010), 0.004, v.rot());
    b.add("nose lever", G_ENGINE, nl, Anim::hinge(Drive::new(Input::FlapLever, 0.80).offset(-0.40)), piv, Vec3::X, None);

    // The nose wheel's tiller on the captain's side console.
    let mut t = Mesh::default();
    let tc = Vec3::new(-(SIDE_X + 0.10), SIDE_Y, EYE.z - 0.38);
    t.lathe(&COLUMN, tc, Vec3::Y, &[[0.010, 0.0], [0.010, 0.025], [0.055, 0.030], [0.060, 0.038], [0.055, 0.044], [0.0, 0.046]], 20, false);
    t.tube(&COLUMN, &[tc + Vec3::new(0.04, 0.044, 0.0), tc + Vec3::new(0.04, 0.080, 0.0)], &|_| 0.007, 8, true);
    b.add("tiller", G_ENGINE, t, Anim::hinge(Drive::new(Input::Steer, -1.8)), tc, Vec3::Y, None);
}

// ---------------------------------------------------------------- what the eye sees

/// Does a look from the seat end on the flight deck, rather than out through
/// the windscreen or a side window? Followed out to the skin: it is a window
/// only where the skin has an opening.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let d = d.normalize_or_zero();
    let mut t = 0.04;
    while t < 6.0 {
        let p = eye + d * t;
        if p.z > Z1 || p.z < WS_Z[0] || p.y < FLOOR {
            return true;
        }
        if outside(p) >= 0.0 {
            let [_, hw, top, mid, bot] = fuse_at(-p.z);
            let h = if p.y >= mid { top - mid } else { mid - bot };
            let a = ((p.y - mid) / h).atan2((p.x / hw).abs());
            let screen = p.z < WS_Z[1] && a > WS_A;
            let side = SW_Z.iter().any(|w| p.z > w[0] && p.z < w[1]) && a > SW_A[0] && a < SW_A[1];
            return !(screen || side);
        }
        t += 0.02;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing on the flight deck pokes out through the skin, or stands in a window's opening.
    #[test]
    fn flight_deck_fits_inside_the_skin() {
        let mut b = Builder::default();
        build(&mut b);
        let mut n = 0;
        let mut worst = ("", Vec3::ZERO, -1.0f32);
        for p in &b.pieces {
            for v in &p.mesh.v {
                let q = Vec3::from(v.pos);
                if !(outside(q) < 0.0 && q.z > -25.3 && q.z < -20.0 && q.y > FLOOR - 0.05) && outside(q) > worst.2 {
                    worst = (p.name, q, outside(q));
                }
                n += 1;
            }
        }
        assert!(worst.0.is_empty(), "{} is outside the skin at {:?} ({:.3})", worst.0, worst.1, worst.2);
        for p in &b.pieces {
            println!("  {}: {} vertices", p.name, p.mesh.v.len());
        }
        println!("{} flight deck pieces, {n} vertices, all inside the skin", b.pieces.len());
    }

    #[test]
    fn the_captain_sees_out_ahead_and_to_his_left() {
        let dir = |yaw: f32, pitch: f32| Vec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), -yaw.cos() * pitch.cos());
        assert!(!sees_inside(EYE, dir(0.0, -0.05)), "ahead, a little up: the windscreen");
        assert!(sees_inside(EYE, dir(0.0, 0.5)), "down ahead: the panel");
        assert!(sees_inside(EYE, dir(0.0, -1.4)), "up: the roof");
        assert!(!sees_inside(EYE, dir(-1.4, 0.3)), "to the left, a little down: a side window");
        assert!(sees_inside(EYE, dir(2.6, 0.1)), "back to the right: the engineer's panel");
    }
}
