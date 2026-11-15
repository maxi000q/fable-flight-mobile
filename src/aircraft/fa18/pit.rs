//! The F/A-18C's front cockpit, laid out from photographs of real ones (Swiss
//! and US Navy C and D models on Wikimedia Commons, category "Cockpit of F/A-18
//! Hornet"): the hooded upper instrument panel with its two monochrome DDIs
//! either side of the up-front control (keypad, scratchpad, five option
//! windows) and the HUD on top of the hood, its combiner held up between two
//! lattice brackets; the lower panel with the colour AMPCD, the IFEI, the
//! landing gear handle, and the standby instruments with the RWR scope; the
//! side consoles packed with switch panels and the two throttles; the stick
//! and rudder pedals; an empty Martin-Baker ejection seat with its olive mesh
//! cushions, the harness lying loose and the yellow and black firing handle.
//!
//! The tub is painted grey, the panels black with white legends (drawn in
//! plane.wgsl / deck747.wgsl from `pat::LEGEND` quads, lit green at night); the
//! displays and the HUD symbology are drawn in pit18.wgsl from the instrument
//! readings. Everything here is body frame (+X right, +Y up, -Z forward).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, screen_quad, sid, style, Face};
use glam::{Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

// ---------------------------------------------------------------- materials

const GREY: Mat = mt(mat::SATIN, 0.20, 0.205, 0.215).pat(pat::PIT);
const GREY_DK: Mat = mt(mat::SATIN, 0.10, 0.104, 0.11).pat(pat::PIT);
const BLK: Mat = mt(mat::PLASTIC, 0.028, 0.028, 0.030).pat(pat::PANEL);
const BEZEL: Mat = mt(mat::PLASTIC, 0.036, 0.037, 0.039).pat(pat::PANEL);
const KNOB: Mat = mt(mat::PLASTIC, 0.013, 0.013, 0.014);
const KNOB_ALU: Mat = mt(mat::ALU, 0.40, 0.41, 0.43);
const BTN: Mat = mt(mat::RUBBER, 0.045, 0.045, 0.047);
const GLARE: Mat = mt(mat::BLACK, 0.028, 0.028, 0.030);
const INK: Mat = mt(mat::PLASTIC, 0.72, 0.72, 0.69);
const OLIVE: Mat = mt(mat::FABRIC, 0.115, 0.135, 0.052).pat(pat::MESH);
const OLIVE_DK: Mat = mt(mat::FABRIC, 0.050, 0.060, 0.030).pat(pat::MESH);
const WEB: Mat = mt(mat::FABRIC, 0.040, 0.050, 0.032).pat(pat::WEBBING);
const WEB_RED: Mat = mt(mat::FABRIC, 0.30, 0.028, 0.022).pat(pat::WEBBING);
const SEAT_GREY: Mat = mt(mat::SATIN, 0.055, 0.057, 0.06).pat(pat::PIT);
const FLOOR_M: Mat = mt(mat::SATIN, 0.032, 0.033, 0.035).pat(pat::NONSLIP);
const HOSE: Mat = mt(mat::RUBBER, 0.055, 0.065, 0.040).pat(pat::HOSE);
const DZUS: Mat = mt(mat::STEEL, 0.30, 0.30, 0.31);
const COMBINER_FRAME: Mat = mt(mat::PLASTIC, 0.022, 0.022, 0.024);
const MIRROR: Mat = mt(mat::STEEL, 0.55, 0.56, 0.57).x(1.0);
const RED_LENS: Mat = mt(mat::LAMP, 1.0, 0.08, 0.04).x(lamp::ANNUNC);
const GREEN_LENS: Mat = mt(mat::LAMP, 0.2, 1.0, 0.25).x(lamp::GEAR_DOWN);

/// Yellow and black hazard stripes, `pitch` m apart: diagonal across a flat
/// face, or rings along a tube or a strap (`rings`).
const fn stripes(pitch: f32, rings: bool) -> Mat {
    mt(mat::PAINT, 0.56, 0.42, 0.02).pat(pat::STRIPE).x(if rings { -pitch } else { pitch })
}

fn blk_col() -> Vec3 {
    BLK.col
}

// ---------------------------------------------------------------- where things are

pub const FLOOR: f32 = 0.16;
/// Side consoles: height of their tops, inner edge, front and back.
const CON_Y: f32 = 0.415;
const CON_X: f32 = 0.235;
const CON_Z0: f32 = -5.38;
const CON_Z1: f32 = -3.80;
/// The seat reference point: where the back and pan cushions meet.
const SRP: Vec3 = Vec3::new(0.0, 0.27, -4.42);
/// The stick's pivot in the floor.
const STICK: Vec3 = Vec3::new(0.0, FLOOR + 0.02, -4.99);

/// The canopy (fa18.py CANOPY): Blender station, half width at the sill, height of the top.
const CANOPY: [[f32; 3]; 10] = [
    [6.25, 0.40, 0.64], [6.00, 0.45, 0.80], [5.70, 0.50, 1.00], [5.40, 0.52, 1.12], [5.00, 0.53, 1.20],
    [4.50, 0.53, 1.22], [4.00, 0.52, 1.20], [3.50, 0.48, 1.12], [3.10, 0.42, 0.98], [2.70, 0.34, 0.80],
];

fn canopy_at(z: f32) -> [f32; 3] {
    let y = -z;
    if y >= CANOPY[0][0] {
        return CANOPY[0];
    }
    for w in CANOPY.windows(2) {
        if w[0][0] >= y && y >= w[1][0] {
            let t = (w[0][0] - y) / (w[0][0] - w[1][0]);
            return [y, w[0][1] + (w[1][1] - w[0][1]) * t, w[0][2] + (w[1][2] - w[0][2]) * t];
        }
    }
    CANOPY[CANOPY.len() - 1]
}
/// Height of the fuselage's upper skin at station z, x out.
fn skin_y(z: f32, x: f32) -> f32 {
    let [_, hw, top, mid, _, p] = fuse_at(-z);
    let f = (1.0 - (x.abs().min(hw) / hw.max(1e-6)).powf(p)).max(0.0).powf(1.0 / p);
    mid + (top - mid) * f
}
/// Half width of the fuselage skin at station z, height y.
fn skin_hw(z: f32, y: f32) -> f32 {
    let [_, hw, top, mid, bot, p] = fuse_at(-z);
    let f = if y >= mid { (y - mid) / (top - mid) } else { (mid - y) / (mid - bot) };
    hw * (1.0 - f.clamp(0.0, 1.0).powf(p)).max(0.0).powf(1.0 / p)
}
/// The inner face of the canopy sill (fa18.py: w - 0.022), and its top.
fn sill_x(z: f32) -> f32 {
    canopy_at(z)[1] - 0.025
}
fn sill_top(z: f32) -> f32 {
    fuse_at(-z)[2] - 0.022
}
/// The cockpit's side walls: inside the sill, and inside the skin below it.
fn wall_x(z: f32, y: f32) -> f32 {
    // (Above the foot of the sill the skin is cut away: the sill is the wall.)
    let foot = skin_y(z, sill_x(z)) - 0.04;
    (sill_x(z) - 0.004).min(skin_hw(z, y.min(foot)) - 0.035).max(0.26)
}
/// Height of the canopy glass's inside at station z, x out (fa18.py canopy_arc).
fn glass_y(z: f32, x: f32) -> f32 {
    let [_, w, zt] = canopy_at(z);
    let z0 = fuse_at(-z)[2] - 0.04;
    let c = (x.abs() / w).min(1.0);
    z0 + (zt - z0) * (1.0 - c * c).sqrt() - 0.006
}

// ---------------------------------------------------------------- outlines

/// Rectangle with rounded corners, clockwise (so slab walls face out).
fn rrect(hx: f32, hy: f32, r: f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    for (q, (sx, sy)) in [(1.0f32, -1.0f32), (-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].iter().enumerate() {
        let c = Vec2::new(sx * (hx - r), sy * (hy - r));
        for i in 0..=4 {
            let a = -FRAC_PI_2 * q as f32 - i as f32 / 4.0 * FRAC_PI_2;
            out.push(c + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    out
}
/// An outline turned clockwise (slab walls then face out); `hole` turns it the other way.
fn wind(mut v: Vec<Vec2>, hole: bool) -> Vec<Vec2> {
    let n = v.len();
    let area: f32 = (0..n).map(|i| v[i].perp_dot(v[(i + 1) % n])).sum();
    if (area > 0.0) != hole {
        v.reverse();
    }
    v
}
/// A quad facing `toward`.
fn quad_to(m: &mut Mesh, mm: &Mat, mut p: [Vec3; 4], toward: Vec3) {
    let n = (p[1] - p[0]).cross(p[2] - p[0]);
    let c = (p[0] + p[1] + p[2] + p[3]) * 0.25;
    if n.dot(toward - c) < 0.0 {
        p.reverse();
    }
    m.quad(mm, p);
}
fn ccw(mut v: Vec<Vec2>) -> Vec<Vec2> {
    v.reverse();
    v
}
/// Octagon (a box with its corners cut by `c`), clockwise.
fn octagon(hx: f32, hy: f32, c: f32) -> Vec<Vec2> {
    vec![
        Vec2::new(hx - c, -hy), Vec2::new(-hx + c, -hy), Vec2::new(-hx, -hy + c), Vec2::new(-hx, hy - c),
        Vec2::new(-hx + c, hy), Vec2::new(hx - c, hy), Vec2::new(hx, hy - c), Vec2::new(hx, -hy + c),
    ]
}

fn catmull(p: &[Vec3], sub: usize) -> Vec<Vec3> {
    let n = p.len();
    let mut out = Vec::new();
    for i in 0..n - 1 {
        let (a, b, c, d) = (p[i.saturating_sub(1)], p[i], p[i + 1], p[(i + 2).min(n - 1)]);
        for k in 0..sub {
            let t = k as f32 / sub as f32;
            let (t2, t3) = (t * t, t * t * t);
            out.push(0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3));
        }
    }
    out.push(p[n - 1]);
    out
}

// ---------------------------------------------------------------- what sits on a panel

fn word(m: &mut Mesh, f: &Face, u: f32, v: f32, h: f32, s: &str) {
    let n = s.chars().count().max(1) as f32;
    let hw = (n * 1.45 - 0.45) * h * 0.25 + h * 0.3;
    rect(m, &legend_mat(style::TEXT, sid(s), blk_col()), f.at_h(u, v, 0.0004), f.ax, f.ay, hw, h * 0.72);
}
/// Made-up lettering filling `hw` x `hh`.
fn fake(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32) {
    rect(m, &legend_mat(style::TEXT, 0, blk_col()), f.at_h(u, v, 0.0004), f.ax, f.ay, hw, hh);
}
fn dzus(m: &mut Mesh, f: &Face, u: f32, v: f32, seed: f32) {
    let c = f.at(u, v);
    m.lathe(&DZUS, c, f.n(), &[[0.0036, 0.0], [0.0036, 0.0006], [0.0029, 0.0013], [0.0, 0.0014]], 10, true);
    m.rbox(&KNOB, c + f.n() * 0.0013, Vec3::new(0.0028, 0.00045, 0.00025), 0.0001, f.rot() * Quat::from_rotation_z(hash(seed) * PI));
}
/// A console panel: a black plate standing proud with a Dzus fastener in each corner.
fn plate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let (cu, cv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let (hu, hv) = ((u1 - u0) * 0.5 - 0.0007, (v1 - v0) * 0.5 - 0.0007);
    m.rbox(&BLK, f.at_h(cu, cv, 0.0015), Vec3::new(hu, hv, 0.0028), 0.0012, f.rot());
    let g = f.sub(0.0, 0.0, 0.0043);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        dzus(m, &g, cu + du * (hu - 0.0055), cv + dv * (hv - 0.0055), seed + k as f32);
    }
}
/// A black knob with a white pointer line, and its scale printed round it.
fn knob(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, seed: f32, scale: bool) {
    let c = f.at(u, v);
    let n = f.n();
    m.lathe(&KNOB, c, n, &[[r * 1.35, 0.0], [r * 1.35, r * 0.32], [r * 1.12, r * 0.45], [r * 0.92, r * 0.5], [r * 0.92, r * 1.45], [r * 0.82, r * 1.62], [0.0, r * 1.65]], 16, false);
    let a = (hash(seed) - 0.5) * 2.6;
    let rot = f.rot() * Quat::from_rotation_z(a);
    m.rbox(&INK, c + n * (r * 1.65) + rot * Vec3::new(0.0, r * 0.42, 0.0), Vec3::new(r * 0.11, r * 0.42, 0.0003), 0.0001, rot);
    m.rbox(&INK, c + n * (r * 0.42) + rot * Vec3::new(0.0, r * 1.25, 0.0), Vec3::new(r * 0.08, r * 0.12, 0.0003), 0.0001, rot);
    if scale {
        rect(m, &legend_mat(style::SCALE, (hash(seed + 3.0) * 9.0) as u32, blk_col()), c + n * 0.0004, f.ax, f.ay, r * 2.7, r * 2.7);
    }
}
/// A chunky machined knob (the UFC's channel selectors).
fn knob_alu(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32) {
    let c = f.at(u, v);
    m.lathe(&KNOB_ALU.pat(pat::KNURL), c, f.n(), &[[r * 1.1, 0.0], [r * 1.1, r * 0.25], [r, r * 0.3], [r, r * 1.1], [r * 0.9, r * 1.25], [0.0, r * 1.28]], 18, false);
    m.disc(&KNOB, c + f.n() * (r * 1.285), f.n(), f.ay, r * 0.55, 14);
}
/// Lever-lock toggle switch.
fn toggle(m: &mut Mesh, f: &Face, u: f32, v: f32, up: bool) {
    let c = f.at(u, v);
    let n = f.n();
    m.lathe(&CHROME, c, n, &[[0.0048, 0.0], [0.0048, 0.0016], [0.0036, 0.0020], [0.0036, 0.0040], [0.0, 0.0042]], 8, false);
    let tip = c + n * 0.0145 + f.ay * if up { 0.0055 } else { -0.0055 };
    m.tube(&CHROME, &[c + n * 0.004, tip], &|t| 0.0016 + 0.0006 * t, 8, true);
    m.dome(&CHROME, tip, (tip - c).normalize(), 0.0023, 0.0018, 8, 0.0);
}
/// A switch under a hinged guard: red, or striped yellow and black.
fn guarded(m: &mut Mesh, f: &Face, u: f32, v: f32, hazard: bool) {
    toggle(m, f, u, v, false);
    let col = if hazard { stripes(0.005, false) } else { mt(mat::PAINT, 0.34, 0.03, 0.02) };
    let rot = f.rot() * Quat::from_rotation_x(-0.12);
    m.rbox(&col, f.at_h(u, v - 0.002, 0.011), Vec3::new(0.0085, 0.015, 0.0085), 0.0022, rot);
}
/// Square push button with its legend.
fn pbutton(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, st: u32, s: u32) {
    m.rbox(&BEZEL, f.at_h(u, v, 0.0035), Vec3::new(hw + 0.0014, hh + 0.0014, 0.0035), 0.0011, f.rot());
    rect(m, &legend_mat(st, s, Vec3::new(0.02, 0.02, 0.021)), f.at_h(u, v, 0.0072), f.ax, f.ay, hw, hh);
}
/// A round push button.
fn rbutton(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32) {
    m.lathe(&BEZEL, f.at(u, v), f.n(), &[[r * 1.4, 0.0], [r * 1.4, r * 0.3], [r, r * 0.35], [r, r * 1.0], [r * 0.85, r * 1.15], [0.0, r * 1.18]], 12, false);
}
/// A warning light: a lens with its legend, dark until lit.
fn light(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, s: u32, lens: &Mat) {
    m.rbox(&BEZEL, f.at_h(u, v, 0.003), Vec3::new(hw + 0.002, hh + 0.002, 0.003), 0.0012, f.rot());
    m.rbox(lens, f.at_h(u, v, 0.0062), Vec3::new(hw, hh, 0.0006), 0.0004, f.rot());
    rect(m, &legend_mat(style::BUTTON, s, Vec3::new(0.03, 0.02, 0.02)), f.at_h(u, v, 0.0070), f.ax, f.ay, hw, hh);
}

/// Fill a console panel with rows of knobs, toggles and buttons and their
/// legends, made up from `seed` the way the Hornet's panels look.
fn populate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32, title: Option<&str>) {
    let g = f.sub(0.0, 0.0, 0.0043);
    let w = u1 - u0;
    let mut top = v1;
    if let Some(t) = title {
        word(m, &g, (u0 + u1) * 0.5, v1 - 0.007, 0.0042, t);
        top -= 0.012;
    }
    let h = top - v0;
    let rows = ((h / 0.044).floor() as usize).max(1);
    let rh = h / rows as f32;
    for r in 0..rows {
        let vy = top - (r as f32 + 0.55) * rh;
        let kind = (hash(seed + r as f32 * 3.1) * 5.0) as u32;
        let pitch = match kind { 0 => 0.042, 1 => 0.030, 2 => 0.032, _ => 0.036 };
        let n = ((w - 0.016) / pitch).floor().max(1.0) as usize;
        let start = (u0 + u1) * 0.5 - (n as f32 - 1.0) * pitch * 0.5;
        for i in 0..n {
            let x = start + i as f32 * pitch;
            let s2 = seed + r as f32 * 13.0 + i as f32 * 1.37;
            let pick = hash(s2);
            if pick < 0.14 {
                continue;
            }
            match kind {
                0 => {
                    knob(m, &g, x, vy - 0.004, 0.0058, s2, true);
                    fake(m, &g, x, vy + 0.0155, 0.013, 0.003);
                }
                1 => {
                    toggle(m, &g, x, vy - 0.002, pick > 0.55);
                    fake(m, &g, x, vy + 0.0125, 0.011, 0.0028);
                    fake(m, &g, x, vy - 0.0145, 0.008, 0.0026);
                }
                2 => {
                    if pick > 0.8 { guarded(m, &g, x, vy - 0.002, pick > 0.93) } else { toggle(m, &g, x, vy - 0.002, pick > 0.4) }
                    fake(m, &g, x, vy + 0.0145, 0.012, 0.0028);
                }
                3 => {
                    pbutton(m, &g, x, vy - 0.002, 0.0095, 0.0085, style::BUTTON, 0);
                    fake(m, &g, x, vy + 0.014, 0.012, 0.0028);
                }
                _ => {
                    if pick > 0.5 {
                        knob(m, &g, x, vy - 0.003, 0.0048, s2, false);
                    } else {
                        toggle(m, &g, x, vy - 0.002, pick > 0.3);
                    }
                    fake(m, &g, x, vy + 0.0135, 0.012, 0.0028);
                }
            }
        }
    }
}

// ---------------------------------------------------------------- the cockpit

pub fn build(b: &mut Builder) {
    let mut tub = Mesh::default();
    walls(&mut tub);
    rear(&mut tub);
    canopy_fittings(&mut tub);
    b.fixed("cockpit tub", G_ENGINE, tub);

    let mut p = Mesh::default();
    let up = upper_face();
    upper_panel(&mut p, &up);
    hood(&mut p);
    hud(&mut p);
    lower_panel(&mut p);
    b.fixed("instrument panel", G_ENGINE, p);

    let mut c = Mesh::default();
    consoles(&mut c);
    b.fixed("consoles", G_ENGINE, c);

    let mut s = Mesh::default();
    seat(&mut s);
    b.fixed("ejection seat", G_ENGINE, s);

    let mut g = Mesh::default();
    combiner(&mut g);
    let k = b.fixed("hud combiner", G_ENGINE, g);
    b.pieces[k].glass = true;

    controls(b);
}

// ---------------------------------------------------------------- walls, floor, rear deck

fn walls(m: &mut Mesh) {
    let (z0, z1) = (-6.02, -3.40);
    for s in [-1.0f32, 1.0] {
        // The side wall: from the floor up to the canopy sill, following the skin.
        m.patch(&GREY, 40, 14, s < 0.0, |u, v| {
            let z = z0 + (z1 - z0) * u;
            let top = sill_top(z) - 0.004;
            let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
            (Vec3::new(s * wall_x(z, y), y, z), [z, y], 0.0)
        });
        // The rounded inner lip of the sill on top of it, with the canopy's lock hooks.
        let lip: Vec<Vec3> = (0..=40).map(|i| {
            let z = z0 + 0.1 + (z1 - z0 - 0.1) * i as f32 / 40.0;
            Vec3::new(s * (sill_x(z) - 0.008), sill_top(z) - 0.007, z)
        }).collect();
        m.tube(&GREY, &lip, &|_| 0.010, 14, true);
        for k in 0..7 {
            let z = -5.55 + k as f32 * 0.30;
            let p = Vec3::new(s * (sill_x(z) - 0.020), sill_top(z) - 0.012, z);
            m.rbox(&GREY_DK, p, Vec3::new(0.008, 0.012, 0.016), 0.003, Quat::IDENTITY);
            m.bolt(&STEEL, p + Vec3::new(-s * 0.008, 0.0, 0.0), Vec3::X * -s, 0.0035);
            m.rbox(&GREY_DK, p + Vec3::new(s * 0.006, -0.018, 0.0), Vec3::new(0.004, 0.012, 0.024), 0.002, Quat::IDENTITY);
        }
    }
    // Rows of screws along the walls' panel joints above the consoles, and a seam between them.
    for s in [-1.0f32, 1.0] {
        for row in [0.455f32, 0.585] {
            for k in 0..26 {
                let z = -5.15 + k as f32 * 0.052;
                let x = s * (wall_x(z, row) - 0.0005);
                m.screw(&SCREW_PAINT, Vec3::new(x, row, z), Vec3::X * -s, 0.0022, k as f32 + row * 10.0);
            }
        }
        for k in 0..6 {
            let z = -5.0 + k as f32 * 0.26;
            let (y0, y1) = (CON_Y + 0.01, sill_top(z) - 0.03);
            m.tube(&GREY_DK, &[Vec3::new(s * (wall_x(z, y0) - 0.001), y0, z), Vec3::new(s * (wall_x(z, y1) - 0.001), y1, z)], &|_| 0.0012, 4, false);
        }
    }
    // The floor, its gritty walkway, and the pedal well in front.
    m.patch(&FLOOR_M, 6, 16, true, |u, v| {
        let z = -6.0 + 2.25 * v;
        let x = (u * 2.0 - 1.0) * (wall_x(z, FLOOR) + 0.01);
        (Vec3::new(x, FLOOR, z), [x, z], 0.0)
    });
    quad_to(m, &GREY_DK, [Vec3::new(-0.36, FLOOR - 0.01, -5.98), Vec3::new(0.36, FLOOR - 0.01, -5.98), Vec3::new(0.36, 0.30, -5.98), Vec3::new(-0.36, 0.30, -5.98)], EYE);
}

/// Behind the seat: the bulkhead, the deck under the back of the canopy and what is on it.
fn rear(m: &mut Mesh) {
    let zb = -3.74;
    let deck_y = |x: f32, z: f32| skin_y(z, x) - 0.035;
    m.patch(&GREY, 12, 8, true, |u, v| {
        let x = (u * 2.0 - 1.0) * (wall_x(zb, 0.4) + 0.01);
        let top = deck_y(x, zb);
        let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
        (Vec3::new(x, y, zb), [x, y], 0.0)
    });
    m.patch(&GREY, 14, 8, true, |u, v| {
        let z = zb + (-3.12 - zb) * v;
        let x = (u * 2.0 - 1.0) * (sill_x(z) + 0.004);
        (Vec3::new(x, deck_y(x, z), z), [x, z], 0.0)
    });
    // Avionics boxes and the canopy actuator's housing on the deck.
    let at = |x: f32, z: f32, h: f32| Vec3::new(x, deck_y(x, z) + h, z);
    m.rbox(&GREY_DK, at(0.0, -3.45, 0.04), Vec3::new(0.09, 0.05, 0.20), 0.02, Quat::IDENTITY);
    m.tube(&CHROME, &[at(0.0, -3.62, 0.06), at(0.0, -3.30, 0.11)], &|_| 0.012, 12, true);
    for s in [-1.0f32, 1.0] {
        m.rbox(&BLK, at(s * 0.24, -3.55, 0.025), Vec3::new(0.08, 0.03, 0.12), 0.006, Quat::IDENTITY);
        for k in 0..3 {
            dzus(m, &Face::new(at(s * 0.24, -3.55, 0.056), Vec3::X, -Vec3::Z), -0.06 + k as f32 * 0.06, 0.1, k as f32 + s);
        }
    }
    // (facing forward, toward the seat)
    let f = Face::new(Vec3::new(-0.20, deck_y(0.0, zb) - 0.12, zb - 0.0005), -Vec3::X, Vec3::Y);
    rect(m, &legend_mat(style::RED_PLACARD, sid("DANGER"), GREY.col), f.o, f.ax, f.ay, 0.035, 0.012);
}

/// Inside the canopy: the windscreen arch's lining and its three rear-view
/// mirrors, the canopy's frame along the sills.
fn canopy_fittings(m: &mut Mesh) {
    let za = -5.55;
    let [_, w, zt] = canopy_at(za);
    let z0 = fuse_at(-za)[2] - 0.04;
    let arc = |a: f32, grow: f32| Vec3::new((w - grow) * a.cos(), z0 + (zt - grow - z0) * a.sin(), za);
    // The arch's inner face: a broad black band with a row of bolts.
    let n = 40;
    let band: Vec<Vec<SP>> = (0..=n).map(|i| {
        let a = PI * i as f32 / n as f32;
        (0..=2).map(|j| {
            let dz = [-0.035, 0.0, 0.030][j];
            let g = [0.034, 0.040, 0.034][j];
            (arc(a, g) + Vec3::Z * dz, [a, dz], 0.0)
        }).collect()
    }).collect();
    m.grid(&BLK, &band, false, false);
    for i in 1..24 {
        let a = PI * i as f32 / 24.0;
        let p = arc(a, 0.041);
        let nrm = -(Vec3::new(p.x, p.y - z0, 0.0)).normalize();
        m.screw(&SCREW_BLACK, p + Vec3::Z * 0.008, nrm, 0.0028, i as f32);
    }
    // Mirrors: top centre and one each side, angled to show the pilot what is behind.
    for (a, yaw) in [(FRAC_PI_2, 0.0f32), (FRAC_PI_2 + 0.62, 0.30), (FRAC_PI_2 - 0.62, -0.30)] {
        let p = arc(a, 0.050) + Vec3::Z * 0.035;
        let down = -(Vec3::new(p.x, p.y - z0, 0.0)).normalize();
        let rot = Quat::from_rotation_arc(Vec3::NEG_Y, down) * Quat::from_rotation_y(yaw);
        m.tube(&BLK, &[arc(a, 0.040), p], &|_| 0.005, 8, false);
        let face_rot = rot * Quat::from_rotation_x(-0.55);
        m.rbox(&BLK, p + rot * Vec3::new(0.0, -0.012, 0.01), Vec3::new(0.050, 0.020, 0.007), 0.005, face_rot);
        let c = p + rot * Vec3::new(0.0, -0.012, 0.01) + face_rot * Vec3::new(0.0, 0.0, 0.0085);
        m.rbox(&MIRROR, c, Vec3::new(0.045, 0.016, 0.0006), 0.004, face_rot);
    }
    // The canopy's own frame along each sill: rounded, black inside.
    for s in [-1.0f32, 1.0] {
        let rail: Vec<Vec3> = (0..=36).map(|i| {
            let z = -6.15 + 3.40 * i as f32 / 36.0;
            Vec3::new(s * (canopy_at(z)[1] - 0.012), sill_top(z) + 0.022, z)
        }).collect();
        m.tube(&BLK, &rail, &|_| 0.013, 14, true);
    }
}

// ---------------------------------------------------------------- instrument panel

/// The upper panel (DDIs and UFC), leaning back 18 degrees.
fn upper_face() -> Face {
    let t = 18f32.to_radians();
    Face::new(Vec3::new(0.0, 0.635, -5.27), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}
/// The lower panel (AMPCD, IFEI, standby instruments), further forward and steeper.
fn lower_face() -> Face {
    let t = 8f32.to_radians();
    Face::new(Vec3::new(0.0, 0.385, -5.425), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}
/// A face at (u, v) on f, turned about its up axis to look toward the eye by `share`.
fn toward_eye(f: &Face, u: f32, v: f32, share: f32) -> Face {
    let o = f.at(u, v);
    let n = f.n().lerp((EYE - o).normalize(), share).normalize();
    let ax = f.ay.cross(n).normalize();
    Face::new(o, ax, n.cross(ax))
}

const DDI_U: f32 = 0.205;
const DDI_H: f32 = 0.090;

fn upper_panel(m: &mut Mesh, f: &Face) {
    // The panel itself, and its side wings angled in toward the seat.
    m.slab(&BLK, &rrect(0.335, 0.122, 0.01), &[], f.at_h(0.0, 0.0, 0.0), f.ax, f.ay, 0.03);
    for s in [-1.0f32, 1.0] {
        let e = f.at(s * 0.335, 0.0);
        let dx = (s * (wall_x(e.z, e.y) + 0.004) - e.x).abs();
        let out = Vec3::new(s * (wall_x(e.z, e.y) + 0.004), e.y, e.z + 0.55 * dx);
        let wing = Face::new((e + out) * 0.5, (out - e) * s, f.ay);
        let hw = ((out - e).length() * 0.5 + 0.004).max(0.03);
        m.slab(&BLK, &rrect(hw, 0.122, 0.006), &[], wing.o, wing.ax, wing.ay, 0.02);
        let g = wing.sub(0.0, 0.0, 0.0);
        if s < 0.0 {
            master_arm(m, &g);
        } else {
            fire_panel(m, &g);
        }
    }
    // The two DDIs, canted toward the pilot, and the UFC between them.
    let l = toward_eye(f, -DDI_U, 0.0, 0.55);
    ddi(m, &l, screens::DDI_L, DDI_H, 1.0);
    let r = toward_eye(f, DDI_U, 0.0, 0.55);
    ddi(m, &r, screens::DDI_R, DDI_H, 7.0);
    ufc(m, &f.sub(0.0, 0.032, 0.0));
    hud_controls(m, &f.sub(0.0, -0.082, 0.0));
}

/// What `mat::DISPLAY` extra the screens get (pit18.wgsl `display18`).
pub mod screens {
    pub const DDI_L: f32 = 40.0;
    pub const DDI_R: f32 = 41.0;
    pub const AMPCD: f32 = 42.0;
    pub const SCRATCH: f32 = 43.0;
    /// 44..48 the option windows, top to bottom
    pub const OPTION: f32 = 44.0;
    pub const COMM1: f32 = 49.0;
    pub const COMM2: f32 = 50.0;
    pub const IFEI_L: f32 = 51.0;
    pub const IFEI_R: f32 = 52.0;
    pub const RWR: f32 = 53.0;
}

/// A display: the octagonal bezel, five buttons a side, mode, brightness and
/// contrast knobs in its corners, the screen recessed behind it.
fn ddi(m: &mut Mesh, f: &Face, id: f32, h: f32, seed: f32) {
    let s = h * 0.71;
    m.slab(&BEZEL, &octagon(h, h, h * 0.26), &[ccw(rrect(s + 0.003, s + 0.003, 0.004))], f.at_h(0.0, 0.0, 0.018), f.ax, f.ay, 0.018);
    // The lip round the glass, and the screen behind it.
    m.slab(&BLK, &rrect(s + 0.003, s + 0.003, 0.004), &[ccw(rrect(s, s, 0.002))], f.at_h(0.0, 0.0, 0.009), f.ax, f.ay, 0.002);
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(0.0, 0.0, 0.0085), f.ax, f.ay, s, s);
    let g = f.sub(0.0, 0.0, 0.018);
    let e = h - 0.0125;
    for k in 0..5 {
        let t = (k as f32 - 2.0) * 0.024;
        for (u, v, rot) in [(t, e, 0.0f32), (t, -e, 0.0), (-e, t, FRAC_PI_2), (e, t, FRAC_PI_2)] {
            let q = g.rot() * Quat::from_rotation_z(rot);
            m.rbox(&BTN, g.at_h(u, v, 0.0022), Vec3::new(0.0062, 0.0050, 0.0024), 0.0016, q);
        }
    }
    let c = h - h * 0.13 - 0.004;
    knob(m, &g, -c, c, 0.0055, seed, false);
    knob(m, &g, -c, -c, 0.0050, seed + 1.0, false);
    knob(m, &g, c, -c, 0.0050, seed + 2.0, false);
    word(m, &g, -c + 0.003, c + 0.011, 0.003, "NIGHT");
    word(m, &g, -c - 0.001, -c - 0.010, 0.0028, "BRT");
    word(m, &g, c + 0.0, -c - 0.010, 0.0028, "CONT");
    for (du, dv) in [(-1.0f32, 0.62f32), (1.0, 0.62), (-1.0, -0.62), (1.0, -0.62)] {
        m.screw(&SCREW_BLACK, g.at_h(du * (h - 0.004), dv * (h - 0.03), 0.0), g.n(), 0.0018, seed + du + dv);
    }
}

/// The up-front control: keypad, scratchpad, the five option windows and
/// their select buttons, the radio channel knobs and the function buttons.
fn ufc(m: &mut Mesh, f: &Face) {
    let (hw, hh) = (0.086, 0.088);
    m.slab(&BEZEL, &rrect(hw, hh, 0.006), &[], f.at_h(0.0, 0.0, 0.012), f.ax, f.ay, 0.012);
    let g = f.sub(0.0, 0.0, 0.012);
    // Scratchpad and option windows, each sunk in a black well.
    let win = |m: &mut Mesh, u: f32, v: f32, w: f32, h: f32, id: f32| {
        m.rbox(&BLK, g.at_h(u, v, 0.0004), Vec3::new(w + 0.003, h + 0.003, 0.0012), 0.0012, g.rot());
        screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), g.at_h(u, v, 0.0018), g.ax, g.ay, w, h);
    };
    win(m, -0.024, 0.068, 0.034, 0.0085, screens::SCRATCH);
    for k in 0..5 {
        let v = 0.068 - k as f32 * 0.0245;
        win(m, 0.046, v, 0.022, 0.0082, screens::OPTION + k as f32);
        rbutton(m, &g, 0.0165, v, 0.0042);
    }
    // Keypad: 1 2 3 / 4 5 6 / 7 8 9 / CLR 0 ENT.
    let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "CLR", "0", "ENT"];
    for (i, k) in keys.iter().enumerate() {
        let (c, r) = ((i % 3) as f32, (i / 3) as f32);
        pbutton(m, &g, -0.044 + c * 0.0205, 0.044 - r * 0.021, 0.0080, 0.0072, style::KEY, sid(k));
    }
    // Left column: I/P, the ADF switch, volume, COMM 1 and its channel window.
    let lx = -0.074;
    rbutton(m, &g, lx, 0.066, 0.0042);
    word(m, &g, lx, 0.076, 0.0026, "I/P");
    toggle(m, &g, lx, 0.044, true);
    word(m, &g, lx, 0.053, 0.0026, "ADF");
    knob(m, &g, lx, 0.018, 0.0045, 3.0, false);
    word(m, &g, lx, 0.028, 0.0026, "VOL");
    word(m, &g, lx, 0.005, 0.0024, "COMM 1");
    win(m, lx, -0.010, 0.0065, 0.0065, screens::COMM1);
    // Right column: brightness, EMCON, volume, COMM 2.
    let rx = 0.078;
    knob(m, &g, rx, 0.066, 0.0042, 4.0, false);
    word(m, &g, rx, 0.076, 0.0026, "BRT");
    rbutton(m, &g, rx, 0.044, 0.0042);
    word(m, &g, rx, 0.0525, 0.0024, "EMCON");
    knob(m, &g, rx, 0.018, 0.0045, 5.0, false);
    word(m, &g, rx, 0.028, 0.0026, "VOL");
    word(m, &g, rx, 0.005, 0.0024, "COMM 2");
    win(m, rx, -0.010, 0.0065, 0.0065, screens::COMM2);
    // Function buttons along the bottom, the channel knobs at the corners.
    for (i, s) in ["A/P", "IFF", "TCN", "ILS", "D/L", "BCN", "ON/OFF"].iter().enumerate() {
        pbutton(m, &g, -0.0515 + i as f32 * 0.0172, -0.040, 0.0070, 0.0058, style::KEY, sid(s));
    }
    knob_alu(m, &g, -0.072, -0.066, 0.0085);
    knob_alu(m, &g, 0.072, -0.066, 0.0085);
    for (du, dv) in [(-1.0f32, 1.0f32), (1.0, 1.0)] {
        m.screw(&SCREW_BLACK, g.at(du * (hw - 0.005), dv * (hh - 0.005)), g.n(), 0.0017, du * 3.0 + dv);
    }
}

