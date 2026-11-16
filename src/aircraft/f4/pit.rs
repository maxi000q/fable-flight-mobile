//! The F-4E's two cockpits, one behind the other under the long canopy: a
//! 1960s office of round dials. In front, the pilot's panel under its glare
//! shield: the radar scope at the top, the attitude indicator and the
//! horizontal situation indicator below it, airspeed and Mach to the left,
//! altitude and vertical speed to the right, the engines' instruments in two
//! columns on the far right (rpm, exhaust temperature, nozzle position, fuel
//! flow), the landing gear handle on the left; the optical sight's glass
//! stands on the glare shield. Behind, the weapon systems officer's panel
//! with its big radar scope under a hood, on the bulkhead between the seats.
//!
//! The seats, the stick, the throttles and the pedals are the Hornet's
//! (`fa18::pit`: Martin-Baker seats, both of them), moved to where they sit
//! here; so are the switches, knobs and plates the consoles are filled with.
//! The dials are faces of plane.wgsl's `gauge`, from the instrument readings.
//! Everything here is body frame (+X right, +Y up, -Z forward).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, sid, style, Face};
use crate::aircraft::c172::cabin::gauge;
use crate::aircraft::fa18::pit as hp;
use glam::{Mat4, Quat, Vec2, Vec3};
use hp::{BEZEL, BLK, GLARE, GREEN_LENS, GREY, GREY_DK, RED_LENS};
use std::f32::consts::{FRAC_PI_2, PI};

// ---------------------------------------------------------------- where things are

/// From the Hornet's cockpit to the front one here (seat to seat), and to the rear one.
const FRONT: Vec3 = Vec3::new(0.0, 0.08, -0.55);
const REAR: Vec3 = Vec3::new(0.0, 0.10, 0.90);
/// The floors, and the tops of the side consoles (inner edge `hp::CON_X`).
const FLOOR: f32 = hp::FLOOR + FRONT.y;
const FLOOR_R: f32 = hp::FLOOR + REAR.y;
const CON_Y: f32 = hp::CON_Y + FRONT.y;
const CON_Y_R: f32 = hp::CON_Y + REAR.y;
/// Front of the pedal well, the bulkhead between the cockpits, the back of the rear one.
const Z0: f32 = -6.53;
const Z_MID: f32 = -4.42;
const Z1: f32 = -2.78;