/// The HUD's control panel under the UFC: knobs and switches with their legends.
fn hud_controls(m: &mut Mesh, f: &Face) {
    m.slab(&BLK, &rrect(0.086, 0.036, 0.004), &[], f.at_h(0.0, 0.0, 0.006), f.ax, f.ay, 0.006);
    let g = f.sub(0.0, 0.0, 0.006);
    let items: [(f32, f32, u8, &str); 9] = [
        (-0.068, 0.010, 1, "NORM"), (-0.040, 0.012, 0, "BRT"), (-0.012, 0.010, 1, "DAY"), (0.016, 0.012, 2, "BLK LVL"), (0.046, 0.010, 1, "W/B"),
        (0.072, 0.012, 0, "BAL"), (-0.040, -0.020, 2, "AOA"), (-0.010, -0.020, 1, "ALT"), (0.022, -0.020, 1, "ATT"),
    ];
    for (k, (u, v, t, s)) in items.iter().enumerate() {
        match t {
            0 => knob(m, &g, *u, *v - 0.004, 0.0062, k as f32, false),
            1 => toggle(m, &g, *u, *v - 0.003, k % 2 == 0),
            _ => knob(m, &g, *u, *v - 0.004, 0.0048, k as f32 + 9.0, false),
        }
        word(m, &g, *u, *v + 0.0125, 0.0027, s);
    }
    word(m, &g, 0.003, -0.033, 0.0024, "BARO");
    word(m, &g, 0.040, -0.026, 0.0024, "INS");
}

/// Left of the left DDI: fire extinguisher, master arm, A/A and A/G, the emergency jettison button.
fn master_arm(m: &mut Mesh, f: &Face) {
    light(m, f, 0.0, 0.088, 0.022, 0.014, sid("FIRE"), &RED_LENS);
    light(m, f, 0.0, 0.058, 0.020, 0.010, sid("MASTER CAUTION"), &mt(mat::LAMP, 1.0, 0.55, 0.05).x(lamp::ANNUNC));
    guarded(m, f, -0.012, 0.022, true);
    word(m, f, 0.012, 0.022, 0.0026, "EXTGH");
    pbutton(m, f, 0.012, -0.010, 0.010, 0.007, style::BUTTON, sid("A/A"));
    pbutton(m, f, 0.012, -0.030, 0.010, 0.007, style::BUTTON, sid("A/G"));
    toggle(m, f, -0.016, -0.020, false);
    word(m, f, -0.016, -0.004, 0.0026, "MASTER ARM");
    m.lathe(&stripes(0.004, false), f.at(0.0, -0.075), f.n(), &[[0.017, 0.0], [0.017, 0.004], [0.0, 0.004]], 20, false);
    m.lathe(&BEZEL, f.at_h(0.0, -0.075, 0.004), f.n(), &[[0.011, 0.0], [0.011, 0.006], [0.009, 0.008], [0.0, 0.008]], 18, false);
    word(m, f, 0.0, -0.100, 0.0028, "EMERG JETT");
}