/// The canopy (f4.py CANOPY): Blender station, half width at the sill, height of the top.
const CANOPY: [[f32; 3]; 11] = [
    [6.55, 0.36, 0.48], [6.25, 0.42, 0.80], [5.90, 0.47, 1.06], [5.55, 0.50, 1.22], [5.10, 0.52, 1.31], [4.60, 0.53, 1.35],
    [4.10, 0.54, 1.38], [3.60, 0.54, 1.39], [3.10, 0.53, 1.38], [2.70, 0.50, 1.36], [2.40, 0.46, 1.34],
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
/// The inner face of the canopy sill (f4.py: w - 0.022), and its top.
fn sill_x(z: f32) -> f32 {
    canopy_at(z)[1] - 0.025
}
fn sill_top(z: f32) -> f32 {
    fuse_at(-z)[2] - 0.022
}
/// The cockpits' side walls: inside the sill, and inside the skin below it.
fn wall_x(z: f32, y: f32) -> f32 {
    let foot = skin_y(z, sill_x(z)) - 0.04;
    (sill_x(z) - 0.004).min(skin_hw(z, y.min(foot)) - 0.035).max(0.26)
}
/// Height of the canopy glass's inside at station z, x out (f4.py canopy_arc).
fn glass_y(z: f32, x: f32) -> f32 {
    let [_, w, zt] = canopy_at(z);
    let z0 = fuse_at(-z)[2] - 0.04;
    let c = (x.abs() / w).min(1.0);
    z0 + (zt - z0) * (1.0 - c * c).sqrt() - 0.006
}

/// A piece of the Hornet's cockpit, moved by `d`.
fn moved(build: impl Fn(&mut Mesh), d: Vec3) -> Mesh {
    let mut m = Mesh::default();
    build(&mut m);
    m.transform(0, Mat4::from_translation(d));
    m
}

// ---------------------------------------------------------------- the cockpits

pub fn build(b: &mut Builder) {
    let mut tub = Mesh::default();
    walls(&mut tub);
    bulkheads(&mut tub);
    canopy_fittings(&mut tub);
    b.fixed("cockpit tub", G_ENGINE, tub);

    let mut p = Mesh::default();
    let f = panel_face();
    panel(&mut p, &f);
    hood(&mut p);
    sight(&mut p);
    rear_panel(&mut p);
    b.fixed("instrument panel", G_ENGINE, p);

    let mut c = Mesh::default();
    // Panels, front to back: (from, to, title); an untitled one is the throttle quadrant, "-" a blank plate with a map case.
    consoles(
        &mut c, CON_Y, FLOOR, -5.80, Z_MID, 60.0,
        &[(-5.80, -5.63, "ELEC"), (-5.63, -5.47, "FUEL"), (-5.47, -5.03, ""), (-5.03, -4.85, "COMM"), (-4.85, -4.65, "FCS"), (-4.65, -4.43, "OIL")],
        &[(-5.80, -5.60, "INTR LT"), (-5.60, -5.38, "IFF"), (-5.38, -5.16, "NAV"), (-5.16, -4.92, "TCN"), (-4.92, -4.68, "ILS"), (-4.68, -4.43, "DEFOG")],
    );
    consoles(
        &mut c, CON_Y_R, FLOOR_R, -4.18, -3.05, 160.0,
        &[(-4.18, -3.90, "COMM"), (-3.90, -3.58, "SNSR"), (-3.58, -3.06, "-")],
        &[(-4.18, -3.88, "NAV"), (-3.88, -3.56, "INS"), (-3.56, -3.06, "-")],
    );
    b.fixed("consoles", G_ENGINE, c);

    b.fixed("ejection seat", G_ENGINE, moved(seat, FRONT));
    b.fixed("rear ejection seat", G_ENGINE, moved(seat, REAR));

    let mut g = Mesh::default();
    sight_glass(&mut g);
    let k = b.fixed("sight glass", G_ENGINE, g);
    b.pieces[k].glass = true;

    controls(b, &f);
}

/// A Martin-Baker seat (the Hornet's, where the Hornet's sits) with what the
/// older marks had over the headrest: the face blind's two firing handles.
fn seat(m: &mut Mesh) {
    hp::seat(m);
    let up = Vec3::new(0.0, 1.0, 0.24).normalize();
    let fwd = Vec3::new(0.0, up.z, -up.y);
    for s in [-1.0f32, 1.0] {
        let c = hp::SRP + Vec3::X * (s * 0.055) + up * 0.885 + fwd * 0.012;
        let pts: Vec<Vec3> = (0..=12).map(|i| {
            let a = PI * i as f32 / 12.0;
            c + Vec3::X * (0.036 * a.cos()) + fwd * (0.050 * a.sin()) + up * (0.012 * a.sin())
        }).collect();
        m.tube(&hp::stripes(0.014, true), &pts, &|_| 0.006, 8, true);
    }
}

// ---------------------------------------------------------------- walls, floors, bulkheads

fn walls(m: &mut Mesh) {
    for s in [-1.0f32, 1.0] {
        // The side wall: from the floor up to the canopy sill, following the skin.
        m.patch(&GREY, 64, 12, s < 0.0, |u, v| {
            let z = Z0 + (Z1 - Z0) * u;
            let top = sill_top(z) - 0.004;
            let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
            (Vec3::new(s * wall_x(z, y), y, z), [z, y], 0.0)
        });
        // The rounded inner lip of the sill on top of it, with the canopies' lock hooks.
        let lip: Vec<Vec3> = (0..=56).map(|i| {
            let z = -6.15 + (Z1 + 6.15) * i as f32 / 56.0;
            Vec3::new(s * (sill_x(z) - 0.008), sill_top(z) - 0.007, z)
        }).collect();
        m.tube(&GREY, &lip, &|_| 0.010, 12, true);
        for k in 0..10 {
            let z = -5.55 + k as f32 * 0.29 + if k > 3 { 0.12 } else { 0.0 };
            let p = Vec3::new(s * (sill_x(z) - 0.020), sill_top(z) - 0.012, z);
            m.rbox(&GREY_DK, p, Vec3::new(0.008, 0.012, 0.016), 0.003, Quat::IDENTITY);
            m.bolt(&STEEL, p + Vec3::new(-s * 0.008, 0.0, 0.0), Vec3::X * -s, 0.0035);
        }
        // A row of screws along each wall's panel joint, above the consoles.
        for (za, zb, row) in [(-5.70, -4.50, CON_Y + 0.045), (-4.10, -3.10, CON_Y_R + 0.045)] {
            for k in 0..22 {
                let z = za + (zb - za) * k as f32 / 21.0;
                m.screw(&SCREW_PAINT, Vec3::new(s * (wall_x(z, row) - 0.0005), row, z), Vec3::X * -s, 0.0022, k as f32 + row * 10.0);
            }
        }
    }
    // The floors with their gritty walkways: the rear one a step higher.
    for (za, zb, y) in [(Z0, Z_MID, FLOOR), (Z_MID, Z1, FLOOR_R)] {
        m.patch(&hp::FLOOR_M, 6, 16, true, |u, v| {
            let z = za + (zb - za) * v;
            let x = (u * 2.0 - 1.0) * (wall_x(z, y) + 0.01);
            (Vec3::new(x, y, z), [x, z], 0.0)
        });
    }
}

/// A wall across the tub at station z, from `y0` up to `top(x)`, facing `toward`.
fn across(m: &mut Mesh, mm: &Mat, z: f32, y0: f32, top: impl Fn(f32) -> f32, hw: f32, aft: bool) {
    m.patch(mm, 12, 6, !aft, |u, v| {
        let x = (u * 2.0 - 1.0) * hw;
        let y = y0 + (top(x) - y0) * v;
        (Vec3::new(x, y, z), [x, y], 0.0)
    });
}

/// The pedal well's front, the bulkhead between the cockpits with the rear
/// panel's shroud on top of it, and the back of the rear cockpit.
fn bulkheads(m: &mut Mesh) {
    across(m, &GREY_DK, Z0 + 0.005, FLOOR - 0.02, |x| skin_y(Z0, x) - 0.02, wall_x(Z0, FLOOR) + 0.01, true);
    // Between the seats: its front faces the pilot's seat, its back the WSO's feet.
    let hw = wall_x(Z_MID, 0.5) + 0.01;
    across(m, &GREY, Z_MID - 0.02, FLOOR - 0.02, |_| 0.66, hw, false);
    across(m, &GREY, Z_MID + 0.02, FLOOR_R - 0.02, |_| 0.66, hw, true);
    // The shroud round the rear panel: a flat top with rounded shoulders, shut in front.
    let f = rear_face();
    let rim: Vec<Vec3> = hp::catmull(&[
        Vec3::new(-0.44, 0.60, 0.0), Vec3::new(-0.43, 0.80, 0.0), Vec3::new(-0.36, 0.955, 0.0), Vec3::new(-0.18, 1.005, 0.0), Vec3::new(0.0, 1.012, 0.0),
        Vec3::new(0.18, 1.005, 0.0), Vec3::new(0.36, 0.955, 0.0), Vec3::new(0.43, 0.80, 0.0), Vec3::new(0.44, 0.60, 0.0),
    ], 4);
    let zb = f.o.z + 0.085;
    let rows: Vec<Vec<SP>> = [Z_MID - 0.02, zb].iter().map(|&z| rim.iter().map(|p| (Vec3::new(p.x, p.y, z), [p.x, z], 0.0)).collect()).collect();
    m.grid(&GLARE.pat(pat::PIT), &rows, false, true);
    let lip: Vec<Vec3> = rim.iter().map(|p| Vec3::new(p.x, p.y - 0.006, zb)).collect();
    m.tube(&GLARE, &lip, &|_| 0.008, 10, true);
    let c = Vec3::new(0.0, 0.66, Z_MID - 0.02);
    let ring: Vec<Vec3> = rim.iter().map(|p| Vec3::new(p.x, p.y, Z_MID - 0.02)).collect();
    m.fan(&GREY, c, &ring, -Vec3::Z);
    // Behind the rear seat: the bulkhead under the back of the canopy.
    across(m, &GREY, Z1 - 0.005, FLOOR_R - 0.02, |x| skin_y(Z1, x) - 0.03, wall_x(Z1, 0.5) + 0.01, false);
    let pl = Face::new(Vec3::new(-0.20, 0.52, Z1 - 0.0055), -Vec3::X, Vec3::Y);
    rect(m, &legend_mat(style::RED_PLACARD, sid("DANGER"), GREY.col), pl.o, pl.ax, pl.ay, 0.035, 0.012);
}

/// Inside the canopy: the windscreen arch's lining with its mirror, the
/// canopies' frames along the sills.
fn canopy_fittings(m: &mut Mesh) {
    let za = -5.72;
    let [_, w, zt] = canopy_at(za);
    let z0 = fuse_at(-za)[2] - 0.04;
    let arc = |a: f32, grow: f32| Vec3::new((w - grow) * a.cos(), z0 + (zt - grow - z0) * a.sin(), za);
    let n = 36;
    let band: Vec<Vec<SP>> = (0..=n).map(|i| {
        let a = PI * i as f32 / n as f32;
        (0..=2).map(|j| {
            let dz = [-0.020, 0.0, 0.020][j];
            let g = [0.030, 0.034, 0.030][j];
            (arc(a, g) + Vec3::Z * dz, [a, dz], 0.0)
        }).collect()
    }).collect();
    m.grid(&BLK, &band, false, false);
    for i in 1..20 {
        let a = PI * i as f32 / 20.0;
        let p = arc(a, 0.037);
        m.screw(&SCREW_BLACK, p + Vec3::Z * 0.008, -(Vec3::new(p.x, p.y - z0, 0.0)).normalize(), 0.0028, i as f32);
    }
    // Mirrors on the arch, either side of the top, angled to show what is behind.
    for (a, yaw) in [(FRAC_PI_2 + 0.55, 0.28f32), (FRAC_PI_2 - 0.55, -0.28)] {
        let p = arc(a, 0.046) + Vec3::Z * 0.035;
        let down = -(Vec3::new(p.x, p.y - z0, 0.0)).normalize();
        let rot = Quat::from_rotation_arc(Vec3::NEG_Y, down) * Quat::from_rotation_y(yaw);
        m.tube(&BLK, &[arc(a, 0.036), p], &|_| 0.005, 8, false);
        let face_rot = rot * Quat::from_rotation_x(-0.55);
        let c = p + rot * Vec3::new(0.0, -0.012, 0.01);
        m.rbox(&BLK, c, Vec3::new(0.045, 0.018, 0.007), 0.005, face_rot);
        m.rbox(&hp::MIRROR, c + face_rot * Vec3::new(0.0, 0.0, 0.0085), Vec3::new(0.040, 0.014, 0.0006), 0.004, face_rot);
    }
    for s in [-1.0f32, 1.0] {
        let rail: Vec<Vec3> = (0..=48).map(|i| {
            let z = -6.15 + (Z1 + 6.15) * i as f32 / 48.0;
            Vec3::new(s * (canopy_at(z)[1] - 0.012), sill_top(z) + 0.022, z)
        }).collect();
        m.tube(&BLK, &rail, &|_| 0.013, 12, true);
    }
}

// ---------------------------------------------------------------- the pilot's panel

/// A round instrument in its black case, its face under the bezel's lip.
fn dial(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, id: f32, seed: f32) {
    m.rbox(&BLK, f.at_h(u, v, 0.005), Vec3::new(r + 0.008, r + 0.008, 0.005), 0.004, f.rot());
    let c = f.at_h(u, v, 0.0105);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, f.n(), f.ay, r, 36);
    m.lathe(&BLK, c, f.n(), &[[r, 0.0], [r, 0.005], [r + 0.003, 0.0065], [r + 0.006, 0.006], [r + 0.006, -0.0005]], 36, false);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (r + 0.0045), v + dv * (r + 0.0045), 0.010), f.n(), 0.0018, seed + k as f32);
    }
}