/// Right of the right DDI: the right engine's and the APU's fire lights, IR cooling, the HMD knob, the spin recovery switch.
fn fire_panel(m: &mut Mesh, f: &Face) {
    light(m, f, 0.0, 0.088, 0.022, 0.014, sid("FIRE"), &RED_LENS);
    light(m, f, 0.0, 0.058, 0.020, 0.010, sid("APU FIRE"), &RED_LENS);
    guarded(m, f, -0.01, 0.022, false);
    word(m, f, -0.01, 0.038, 0.0026, "IR COOL");
    knob(m, f, 0.0, -0.012, 0.0055, 2.0, true);
    word(m, f, 0.0, 0.004, 0.0026, "HMD");
    // The spin recovery switch under its striped guard, on a striped plate.
    m.rbox(&stripes(0.006, false), f.at_h(0.0, -0.068, 0.002), Vec3::new(0.022, 0.03, 0.002), 0.002, f.rot());
    guarded(m, f, 0.0, -0.066, true);
    word(m, f, 0.0, -0.034, 0.0028, "SPIN");
}

/// The glareshield's hood over the upper panel, running forward under the windscreen.
fn hood(m: &mut Mesh) {
    // The hood's rear edge seen from the seat: a broad flat top, rounded
    // corners, the sides coming down beside the DDIs.
    let rear: Vec<Vec3> = catmull(&[
        Vec3::new(-0.488, 0.47, -5.17), Vec3::new(-0.494, 0.60, -5.18), Vec3::new(-0.482, 0.70, -5.20), Vec3::new(-0.43, 0.755, -5.225),
        Vec3::new(-0.26, 0.778, -5.25), Vec3::new(0.0, 0.782, -5.26), Vec3::new(0.26, 0.778, -5.25), Vec3::new(0.43, 0.755, -5.225),
        Vec3::new(0.482, 0.70, -5.20), Vec3::new(0.494, 0.60, -5.18), Vec3::new(0.488, 0.47, -5.17),
    ], 5);
    let nz = 14;
    let front_z = -6.16;
    let rows: Vec<Vec<SP>> = (0..=nz).map(|i| {
        let t = i as f32 / nz as f32;
        rear.iter().map(|p| {
            let z = p.z + (front_z - p.z) * t;
            // Forward, the hood drops to the skin under the windscreen and spreads to the glass.
            let e = t * t * (3.0 - 2.0 * t);
            let floor = skin_y(z, p.x * 0.9) + 0.005;
            let lim = glass_y(z, p.x) - 0.02;
            let y0 = p.y.min(lim);
            let y = (y0 + (floor.max(0.5) - y0) * e * 0.85).min(lim).max(floor.min(y0));
            (Vec3::new(p.x * (1.0 - 0.04 * t), y, z), [p.x, z], 0.0)
        }).collect()
    }).collect();
    m.grid(&GLARE.pat(pat::PIT), &rows, false, true);
    // Its underside back from the lip, and the thick rolled lip itself.
    let under: Vec<Vec<SP>> = (0..=3).map(|i| {
        let t = i as f32 / 3.0 * 0.11;
        rear.iter().map(|p| (Vec3::new(p.x * 0.985, p.y - 0.016, p.z - t), [p.x, t], 0.0)).collect()
    }).collect();
    m.grid(&GLARE, &under, false, false);
    let lip: Vec<Vec3> = rear.iter().map(|p| *p + Vec3::new(0.0, -0.007, 0.004)).collect();
    m.tube(&GLARE, &lip, &|_| 0.0095, 12, true);
}