/// The main panel, leaning back 15 degrees.
fn panel_face() -> Face {
    let t = 15f32.to_radians();
    Face::new(Vec3::new(0.0, 0.665, -5.82), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}

/// Where the landing gear handle's slot is on it.
const GEAR_U: f32 = -0.345;
const GEAR_V: f32 = -0.045;

fn panel(m: &mut Mesh, f: &Face) {
    // The panel: its upper corners cut to clear the windscreen.
    let outline = vec![Vec2::new(0.385, -0.20), Vec2::new(-0.385, -0.20), Vec2::new(-0.385, 0.06), Vec2::new(-0.29, 0.18), Vec2::new(0.29, 0.18), Vec2::new(0.385, 0.06)];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    // The radar scope at the top, in its rubber hood.
    let sc = f.sub(0.0, 0.108, 0.0);
    m.lathe(&BEZEL, sc.o, sc.n(), &[[0.066, 0.0], [0.066, 0.012], [0.058, 0.014], [0.058, 0.004]], 36, false);
    m.disc(&mt(mat::GLASS, 0.015, 0.035, 0.02), sc.at_h(0.0, 0.0, 0.004), sc.n(), sc.ay, 0.058, 36);
    m.lathe(&mt(mat::RUBBER, 0.02, 0.02, 0.022), sc.at_h(0.0, 0.0, 0.012), sc.n(), &[[0.064, 0.0], [0.068, 0.03], [0.064, 0.032], [0.059, 0.0]], 36, true);
    for (k, u) in [-0.085f32, 0.085].iter().enumerate() {
        hp::knob(m, &sc, *u, -0.045, 0.0045, 20.0 + k as f32, false);
    }
    // The flight instruments: attitude over heading in the middle, speed to the left, height to the right.
    dial(m, f, 0.0, -0.022, 0.052, gauge::AI, 1.0);
    dial(m, f, 0.0, -0.137, 0.045, gauge::HI, 2.0);
    dial(m, f, -0.125, 0.095, 0.033, gauge::MACH, 3.0);
    dial(m, f, -0.125, 0.0, 0.041, gauge::ASI, 4.0);
    dial(m, f, -0.125, -0.10, 0.036, gauge::TC, 5.0);
    dial(m, f, 0.125, 0.095, 0.033, gauge::JET_FUEL, 6.0);
    dial(m, f, 0.125, 0.0, 0.041, gauge::ALT, 7.0);
    dial(m, f, 0.125, -0.10, 0.036, gauge::VSI, 8.0);
    dial(m, f, -0.215, 0.012, 0.029, gauge::CLOCK, 9.0);
    dial(m, f, -0.215, -0.085, 0.031, gauge::CDI, 10.0);
    let g = f.sub(0.0, 0.0, 0.0004);
    hp::word(m, &g, 0.125, 0.139, 0.0028, "FUEL");
    // Warning lights over the clock.
    hp::light(m, f, -0.215, 0.105, 0.022, 0.011, sid("FIRE"), &RED_LENS);
    hp::light(m, f, -0.215, 0.073, 0.022, 0.009, sid("MASTER CAUTION"), &mt(mat::LAMP, 1.0, 0.55, 0.05).x(lamp::ANNUNC));
    // The engines' instruments, a column for each: rpm, exhaust temperature, nozzle position, fuel flow.
    for (r, (id, name)) in [(gauge::JET_RPM, "RPM"), (gauge::JET_EGT, "TEMP"), (gauge::NOZZLE, "NOZ"), (gauge::JET_FF, "FF")].iter().enumerate() {
        let v = 0.092 - r as f32 * 0.066;
        for (k, u) in [0.222f32, 0.292].iter().enumerate() {
            dial(m, f, *u, v, 0.026, *id, 11.0 + (r * 2 + k) as f32);
        }
        hp::word(m, &g, 0.354, v, 0.0028, name);
    }
    // The landing gear panel on the left: position lights, the flap lights, the handle's slot (the handle is a piece of its own).
    hp::word(m, &g, -0.318, 0.045, 0.0028, "LDG GEAR");
    for (i, s) in ["NOSE", "LEFT", "RIGHT"].iter().enumerate() {
        hp::light(m, f, -0.298, 0.022 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), f.at_h(GEAR_U, GEAR_V + 0.03, 0.0), Vec3::new(0.005, 0.034, 0.002), 0.002, f.rot());
    hp::word(m, &g, -0.318, -0.105, 0.0026, "FLAPS");
    for (i, s) in ["HALF", "FULL"].iter().enumerate() {
        hp::light(m, f, -0.318, -0.125 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    // Under the right instruments: the armament switches.
    hp::plate(m, f, 0.085, -0.196, 0.33, -0.148, 33.0);
    let gg = f.sub(0.0, 0.0, 0.0043);
    for i in 0..5 {
        hp::toggle(m, &gg, 0.115 + i as f32 * 0.03, -0.176, i % 2 == 0);
    }
    hp::guarded(m, &gg, 0.285, -0.172, false);
    hp::word(m, &gg, 0.305, -0.155, 0.0026, "GUN");
    // The pedestal between the knees: the weapon panel and the missile status lights.
    let p = f.sub(0.0, -0.305, -0.012);
    m.slab(&GREY, &hp::rrect(0.118, 0.105, 0.008), &[], p.o, p.ax, p.ay, 0.03);
    hp::plate(m, &p, -0.108, -0.095, 0.108, 0.02, 35.0);
    hp::populate(m, &p, -0.104, -0.091, 0.104, 0.016, 35.0, Some("STORES"));
    hp::plate(m, &p, -0.108, 0.024, 0.108, 0.097, 36.0);
    let pp = p.sub(0.0, 0.0, 0.0043);
    for i in 0..5 {
        hp::pbutton(m, &pp, -0.076 + i as f32 * 0.038, 0.052, 0.013, 0.011, style::BUTTON, 0);
    }
    hp::word(m, &pp, 0.0, 0.086, 0.0028, "MASTER ARM");
}

/// The glare shield over the panel, running forward under the windscreen.
fn hood(m: &mut Mesh) {
    let rear: Vec<Vec3> = hp::catmull(&[
        Vec3::new(-0.448, 0.47, -5.74), Vec3::new(-0.452, 0.60, -5.75), Vec3::new(-0.415, 0.725, -5.77), Vec3::new(-0.315, 0.842, -5.79), Vec3::new(-0.15, 0.868, -5.80),
        Vec3::new(0.0, 0.872, -5.80), Vec3::new(0.15, 0.868, -5.80), Vec3::new(0.315, 0.842, -5.79), Vec3::new(0.415, 0.725, -5.77), Vec3::new(0.452, 0.60, -5.75), Vec3::new(0.448, 0.47, -5.74),
    ], 5);
    let (nz, front_z) = (14, -6.50);
    let rows: Vec<Vec<SP>> = (0..=nz).map(|i| {
        let t = i as f32 / nz as f32;
        rear.iter().map(|p| {
            let z = p.z + (front_z - p.z) * t;
            // Forward, it drops to the skin under the windscreen and narrows with the glass.
            let e = t * t * (3.0 - 2.0 * t);
            let floor = skin_y(z, p.x * 0.9) + 0.005;
            let x = p.x * (1.0 - 0.22 * t);
            let lim = glass_y(z, x) - 0.02;
            let y0 = p.y.min(lim);
            let y = (y0 + (floor.max(0.40) - y0) * e * 0.9).min(lim).max(floor.min(y0));
            (Vec3::new(x, y, z), [p.x, z], 0.0)
        }).collect()
    }).collect();
    m.grid(&GLARE.pat(pat::PIT), &rows, false, true);
    let under: Vec<Vec<SP>> = (0..=3).map(|i| {
        let t = i as f32 / 3.0 * 0.10;
        rear.iter().map(|p| (Vec3::new(p.x * 0.985, p.y - 0.016, p.z - t), [p.x, t], 0.0)).collect()
    }).collect();
    m.grid(&GLARE, &under, false, false);
    let lip: Vec<Vec3> = rear.iter().map(|p| *p + Vec3::new(0.0, -0.007, 0.004)).collect();
    m.tube(&GLARE, &lip, &|_| 0.0095, 12, true);
}

/// The optical sight on the glare shield: its body and the arms that hold the glass.
const SIGHT: Vec3 = Vec3::new(0.0, 0.905, -5.90);

fn sight_up() -> Vec3 {
    Vec3::new(0.0, 1.0, 0.50).normalize()
}

fn sight(m: &mut Mesh) {
    m.rbox(&BLK, SIGHT + Vec3::new(0.0, -0.022, -0.035), Vec3::new(0.055, 0.030, 0.075), 0.010, Quat::from_rotation_x(0.05));
    m.lathe(&BEZEL, SIGHT + Vec3::new(0.0, 0.008, -0.045), Vec3::Y, &[[0.034, 0.0], [0.034, 0.006], [0.028, 0.008], [0.0, 0.008]], 20, false);
    m.disc(&mt(mat::GLASS, 0.03, 0.03, 0.04), SIGHT + Vec3::new(0.0, 0.0165, -0.045), Vec3::Y, Vec3::Z, 0.027, 20);
    let up = sight_up();
    for s in [-1.0f32, 1.0] {
        let foot = SIGHT + Vec3::new(s * 0.062, -0.01, 0.005);
        m.tube(&hp::COMBINER_FRAME, &[foot, foot + up * 0.125], &|_| 0.004, 8, true);
    }
    m.rbox(&hp::COMBINER_FRAME, SIGHT + Vec3::new(0.0, -0.006, 0.006), Vec3::new(0.064, 0.006, 0.008), 0.003, Quat::IDENTITY);
    hp::knob(m, &Face::new(SIGHT + Vec3::new(0.0, -0.022, 0.041), Vec3::X, Vec3::Y), -0.03, 0.0, 0.005, 41.0, false);
}

/// Its glass, leaning aft at the top (drawn in the glass pass).
fn sight_glass(m: &mut Mesh) {
    let up = sight_up();
    let pts = [(-0.058, 0.0), (0.058, 0.0), (0.058, 0.095), (0.040, 0.122), (-0.040, 0.122), (-0.058, 0.095)];
    let ring: Vec<Vec3> = pts.iter().map(|&(x, y)| SIGHT + Vec3::new(0.0, 0.0, 0.005) + Vec3::X * x + up * y).collect();
    let c = ring.iter().fold(Vec3::ZERO, |a, p| a + *p) / ring.len() as f32;
    m.fan(&mt(mat::GLASS, 0.1, 0.1, 0.1), c, &ring, Vec3::X.cross(up));
}

// ---------------------------------------------------------------- the rear panel

fn rear_face() -> Face {
    let t = 10f32.to_radians();
    Face::new(Vec3::new(0.0, 0.80, -4.30), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}

/// The weapon systems officer's panel: the big radar scope under its hood,
/// a few flight instruments either side of it.
fn rear_panel(m: &mut Mesh) {
    let f = rear_face();
    let outline = vec![Vec2::new(0.42, -0.19), Vec2::new(-0.42, -0.19), Vec2::new(-0.42, 0.02), Vec2::new(-0.34, 0.16), Vec2::new(0.34, 0.16), Vec2::new(0.42, 0.02)];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    let sc = f.sub(0.0, 0.0, 0.0);
    m.lathe(&BEZEL, sc.o, sc.n(), &[[0.092, 0.0], [0.092, 0.014], [0.082, 0.016], [0.082, 0.004]], 40, false);
    m.disc(&mt(mat::GLASS, 0.015, 0.035, 0.02), sc.at_h(0.0, 0.0, 0.004), sc.n(), sc.ay, 0.082, 40);
    m.lathe(&mt(mat::RUBBER, 0.02, 0.02, 0.022), sc.at_h(0.0, 0.0, 0.014), sc.n(), &[[0.090, 0.0], [0.096, 0.07], [0.091, 0.072], [0.084, 0.0]], 40, true);
    dial(m, &f, -0.20, 0.055, 0.036, gauge::ASI, 51.0);
    dial(m, &f, -0.20, -0.05, 0.036, gauge::AI, 52.0);
    dial(m, &f, 0.20, 0.055, 0.036, gauge::ALT, 53.0);
    dial(m, &f, 0.20, -0.045, 0.028, gauge::CLOCK, 54.0);
    dial(m, &f, -0.31, 0.0, 0.028, gauge::HI, 55.0);
    hp::plate(m, &f, -0.36, -0.182, -0.10, -0.108, 56.0);
    hp::populate(m, &f, -0.356, -0.178, -0.104, -0.112, 56.0, None);
    hp::plate(m, &f, 0.10, -0.182, 0.36, -0.108, 57.0);
    hp::populate(m, &f, 0.104, -0.178, 0.356, -0.112, 57.0, None);
    hp::plate(m, &f, 0.262, -0.09, 0.40, 0.01, 58.0);
    hp::populate(m, &f, 0.266, -0.086, 0.396, 0.006, 58.0, None);
}

// ---------------------------------------------------------------- side consoles

fn consoles(m: &mut Mesh, top: f32, floor: f32, z0: f32, z1: f32, seed: f32, left: &[(f32, f32, &str)], right: &[(f32, f32, &str)]) {
    let cx = hp::CON_X;
    for s in [-1.0f32, 1.0] {
        // Inner side down to the floor, and the top: a grey tray the panels sit in.
        hp::quad_to(m, &GREY, [Vec3::new(s * cx, floor - 0.01, z1), Vec3::new(s * cx, floor - 0.01, z0), Vec3::new(s * cx, top, z0), Vec3::new(s * cx, top, z1)], Vec3::new(0.0, 0.3, (z0 + z1) * 0.5));
        let xo = |z: f32| s * (wall_x(z, top) + 0.004);
        hp::quad_to(m, &GREY, [Vec3::new(s * cx, floor - 0.01, z0), Vec3::new(xo(z0), floor - 0.01, z0), Vec3::new(xo(z0), top, z0), Vec3::new(s * cx, top, z0)], Vec3::new(0.0, 0.3, z0 - 1.0));
        m.patch(&GREY, 2, 16, s > 0.0, |u, v| {
            let z = z0 + (z1 - z0) * v;
            let x = s * cx + (xo(z) - s * cx) * u;
            (Vec3::new(x, top, z), [x, z], 0.0)
        });
        m.tube(&GREY, &[Vec3::new(s * cx, top - 0.004, z0), Vec3::new(s * cx, top - 0.004, z1)], &|_| 0.006, 10, true);
    }
    // Looking down from the seat: u is x, v forward.
    let f = Face::new(Vec3::new(0.0, top, 0.0), Vec3::X, -Vec3::Z);
    let mut seed = seed;
    for (s, list) in [(-1.0f32, left), (1.0, right)] {
        for &(za, zb, title) in list {
            seed += 7.0;
            let xo = wall_x((za + zb) * 0.5, top).min(wall_x(za, top)).min(wall_x(zb, top)) - 0.004;
            let (ua, ub) = if s < 0.0 { (-xo, -cx - 0.003) } else { (cx + 0.003, xo) };
            let (va, vb) = (-zb, -za);
            if title.is_empty() {
                // The throttle quadrant, a panel either side of it.
                hp::plate(m, &f, ua, va, ua + 0.09, vb, seed);
                hp::populate(m, &f, ua + 0.004, va + 0.004, ua + 0.086, vb - 0.004, seed, Some("EXT LT"));
                hp::plate(m, &f, ub - 0.03, va, ub, vb, seed + 1.0);
                hp::quadrant(m, &f, ua + 0.09, ub - 0.03, va, vb);
                continue;
            }
            hp::plate(m, &f, ua, va, ub, vb, seed);
            if title == "-" {
                rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.7, 0.7, 0.66)), f.at_h((ua + ub) * 0.5, (va + vb) * 0.5, 0.0047), f.ax, f.ay, (ub - ua) * 0.32, (vb - va) * 0.3);
                continue;
            }
            hp::populate(m, &f, ua + 0.004, va + 0.004, ub - 0.004, vb - 0.004, seed, Some(title));
        }
    }
}

// ---------------------------------------------------------------- what moves

/// The Hornet's stick, throttles and pedals, where the front seat is; the
/// landing gear handle in its slot on the panel.
fn controls(b: &mut Builder, f: &Face) {
    let mut h = Builder::default();
    hp::controls(&mut h);
    let off = b.pieces.len();
    let mut at = Vec::new();
    for mut p in h.pieces {
        // (Its gear handle is on a panel that is not here.)
        if p.name == "gear handle" {
            at.push(None);
            continue;
        }
        p.mesh.transform(0, Mat4::from_translation(FRONT));
        p.pivot += FRONT;
        p.parent = p.parent.and_then(|i: usize| at[i]);
        at.push(Some(off + at.iter().flatten().count()));
        b.pieces.push(p);
    }
    // A wheel-shaped knob (red light inside) on an arm moving up and down its slot.
    let w = f.sub(GEAR_U, GEAR_V + 0.03, 0.0);
    let mut gh = Mesh::default();
    let arm_top = w.at_h(0.0, 0.0, 0.05);
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), arm_top], &|_| 0.0045, 10, false);
    let c = arm_top + w.n() * 0.004;
    gh.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.016, 0.0], [0.019, 0.003], [0.019, 0.021], [0.016, 0.024], [0.0, 0.024]], 22, false);
    gh.lathe(&RED_LENS, c - w.ax * 0.013, w.ax, &[[0.0, 0.0], [0.009, 0.0], [0.009, 0.026], [0.0, 0.026]], 14, false);
    b.add("gear handle", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, 0.028), w.ay), w.o, Vec3::X, None);
}