/// The HUD on top of the hood: its body, the lock and shoot lights, the
/// combiner's lattice brackets and the camera at their foot.
fn hud(m: &mut Mesh) {
    let body = Vec3::new(0.0, 0.818, -5.37);
    m.rbox(&BLK, body, Vec3::new(0.096, 0.042, 0.13), 0.012, Quat::from_rotation_x(0.06));
    // Its rear face, sloping, with the two lights.
    let rf = Face::new(Vec3::new(0.0, 0.826, -5.236), Vec3::X, Vec3::new(0.0, 1.0, 0.25));
    m.rbox(&BEZEL, rf.o, Vec3::new(0.094, 0.030, 0.006), 0.004, rf.rot());
    let g = rf.sub(0.0, 0.0, 0.006);
    m.lathe(&GREEN_LENS, g.at(0.0, 0.010), g.n(), &[[0.0045, 0.0], [0.0045, 0.002], [0.0, 0.003]], 12, true);
    m.lathe(&mt(mat::LAMP, 1.0, 0.85, 0.1).x(lamp::GEAR_UP), g.at(0.0, -0.006), g.n(), &[[0.0045, 0.0], [0.0045, 0.002], [0.0, 0.003]], 12, true);
    word(&mut *m, &g, 0.024, 0.010, 0.0026, "LOCK");
    word(&mut *m, &g, 0.024, -0.006, 0.0026, "SHOOT");
    knob(m, &g, -0.05, 0.0, 0.004, 7.0, false);
    // The combiner's brackets: angled plates, open in the middle, leaning toward the pilot.
    for s in [-1.0f32, 1.0] {
        let x = s * 0.098;
        let o = Vec3::new(x, 0.862, -5.30);
        let outline = vec![Vec2::new(0.0, 0.0), Vec2::new(-0.05, 0.0), Vec2::new(-0.004, 0.115), Vec2::new(0.018, 0.200), Vec2::new(0.036, 0.200), Vec2::new(0.040, 0.188), Vec2::new(0.018, 0.10)];
        let hole = vec![Vec2::new(-0.012, 0.022), Vec2::new(-0.030, 0.022), Vec2::new(0.004, 0.110), Vec2::new(0.020, 0.170), Vec2::new(0.024, 0.165), Vec2::new(0.008, 0.10)];
        // (u aft along +z, v up; the plate's face looks inboard, 1 cm thick.)
        let (o, ax) = if s > 0.0 { (o - Vec3::X * 0.005, Vec3::Z) } else { (o + Vec3::X * 0.005, -Vec3::Z) };
        let flip = |v: Vec<Vec2>| -> Vec<Vec2> { v.iter().map(|p| Vec2::new(p.x * s, p.y)).collect() };
        m.slab(&COMBINER_FRAME, &wind(flip(outline), false), &[wind(flip(hole), true)], o, ax, Vec3::Y, 0.010);
    }
    // A bar across the foot of the combiner, the camera on its left.
    m.rbox(&COMBINER_FRAME, Vec3::new(0.0, 0.866, -5.285), Vec3::new(0.100, 0.007, 0.012), 0.004, Quat::IDENTITY);
    m.lathe(&BLK, Vec3::new(-0.045, 0.882, -5.27), Vec3::Z, &[[0.0, -0.03], [0.012, -0.03], [0.012, 0.012], [0.009, 0.016], [0.0, 0.016]], 14, false);
    m.disc(&mt(mat::GLASS, 0.05, 0.05, 0.06), Vec3::new(-0.045, 0.882, -5.2535), Vec3::Z, Vec3::Y, 0.007, 12);
    m.rbox(&BLK, Vec3::new(-0.045, 0.873, -5.285), Vec3::new(0.012, 0.008, 0.02), 0.003, Quat::IDENTITY);
}

/// The combiner glass, leaning aft at the top (drawn in the glass pass;
/// pit18.wgsl draws the symbology on it).
fn combiner(m: &mut Mesh) {
    // The outline seen from the seat: wide, its top corners cut.
    let pts = [(-0.090, 0.0), (0.090, 0.0), (0.092, 0.13), (0.062, 0.180), (-0.062, 0.180), (-0.092, 0.13)];
    let base = Vec3::new(0.0, 0.872, -5.262);
    let up = Vec3::new(0.0, 1.0, 0.30).normalize();
    let ring: Vec<Vec3> = pts.iter().map(|&(x, y)| base + Vec3::X * x + up * y).collect();
    let n = Vec3::X.cross(up);
    let c = ring.iter().fold(Vec3::ZERO, |a, p| a + *p) / ring.len() as f32;
    m.fan(&mt(mat::GLASS, 0.1, 0.1, 0.1), c, &ring, n);
}

fn lower_panel(m: &mut Mesh) {
    let f = lower_face();
    // A grey sheet-metal panel, cut away below beside the console fronts.
    let outline = vec![
        Vec2::new(0.372, -0.02), Vec2::new(0.24, -0.02), Vec2::new(0.225, -0.035), Vec2::new(0.225, -0.142), Vec2::new(-0.225, -0.142), Vec2::new(-0.225, -0.035),
        Vec2::new(-0.24, -0.02), Vec2::new(-0.372, -0.02), Vec2::new(-0.372, 0.118), Vec2::new(0.372, 0.118),
    ];
    m.slab(&GREY, &wind(outline, false), &[], f.at_h(0.0, 0.0, 0.0), f.ax, f.ay, 0.02);
    // The shelf from the upper panel's foot back to this one's top.
    let up = upper_face();
    let a = up.at_h(0.0, -0.122, -0.03);
    let bb = f.at(0.0, 0.118);
    quad_to(m, &GREY_DK, [Vec3::new(-0.34, a.y, a.z), Vec3::new(0.34, a.y, a.z), Vec3::new(0.34, bb.y, bb.z), Vec3::new(-0.34, bb.y, bb.z)], EYE);
    ddi(m, &toward_eye(&f, 0.0, -0.005, 0.3), screens::AMPCD, 0.098, 13.0);
    // Rockers at the AMPCD's corners.
    let g = f.sub(0.0, 0.0, 0.0);
    // The IFEI: engine readouts on the left screen, fuel and time on the right, its buttons below.
    let ifei = toward_eye(&g, -0.262, 0.040, 0.35);
    m.rbox(&BLK, ifei.at_h(0.0, 0.0, 0.010), Vec3::new(0.088, 0.072, 0.010), 0.004, ifei.rot());
    let h = ifei.sub(0.0, 0.0, 0.020);
    for (u, id) in [(-0.040, screens::IFEI_L), (0.042, screens::IFEI_R)] {
        m.rbox(&BLK, h.at_h(u, 0.012, 0.0004), Vec3::new(0.039, 0.046, 0.0012), 0.0015, h.rot());
        screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), h.at_h(u, 0.012, 0.0018), h.ax, h.ay, 0.036, 0.043);
    }
    for (i, s) in ["MODE", "QTY", "UP", "DOWN", "ZONE", "ET"].iter().enumerate() {
        pbutton(m, &h, -0.065 + i as f32 * 0.026, -0.052, 0.0085, 0.0058, style::KEY, sid(s));
    }
    // Under the IFEI: the clock and the brake pressure gauge.
    instrument(m, &g, -0.300, -0.072, 0.019, 14.0, 3.0);
    instrument(m, &g, -0.255, -0.072, 0.019, 21.0, 4.0);
    // The standby instruments, the RWR scope above them on the right.
    instrument(m, &g, 0.168, 0.060, 0.034, 1.0, 5.0);
    m.rbox(&BLK, g.at_h(0.288, 0.060, 0.010), Vec3::new(0.050, 0.048, 0.010), 0.004, g.rot());
    let rw = g.sub(0.288, 0.064, 0.020);
    m.disc(&mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(screens::RWR), rw.o, rw.n(), rw.ay, 0.036, 40);
    m.lathe(&BLK, rw.o, rw.n(), &[[0.036, 0.0], [0.036, 0.003], [0.040, 0.004], [0.042, 0.003]], 36, false);
    word(m, &rw, 0.034, -0.044, 0.0028, "INT");
    knob(m, &rw, 0.041, -0.036, 0.004, 9.0, false);
    instrument(m, &g, 0.138, -0.044, 0.026, 0.0, 6.0);
    instrument(m, &g, 0.212, -0.048, 0.030, 2.0, 7.0);
    instrument(m, &g, 0.298, -0.048, 0.030, 5.0, 8.0);
    word(m, &g, 0.168, 0.105, 0.0028, "CRS");
    knob(m, &g, 0.122, 0.096, 0.005, 2.0, false);
    // The ECM panel under the AMPCD, and the striped jettison guard beside it.
    plate(m, &g, -0.105, -0.140, 0.105, -0.108, 31.0);
    let gg = g.sub(0.0, 0.0, 0.0043);
    for i in 0..6 {
        pbutton(m, &gg, -0.07 + i as f32 * 0.028, -0.124, 0.0085, 0.0065, style::BUTTON, 0);
    }
    word(m, &gg, -0.09, -0.112, 0.0026, "ECM");
    m.rbox(&stripes(0.006, false), g.at_h(0.15, -0.125, 0.004), Vec3::new(0.026, 0.014, 0.004), 0.002, g.rot());
    // Left of the IFEI, on a wing turned toward the seat: the landing gear
    // and flap panel (position lights; the gear handle is a piece of its own).
    let e = f.at(-0.372, 0.04);
    let out = Vec3::new(-(wall_x(e.z, e.y) + 0.004), e.y, e.z + 0.07);
    let wing = Face::new((e + out) * 0.5, e - out, f.ay);
    let hw = (e - out).length() * 0.5 + 0.004;
    m.slab(&GREY, &rrect(hw, 0.078, 0.006), &[], wing.o, wing.ax, wing.ay, 0.02);
    let w = wing.sub(0.0, 0.0, 0.0);
    m.rbox(&BLK, w.at_h(0.0, 0.0, 0.003), Vec3::new(hw - 0.006, 0.074, 0.003), 0.003, w.rot());
    let w = w.sub(0.0, 0.0, 0.006);
    word(m, &w, 0.0, 0.066, 0.0028, "LDG GEAR");
    for (i, s) in ["NOSE", "LEFT", "RIGHT"].iter().enumerate() {
        light(m, &w, 0.022, 0.040 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    for (i, s) in ["HALF", "FULL"].iter().enumerate() {
        light(m, &w, 0.022, -0.020 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    toggle(m, &w, -0.022, -0.050, true);
    word(m, &w, -0.022, -0.034, 0.0026, "FLAPS");
    word(m, &w, 0.005, -0.068, 0.0024, "AUTO");
    // The gear handle's slot.
    m.rbox(&BLK, w.at_h(-0.022, 0.030, 0.0), Vec3::new(0.005, 0.034, 0.002), 0.002, w.rot());
}

/// A standby instrument in its black case.
fn instrument(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, id: f32, seed: f32) {
    let rot = f.rot();
    m.rbox(&BLK, f.at_h(u, v, 0.006), Vec3::new(r + 0.008, r + 0.008, 0.006), 0.004, rot);
    let c = f.at_h(u, v, 0.0105);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, f.n(), f.ay, r, 36);
    m.lathe(&BLK, c, f.n(), &[[r, 0.0], [r, 0.005], [r + 0.003, 0.0065], [r + 0.006, 0.006]], 36, false);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (r + 0.0045), v + dv * (r + 0.0045), 0.012), f.n(), 0.0018, seed + k as f32);
    }
}

// ---------------------------------------------------------------- side consoles

fn consoles(m: &mut Mesh) {
    for s in [-1.0f32, 1.0] {
        // Inner side down to the floor, the front facing the pedals.
        quad_to(m, &GREY, [Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z1), Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z0), Vec3::new(s * CON_X, CON_Y, CON_Z0), Vec3::new(s * CON_X, CON_Y, CON_Z1)], Vec3::new(0.0, 0.3, -4.6));
        let xo = |z: f32| s * (wall_x(z, CON_Y) + 0.004);
        quad_to(m, &GREY, [Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z0), Vec3::new(xo(CON_Z0), FLOOR - 0.01, CON_Z0), Vec3::new(xo(CON_Z0), CON_Y, CON_Z0), Vec3::new(s * CON_X, CON_Y, CON_Z0)], Vec3::new(0.0, 0.3, -6.0));
        quad_to(m, &GREY, [Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z1), Vec3::new(xo(CON_Z1), FLOOR - 0.01, CON_Z1), Vec3::new(xo(CON_Z1), CON_Y, CON_Z1), Vec3::new(s * CON_X, CON_Y, CON_Z1)], Vec3::new(0.0, 0.3, -3.0));
        // The top: a grey tray the panels sit in, its rounded inner edge.
        m.patch(&GREY, 2, 16, s > 0.0, |u, v| {
            let z = CON_Z0 + (CON_Z1 - CON_Z0) * v;
            let x = s * CON_X + (xo(z) - s * CON_X) * u;
            (Vec3::new(x, CON_Y, z), [x, z], 0.0)
        });
        m.tube(&GREY, &[Vec3::new(s * CON_X, CON_Y - 0.004, CON_Z0), Vec3::new(s * CON_X, CON_Y - 0.004, CON_Z1)], &|_| 0.006, 10, true);
    }
    // Panels, front to back. The face's u runs inboard to outboard, v aft to forward.
    let left: [(f32, f32, &str); 8] = [(-5.38, -5.25, "ELEC"), (-5.25, -5.06, "FUEL"), (-5.06, -4.92, "EXT LT"), (-4.92, -4.48, ""), (-4.48, -4.30, "APU"), (-4.30, -4.12, "FCS"), (-4.12, -3.95, "COMM"), (-3.95, -3.81, "")];
    let right: [(f32, f32, &str); 7] = [(-5.38, -5.20, "ELEC"), (-5.20, -4.98, "ECS"), (-4.98, -4.76, "INTR LT"), (-4.76, -4.50, "SNSR"), (-4.50, -4.28, "KY-58"), (-4.28, -4.06, "DEFOG"), (-4.06, -3.81, "")];
    let mut seed = 50.0;
    for (s, list) in [(-1.0f32, &left[..]), (1.0, &right[..])] {
        for &(z0, z1, title) in list {
            seed += 7.0;
            let xo = wall_x((z0 + z1) * 0.5, CON_Y) - 0.004;
            // Looking down from the seat: u is x, v forward.
            let f = Face::new(Vec3::new(0.0, CON_Y, 0.0), Vec3::X, -Vec3::Z);
            let (ua, ub) = if s < 0.0 { (-xo, -CON_X - 0.003) } else { (CON_X + 0.003, xo) };
            let (va, vb) = (-z1, -z0);
            if title.is_empty() && s < 0.0 && z0 < -4.6 {
                // The throttle quadrant's panels either side of it.
                plate(m, &f, ua, va, ua + 0.09, vb, seed);
                populate(m, &f, ua + 0.004, va + 0.004, ua + 0.086, vb - 0.004, seed, Some("EXT LT"));
                plate(m, &f, ub - 0.03, va, ub, vb, seed + 1.0);
                quadrant(m, &f, ua + 0.09, ub - 0.03, va, vb);
                continue;
            }
            plate(m, &f, ua, va, ub, vb, seed);
            if title.is_empty() {
                // Map case and a placard.
                rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.7, 0.7, 0.66)), f.at_h((ua + ub) * 0.5, (va + vb) * 0.5, 0.0047), f.ax, f.ay, (ub - ua) * 0.32, (vb - va) * 0.3);
                continue;
            }
            populate(m, &f, ua + 0.004, va + 0.004, ub - 0.004, vb - 0.004, seed, Some(title));
        }
    }
    // The canopy jettison handle on the right wall, the canopy switch below the rail.
    let z = -4.62;
    let x = wall_x(z, 0.52) - 0.012;
    let base = Vec3::new(x, 0.52, z);
    m.rbox(&BLK, base + Vec3::X * 0.006, Vec3::new(0.008, 0.035, 0.05), 0.004, Quat::IDENTITY);
    let h: Vec<Vec3> = catmull(&[base + Vec3::new(-0.004, -0.015, -0.035), base + Vec3::new(-0.030, -0.012, -0.030), base + Vec3::new(-0.034, -0.008, 0.0), base + Vec3::new(-0.030, -0.012, 0.030), base + Vec3::new(-0.004, -0.015, 0.035)], 6);
    m.tube(&stripes(0.012, true), &h, &|_| 0.0065, 10, true);
    let wf = Face::new(Vec3::new(x - 0.001, 0.56, -4.50), Vec3::Z, Vec3::Y);
    word(m, &wf, 0.0, 0.0, 0.004, "CANOPY");
    m.rbox(&BLK, wf.at_h(0.0, -0.025, 0.004), Vec3::new(0.02, 0.015, 0.004), 0.003, wf.rot());
    toggle(m, &wf.sub(0.0, -0.025, 0.008), 0.0, 0.0, true);
    // On the left wall forward: the yellow and black striped handle by the hood.
    let lz = -5.13;
    let lx = -(wall_x(lz, 0.6) - 0.012);
    let pts: Vec<Vec3> = (0..=10).map(|i| Vec3::new(lx, 0.50 + 0.13 * i as f32 / 10.0, lz + 0.01 * i as f32 / 10.0)).collect();
    m.tube(&stripes(0.012, true), &pts, &|_| 0.0055, 10, true);
    m.rbox(&BLK, Vec3::new(lx - 0.004, 0.495, lz), Vec3::new(0.009, 0.009, 0.012), 0.003, Quat::IDENTITY);
    m.rbox(&BLK, Vec3::new(lx - 0.004, 0.635, lz + 0.01), Vec3::new(0.009, 0.009, 0.012), 0.003, Quat::IDENTITY);
}

/// The throttle quadrant's housing: slots, detent markings, the friction lever.
fn quadrant(m: &mut Mesh, f: &Face, u0: f32, u1: f32, v0: f32, v1: f32) {
    let (cu, cv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
    m.rbox(&BLK, f.at_h(cu, cv, 0.012), Vec3::new((u1 - u0) * 0.5, (v1 - v0) * 0.5, 0.012), 0.008, f.rot());
    let g = f.sub(0.0, 0.0, 0.024);
    for x in [THR_X[0], THR_X[1]] {
        let u = if f.ax.x > 0.0 { x } else { -x };
        m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), g.at_h(u, cv + 0.02, 0.0003), Vec3::new(0.004, 0.12, 0.0004), 0.0003, g.rot());
    }
    for (i, s) in ["OFF", "IDLE", "MIL", "MAX"].iter().enumerate() {
        word(m, &g, u1 - 0.012, cv - 0.10 + i as f32 * 0.07, 0.0034, s);
    }
    m.rbox(&BLK, g.at_h(u0 + 0.012, cv - 0.11, 0.01), Vec3::new(0.004, 0.02, 0.01), 0.003, g.rot());
}

// ---------------------------------------------------------------- the seat