/// Looking down or behind from the front seat, the view is mostly of the cockpit.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let h = Vec2::new(d.x, d.z);
    let hl = h.length().max(1e-4);
    if d.z > 0.0 && d.x.abs() < 0.35 * hl && d.y < 0.2 {
        return true; // the headbox, and the rear cockpit behind it
    }
    // How far out the cockpit's edge is that way, and how far below the eye.
    let fwd = (-d.z / hl).max(0.0);
    let side = (d.x / hl).abs();
    let reach = 1.0 / ((fwd / 0.70).powi(2) + (side / 0.50).powi(2)).sqrt().max(1e-3);
    let drop = eye.y - (0.87 * fwd + 0.58 * (1.0 - fwd)).max(0.56);
    d.y / hl < -drop / reach
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing in the cockpits pokes out through the canopy or stands outside the sills.
    #[test]
    fn cockpits_fit_under_the_canopy() {
        let mut b = Builder::default();
        build(&mut b);
        let mut n = 0;
        for p in &b.pieces {
            for v in &p.mesh.v {
                let [x, y, z] = v.pos;
                let [_, w, zt] = canopy_at(z);
                let z0 = fuse_at(-z)[2] - 0.04;
                // (The sills' rails lie against the glass: 2 cm of grace across.)
                let w = w + 0.02;
                let out = if y > z0 { (x / w).powi(2) + ((y - z0) / (zt - z0)).powi(2) - 1.0 } else { x.abs() - w };
                assert!(out < 0.0 && z > -6.56 && z < -2.75, "{} is outside the canopy at {:?}", p.name, v.pos);
                n += 1;
            }
        }
        println!("{} cockpit pieces, {n} vertices, all under the canopy", b.pieces.len());
    }
}