/// Points on the seat: `a` forward of the reference point along the pan, `b`
/// up the back, x across.
fn pan(x: f32, a: f32, h: f32) -> Vec3 {
    SRP + Vec3::new(x, a * 0.10 + h, -a)
}
fn back_dir() -> Vec3 {
    Vec3::new(0.0, 1.0, 0.24).normalize()
}
fn back(x: f32, b: f32, h: f32) -> Vec3 {
    let up = back_dir();
    let fwd = Vec3::new(0.0, up.z, -up.y);
    SRP + Vec3::X * x + up * b + fwd * h
}

fn seat(m: &mut Mesh) {
    let up = back_dir();
    let tilt = Quat::from_rotation_arc(Vec3::Y, up);
    let pan_rot = Quat::from_rotation_x(0.10);
    // The bucket: side plates either side of the pan and up the back, the survival kit under the cushion.
    for s in [-1.0f32, 1.0] {
        m.rbox(&SEAT_GREY, pan(s * 0.232, 0.22, -0.06), Vec3::new(0.008, 0.11, 0.25), 0.004, pan_rot);
        m.rbox(&SEAT_GREY, back(s * 0.222, 0.30, -0.045), Vec3::new(0.008, 0.33, 0.05), 0.004, tilt);
        // The seat rails behind it.
        m.rbox(&GREY_DK, back(s * 0.13, 0.48, -0.17), Vec3::new(0.018, 0.55, 0.022), 0.005, tilt);
    }
    m.rbox(&OLIVE_DK, pan(0.0, 0.22, -0.10), Vec3::new(0.21, 0.06, 0.23), 0.02, pan_rot);
    // Cushions: the pan, the back in two parts, the headrest pad.
    m.rbox(&OLIVE, pan(0.0, 0.23, -0.028), Vec3::new(0.212, 0.030, 0.225), 0.022, pan_rot);
    m.rbox(&OLIVE, back(0.0, 0.17, 0.028), Vec3::new(0.19, 0.14, 0.028), 0.022, tilt);
    m.rbox(&OLIVE, back(0.0, 0.47, 0.024), Vec3::new(0.18, 0.15, 0.026), 0.022, tilt);
    m.rbox(&OLIVE_DK, back(0.0, 0.74, 0.030), Vec3::new(0.115, 0.09, 0.032), 0.025, tilt);
    // The back plate behind the cushions, and the headbox with the canopy breakers on top.
    m.rbox(&SEAT_GREY, back(0.0, 0.32, -0.02), Vec3::new(0.215, 0.33, 0.012), 0.006, tilt);
    m.rbox(&SEAT_GREY, back(0.0, 0.76, -0.085), Vec3::new(0.165, 0.15, 0.095), 0.04, tilt);
    for s in [-1.0f32, 1.0] {
        m.lathe(&STEEL, back(s * 0.10, 0.90, -0.06), up, &[[0.010, 0.0], [0.010, 0.012], [0.006, 0.03], [0.0, 0.045]], 10, false);
    }
    // Warning triangles on the headbox's sides.
    for s in [-1.0f32, 1.0] {
        let c = back(s * 0.166, 0.78, -0.085);
        let f = Face::new(c, -Vec3::Z * s, up);
        rect(m, &legend_mat(style::RED_PLACARD, sid("EJECTION SEAT"), SEAT_GREY.col), c + Vec3::X * s * 0.0005, f.ax, f.ay, 0.045, 0.012);
    }
    // Harness: the shoulder straps lying down the back cushion onto the pan,
    // the lap belts from the bucket sides, all ending in their fittings.
    for s in [-1.0f32, 1.0] {
        let sh: Vec<Vec3> = catmull(&[back(s * 0.07, 0.66, 0.05), back(s * 0.075, 0.50, 0.054), back(s * 0.09, 0.26, 0.060), back(s * 0.11, 0.08, 0.06), pan(s * 0.14, 0.05, 0.004), pan(s * 0.15, 0.14, 0.003)], 5);
        strap(m, &WEB, &sh, 0.044, &|p| if p.y > SRP.y + 0.04 { Vec3::new(0.0, up.z, -up.y) } else { Vec3::Y });
        let end = pan(s * 0.15, 0.15, 0.006);
        m.rbox(&ALU, end, Vec3::new(0.022, 0.005, 0.03), 0.003, pan_rot);
        m.rbox(&STEEL, end + Vec3::new(0.0, 0.004, -0.032), Vec3::new(0.02, 0.004, 0.006), 0.002, pan_rot);
        // "UPPER" tags on the right strap.
        if s > 0.0 {
            let p = back(0.083, 0.42, 0.058);
            rect(m, &legend_mat(style::PLACARD, sid("UPPER"), Vec3::new(0.70, 0.66, 0.40)), p, -Vec3::X, up, 0.02, 0.035);
        }
        let lap: Vec<Vec3> = catmull(&[pan(s * 0.235, 0.06, 0.03), pan(s * 0.22, 0.10, 0.004), pan(s * 0.17, 0.24, 0.002), pan(s * 0.10, 0.30, 0.002)], 5);
        strap(m, &WEB, &lap, 0.05, &|_| Vec3::Y);
        m.rbox(&ALU, pan(s * 0.09, 0.31, 0.006), Vec3::new(0.025, 0.005, 0.02), 0.003, pan_rot);
        // Leg restraint lines: red straps from the floor to the seat's front.
        let leg: Vec<Vec3> = catmull(&[Vec3::new(s * 0.20, FLOOR + 0.005, -4.92), Vec3::new(s * 0.21, FLOOR + 0.04, -4.88), pan(s * 0.205, 0.46, -0.05), pan(s * 0.20, 0.40, 0.004)], 5);
        strap(m, &WEB_RED, &leg, 0.022, &|p| if p.z < -4.86 { Vec3::new(0.0, 0.3, 1.0).normalize() } else { Vec3::Y });
        m.lathe(&mt(mat::PLASTIC, 0.05, 0.25, 0.08), pan(s * 0.19, 0.40, 0.008), Vec3::Y, &[[0.016, 0.0], [0.016, 0.004], [0.010, 0.004], [0.010, 0.0]], 16, false);
    }
    // The firing handle at the front of the pan: a striped loop.
    let c = pan(0.0, 0.47, -0.02);
    let loop_: Vec<Vec3> = (0..=20).map(|i| {
        let a = PI * i as f32 / 20.0;
        c + Vec3::new(-0.045 * a.cos(), 0.055 * a.sin(), -0.012 * a.sin())
    }).collect();
    m.tube(&stripes(0.014, true), &loop_, &|_| 0.0075, 10, true);
    m.rbox(&SEAT_GREY, c + Vec3::new(0.0, -0.004, 0.0), Vec3::new(0.06, 0.012, 0.018), 0.004, Quat::IDENTITY);
    // The safe/arm handle on the right of the headbox, the manual override on the left of the bucket.
    m.rbox(&stripes(0.006, false), back(0.18, 0.62, -0.03), Vec3::new(0.012, 0.035, 0.010), 0.004, tilt);
    m.rbox(&stripes(0.006, false), pan(-0.245, 0.12, 0.02), Vec3::new(0.008, 0.014, 0.05), 0.004, pan_rot);
    // The oxygen and comm hose from the seat's left, drooping over the console.
    let hose = catmull(&[pan(-0.22, 0.05, 0.05), pan(-0.24, 0.12, 0.10), Vec3::new(-0.26, CON_Y + 0.025, -4.38), Vec3::new(-0.30, CON_Y + 0.018, -4.25), Vec3::new(-0.36, CON_Y + 0.03, -4.10)], 8);
    m.tube(&HOSE, &hose, &|_| 0.011, 12, true);
}

/// A strap lying on a surface: a thin ribbon along `pts`, `w` wide, lying
/// on the surface whose normal `up(p)` gives.
fn strap(m: &mut Mesh, mm: &Mat, pts: &[Vec3], w: f32, up: &dyn Fn(Vec3) -> Vec3) {
    let n = pts.len();
    let mut along = 0.0;
    let mut top: Vec<Vec<SP>> = Vec::new();
    let mut bot: Vec<Vec<SP>> = Vec::new();
    for i in 0..n {
        if i > 0 {
            along += (pts[i] - pts[i - 1]).length();
        }
        let t = (pts[(i + 1).min(n - 1)] - pts[i.saturating_sub(1)]).normalize();
        let nn = up(pts[i]);
        let side = t.cross(nn).normalize();
        let nn = side.cross(t).normalize();
        let row = |h: f32| -> Vec<SP> {
            (0..=2).map(|j| {
                let a = (j as f32 - 1.0) * 0.5 * w;
                // A slight crown across the webbing.
                let crown = 0.0012 * (1.0 - (2.0 * a / w).powi(2));
                (pts[i] + side * a + nn * (h + crown), [a, along], 0.0)
            }).collect()
        };
        top.push(row(0.0025));
        bot.push(row(0.0005));
    }
    m.grid(mm, &top, false, true);
    m.grid(mm, &bot, false, false);
}

// ---------------------------------------------------------------- what moves

/// The throttles' slots (x, body frame) and their pivot under the console.
const THR_X: [f32; 2] = [-0.352, -0.318];
const THR_PIVOT: Vec3 = Vec3::new(0.0, CON_Y - 0.17, -4.70);

fn controls(b: &mut Builder) {
    // The stick: a gimbal ring at its foot turns fore and aft, the stick in it from side to side.
    let mut boot = Mesh::default();
    let prof: Vec<[f32; 2]> = (0..=12).map(|i| {
        let t = i as f32 / 12.0;
        [0.055 - 0.035 * t + 0.006 * (i % 2) as f32, t * 0.12]
    }).collect();
    boot.lathe(&mt(mat::RUBBER, 0.03, 0.03, 0.032), STICK + Vec3::Y * 0.0, Vec3::Y, &prof, 20, true);
    boot.lathe(&GREY_DK, STICK - Vec3::Y * 0.015, Vec3::Y, &[[0.075, 0.0], [0.075, 0.012], [0.062, 0.018], [0.0, 0.018]], 24, false);
    let g = b.add("stick gimbal", G_ENGINE, boot, Anim::hinge(Drive::new(Input::Pitch, 0.20)), STICK, Vec3::X, None);
    let mut st = Mesh::default();
    stick(&mut st);
    b.add("stick", G_ENGINE, st, Anim::hinge(Drive::new(Input::Roll, -0.22)), STICK, Vec3::Z, Some(g));

    // The throttles: an arm up through each slot to its grip.
    for (k, &x) in THR_X.iter().enumerate() {
        let mut t = Mesh::default();
        throttle(&mut t, x, k == 0);
        let piv = Vec3::new(x, THR_PIVOT.y, THR_PIVOT.z);
        b.add("throttle", G_ENGINE, t, Anim::hinge(Drive::new(Input::Throttle, -0.55).offset(0.30)), piv, Vec3::X, None);
    }

    // The landing gear handle: a wheel-shaped knob (red light inside) on an arm moving up and down its slot.
    let f = lower_face();
    let e = f.at(-0.372, 0.04);
    let out = Vec3::new(-(wall_x(e.z, e.y) + 0.004), e.y, e.z + 0.07);
    let wing = Face::new((e + out) * 0.5, e - out, f.ay);
    let w = wing.sub(-0.022, 0.030, 0.0);
    let mut gh = Mesh::default();
    let arm_top = w.at_h(0.0, 0.0, 0.05);
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), arm_top], &|_| 0.0045, 10, false);
    let wheel_c = arm_top + w.n() * 0.004;
    gh.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), wheel_c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.016, 0.0], [0.019, 0.003], [0.019, 0.021], [0.016, 0.024], [0.0, 0.024]], 22, false);
    gh.lathe(&RED_LENS, wheel_c - w.ax * 0.013, w.ax, &[[0.0, 0.0], [0.009, 0.0], [0.009, 0.026], [0.0, 0.026]], 14, false);
    b.add("gear handle", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, 0.028), w.ay), w.o, Vec3::X, None);

    // Rudder pedals, hinged at the top of their arms under the panel.
    for s in [-1.0f32, 1.0] {
        let mut p = Mesh::default();
        let pv = Vec3::new(s * 0.125, 0.40, -5.66);
        let pc = Vec3::new(s * 0.125, 0.235, -5.60);
        p.tube(&GREY_DK, &[pv, pc], &|_| 0.011, 10, true);
        let tilt = Quat::from_rotation_x(-0.42);
        p.rbox(&GREY_DK, pc + Vec3::new(0.0, 0.0, 0.012), Vec3::new(0.048, 0.075, 0.009), 0.006, tilt);
        p.rbox(&mt(mat::RUBBER, 0.03, 0.03, 0.032).pat(pat::KNURL), pc + Vec3::new(0.0, 0.003, 0.021), Vec3::new(0.042, 0.066, 0.002), 0.0015, tilt);
        // The heel guard.
        p.rbox(&GREY_DK, pc + Vec3::new(0.0, -0.075, 0.035), Vec3::new(0.05, 0.012, 0.006), 0.004, Quat::from_rotation_x(0.6));
        b.add("rudder pedal", G_ENGINE, p, Anim::hinge(Drive::new(Input::Yaw, s * 0.16)), pv, Vec3::X, None);
    }
    // The pedals' box and the cross-tube they hang from.
    let mut pb = Mesh::default();
    pb.tube(&GREY_DK, &[Vec3::new(-0.20, 0.40, -5.66), Vec3::new(0.20, 0.40, -5.66)], &|_| 0.014, 12, true);
    pb.rbox(&GREY_DK, Vec3::new(0.0, 0.42, -5.75), Vec3::new(0.22, 0.04, 0.10), 0.01, Quat::IDENTITY);
    b.fixed("pedal box", G_ENGINE, pb);
}

/// The stick from its foot: a straight shaft, then the grip curving forward
/// into its head with the trigger, the weapon release and trim buttons, the
/// sensor control switch and the autopilot paddle.
fn stick(m: &mut Mesh) {
    let base = STICK + Vec3::Y * 0.08;
    let s0 = base + Vec3::new(0.0, 0.15, 0.010);
    m.tube(&mt(mat::BLACK, 0.025, 0.025, 0.027), &[STICK + Vec3::Y * 0.02, base, s0], &|_| 0.0135, 14, false);
    // The grip: a loft of ovals along a curve, wider fore and aft than across.
    let spine = catmull(&[s0, s0 + Vec3::new(0.0, 0.04, -0.004), s0 + Vec3::new(0.0, 0.10, -0.016), s0 + Vec3::new(0.0, 0.145, -0.042), s0 + Vec3::new(0.0, 0.158, -0.070)], 6);
    let n = spine.len();
    let grip = mt(mat::PLASTIC, 0.022, 0.022, 0.024).pat(pat::KNURL);
    let rows: Vec<Vec<SP>> = (0..n).map(|i| {
        let t = i as f32 / (n - 1) as f32;
        let tan = (spine[(i + 1).min(n - 1)] - spine[i.saturating_sub(1)]).normalize();
        let side = Vec3::X;
        let fwd = side.cross(tan).normalize();
        // half width across, half depth fore and aft; swelling under the palm, the head domed
        let a = 0.0145 + 0.006 * (t * PI).sin() + 0.004 * smoothstep(0.75, 0.9, t);
        let d = 0.016 + 0.010 * (t * PI * 0.9).sin() + 0.006 * smoothstep(0.7, 0.9, t);
        let cap = if i == n - 1 { 0.3 } else { 1.0 };
        (0..=16).map(|k| {
            let ang = k as f32 / 16.0 * TAU;
            let p = spine[i] + side * (ang.cos() * a * cap) + fwd * (ang.sin() * d * cap);
            (p, [ang * a, t * 0.16], 0.0)
        }).collect()
    }).collect();
    m.grid(&grip, &rows, true, false);
    let top = spine[n - 1];
    m.dome(&grip, top - Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.4, -1.0).normalize(), 0.012, 0.008, 14, 0.0);
    // Head: the weapon release (red) on the left, the trim hat in the middle, the castle switch on the right.
    let head = s0 + Vec3::new(0.0, 0.150, -0.030);
    m.dome(&mt(mat::PLASTIC, 0.40, 0.025, 0.02), head + Vec3::new(-0.012, 0.012, 0.0), Vec3::new(-0.3, 1.0, 0.1).normalize(), 0.0055, 0.004, 12, 0.0);
    m.lathe(&BLK, head + Vec3::new(0.002, 0.016, -0.004), Vec3::Y, &[[0.0045, 0.0], [0.0045, 0.003], [0.0025, 0.0045], [0.0, 0.0048]], 10, false);
    m.rbox(&BLK, head + Vec3::new(0.016, 0.004, -0.006), Vec3::new(0.004, 0.006, 0.006), 0.002, Quat::IDENTITY);
    // The gun trigger in its guard at the front, the paddle switch low on the front.
    let tg = s0 + Vec3::new(0.0, 0.095, -0.036);
    m.tube(&BLK, &catmull(&[tg + Vec3::new(0.0, 0.020, 0.010), tg + Vec3::new(0.0, 0.010, -0.010), tg + Vec3::new(0.0, -0.012, -0.006)], 4), &|_| 0.0045, 8, true);
    m.tube(&BLK, &catmull(&[tg + Vec3::new(0.0, 0.03, 0.012), tg + Vec3::new(0.0, 0.025, -0.030), tg + Vec3::new(0.0, -0.025, -0.022), tg + Vec3::new(0.0, -0.035, 0.008)], 4), &|_| 0.0025, 6, true);
    m.rbox(&BLK, s0 + Vec3::new(0.0, 0.045, -0.028), Vec3::new(0.012, 0.010, 0.004), 0.002, Quat::from_rotation_x(0.3));
    // The nose wheel steering button on the right side.
    m.dome(&BLK, s0 + Vec3::new(0.022, 0.08, -0.004), Vec3::X, 0.004, 0.003, 10, 0.0);
}

/// One throttle grip: the left one wide, with its switches (speedbrake, comm,
/// cage), the right one with the designator controller on top.
fn throttle(m: &mut Mesh, x: f32, left: bool) {
    let piv = Vec3::new(x, THR_PIVOT.y, THR_PIVOT.z);
    let top = piv + Vec3::Y * 0.235;
    m.rbox(&mt(mat::STEEL, 0.25, 0.25, 0.26), piv + Vec3::Y * 0.17, Vec3::new(0.005, 0.08, 0.012), 0.003, Quat::IDENTITY);
    let grip = mt(mat::PLASTIC, 0.024, 0.024, 0.026);
    let w = 0.016;
    let c = top + Vec3::new(if left { -0.006 } else { 0.004 }, 0.035, -0.008);
    m.rbox(&grip, c, Vec3::new(w + if left { 0.004 } else { 0.0 }, 0.042, 0.034), 0.014, Quat::from_rotation_x(-0.15));
    m.rbox(&grip, c + Vec3::new(0.0, 0.032, -0.022), Vec3::new(w, 0.016, 0.022), 0.012, Quat::from_rotation_x(-0.5));
    let sw = c + Vec3::new(0.0, 0.040, -0.040);
    if left {
        // Speedbrake (a slide switch), comm (a hat), cage/uncage.
        m.rbox(&BLK, c + Vec3::new(-0.021, 0.02, -0.02), Vec3::new(0.003, 0.012, 0.004), 0.0015, Quat::IDENTITY);
        m.lathe(&BLK, sw, Vec3::new(0.0, 0.5, -1.0).normalize(), &[[0.004, 0.0], [0.004, 0.003], [0.0, 0.004]], 10, false);
        m.rbox(&mt(mat::PLASTIC, 0.06, 0.06, 0.062), c + Vec3::new(-0.022, -0.008, -0.03), Vec3::new(0.002, 0.005, 0.005), 0.0015, Quat::IDENTITY);
    } else {
        // The target designator: a pressure-sensitive pad on top.
        m.lathe(&mt(mat::RUBBER, 0.05, 0.05, 0.05), c + Vec3::new(0.0, 0.050, -0.022), Vec3::new(0.0, 1.0, -0.4).normalize(), &[[0.0065, 0.0], [0.0065, 0.003], [0.005, 0.005], [0.0, 0.0055]], 12, false);
        m.rbox(&BLK, c + Vec3::new(0.018, 0.015, -0.01), Vec3::new(0.003, 0.006, 0.006), 0.0015, Quat::IDENTITY);
    }
    let _ = sw;
}

/// Looking down or behind from the seat, the view is mostly of the cockpit.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let h = Vec2::new(d.x, d.z);
    let hl = h.length().max(1e-4);
    if d.z > 0.0 && d.x.abs() < 0.35 * hl && d.y < 0.2 {
        return true; // the headbox
    }
    // How far out the cockpit's edge is that way, and how far below the eye.
    let fwd = (-d.z / hl).max(0.0);
    let side = (d.x / hl).abs();
    let reach = 1.0 / ((fwd / 0.66).powi(2) + (side / 0.50).powi(2)).sqrt().max(1e-3);
    let drop = eye.y - (0.78 * fwd + 0.63 * (1.0 - fwd)).max(0.6);
    d.y / hl < -drop / reach
}
