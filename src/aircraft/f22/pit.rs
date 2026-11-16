//! The F-22A's cockpit: a glass office under the frameless bubble. One
//! instrument panel under a low glare shield, with the large primary
//! multifunction display in the middle, a secondary display either side of
//! it and a third on the pedestal between the knees; the integrated control
//! panel (a keypad) under the wide HUD, an up-front display each side of it;
//! the standby flight group and the landing gear handle at the panel's
//! edges. A side-stick on the right console behind its armrest, one broad
//! throttle grip for both engines on the left, an ACES II seat with its
//! yellow ejection handles either side of the pan.
//!
//! The seat, the pedals and the widgets on the panels are the Hornet's
//! (`fa18::pit`), moved to where they sit here: the two cockpits are the same
//! size from the eye down, this one 0.90 m further forward. Its screens show
//! the Hornet's pages too (pit18.wgsl `display18`) and its combiner the same
//! symbology (`hud18`, plane.wgsl `F22_HUD`). Everything here is body frame
//! (+X right, +Y up, -Z forward).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, screen_quad, sid, style, Face};
use crate::aircraft::fa18::pit as hp;
use glam::{Mat4, Quat, Vec2, Vec3};
use hp::{screens, BEZEL, BLK, COMBINER_FRAME, FLOOR_M, GLARE, GREEN_LENS, GREY, GREY_DK, RED_LENS, SEAT_GREY};
use std::f32::consts::{FRAC_PI_2, PI};

const BTN: Mat = mt(mat::RUBBER, 0.045, 0.045, 0.047);
const PAD: Mat = mt(mat::FABRIC, 0.035, 0.036, 0.038);

// ---------------------------------------------------------------- where things are

/// From the Hornet's cockpit to this one.
const D: Vec3 = Vec3::new(0.0, 0.0, -0.90);
const FLOOR: f32 = hp::FLOOR;
/// Side consoles: height of their tops, inner edge, front and back.
const CON_Y: f32 = hp::CON_Y;
const CON_X: f32 = hp::CON_X;
const CON_Z0: f32 = -6.28;
const CON_Z1: f32 = -4.72;
/// The seat reference point: where the back and pan cushions meet.
const SRP: Vec3 = Vec3::new(hp::SRP.x, hp::SRP.y, hp::SRP.z + D.z);
/// The bulkhead behind the seat.
const REAR_Z: f32 = -4.64;
/// The side-stick's pivot on the right console, the throttle's under the left one.
const STICK: Vec3 = Vec3::new(0.345, CON_Y + 0.012, -5.74);
const THR_PIVOT: Vec3 = Vec3::new(-0.335, CON_Y - 0.17, -5.60);

/// The canopy (f22.py CANOPY): Blender station, half width at the sill, height of the top.
const CANOPY: [[f32; 3]; 10] = [
    [7.30, 0.20, 0.52], [7.00, 0.33, 0.74], [6.60, 0.42, 0.98], [6.20, 0.46, 1.14], [5.70, 0.48, 1.23],
    [5.20, 0.48, 1.25], [4.70, 0.46, 1.18], [4.30, 0.40, 1.04], [3.95, 0.30, 0.86], [3.70, 0.18, 0.72],
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
/// Height of the fuselage's upper skin at station z, x out (f22.py section_z:
/// flat across the top, then a facet down to the chine).
fn skin_y(z: f32, x: f32) -> f32 {
    let [_, hw, top, mid, _, wt, _, ku, _] = fuse_at(-z);
    let ax = x.abs().min(hw);
    if ax <= wt || hw - wt < 1e-6 {
        return top;
    }
    mid + (top - mid) * (1.0 - (1.0 - (hw - ax) / (hw - wt)).powf(ku))
}
/// Where the canopy stands on the skin (f22.py sill_z), and the top of the sill inside it.
fn sill_y(z: f32) -> f32 {
    skin_y(z, canopy_at(z)[1]) - 0.02
}
fn sill_top(z: f32) -> f32 {
    sill_y(z) + 0.018
}
/// The cockpit's side walls: straight down from the sill's inner face (f22.py: w - 0.03).
fn wall_x(z: f32) -> f32 {
    (canopy_at(z)[1] - 0.034).max(0.20)
}
/// Height of the canopy glass's inside at station z, x out (f22.py canopy_arc).
fn glass_y(z: f32, x: f32) -> f32 {
    let [_, w, zt] = canopy_at(z);
    let z0 = sill_y(z);
    let c = (x.abs() / w).min(1.0);
    z0 + (zt - z0) * (1.0 - c * c).sqrt() - 0.006
}
/// Points on the seat: `a` forward of the reference point along the pan, x across.
fn pan(x: f32, a: f32, h: f32) -> Vec3 {
    SRP + Vec3::new(x, a * 0.10 + h, -a)
}

// ---------------------------------------------------------------- the cockpit

pub fn build(b: &mut Builder) {
    let mut tub = Mesh::default();
    walls(&mut tub);
    rear(&mut tub);
    b.fixed("cockpit tub", G_ENGINE, tub);

    let mut p = Mesh::default();
    let f = main_face();
    main_panel(&mut p, &f);
    pedestal(&mut p);
    hood(&mut p);
    hud(&mut p);
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

    controls(b, &f);
}

// ---------------------------------------------------------------- walls, floor, rear deck

fn walls(m: &mut Mesh) {
    let (z0, z1) = (-6.92, REAR_Z + 0.02);
    for s in [-1.0f32, 1.0] {
        // The side wall: from the floor up to the canopy sill.
        m.patch(&GREY, 40, 12, s < 0.0, |u, v| {
            let z = z0 + (z1 - z0) * u;
            let top = sill_top(z) - 0.004;
            let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
            (Vec3::new(s * wall_x(z), y, z), [z, y], 0.0)
        });
        // The rounded inner lip of the sill on top of it, with the canopy's lock hooks.
        let lip: Vec<Vec3> = (0..=40).map(|i| {
            let z = z0 + 0.1 + (z1 - z0 - 0.1) * i as f32 / 40.0;
            Vec3::new(s * (wall_x(z) - 0.006), sill_top(z) - 0.007, z)
        }).collect();
        m.tube(&GREY, &lip, &|_| 0.010, 14, true);
        for k in 0..6 {
            let z = -6.35 + k as f32 * 0.30;
            let p = Vec3::new(s * (wall_x(z) - 0.018), sill_top(z) - 0.012, z);
            m.rbox(&GREY_DK, p, Vec3::new(0.008, 0.012, 0.016), 0.003, Quat::IDENTITY);
            m.bolt(&STEEL, p + Vec3::new(-s * 0.008, 0.0, 0.0), Vec3::X * -s, 0.0035);
        }
        // A row of screws along the wall's panel joint above the console.
        for k in 0..28 {
            let z = -6.20 + k as f32 * 0.052;
            m.screw(&SCREW_PAINT, Vec3::new(s * (wall_x(z) - 0.0005), CON_Y + 0.045, z), Vec3::X * -s, 0.0022, k as f32);
        }
    }
    // The floor, and the pedal well's front.
    m.patch(&FLOOR_M, 6, 16, true, |u, v| {
        let z = -6.90 + (REAR_Z + 6.90) * v;
        let x = (u * 2.0 - 1.0) * (wall_x(z) + 0.01);
        (Vec3::new(x, FLOOR, z), [x, z], 0.0)
    });
    let w = wall_x(-6.88) + 0.01;
    hp::quad_to(m, &GREY_DK, [Vec3::new(-w, FLOOR - 0.01, -6.88), Vec3::new(w, FLOOR - 0.01, -6.88), Vec3::new(w, 0.47, -6.88), Vec3::new(-w, 0.47, -6.88)], EYE);
}

/// Behind the seat: the bulkhead, and the deck under the back of the canopy.
fn rear(m: &mut Mesh) {
    let deck_y = |x: f32, z: f32| skin_y(z, x) - 0.035;
    m.patch(&GREY, 12, 8, true, |u, v| {
        let x = (u * 2.0 - 1.0) * (wall_x(REAR_Z) + 0.01);
        let top = deck_y(x, REAR_Z);
        let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
        (Vec3::new(x, y, REAR_Z), [x, y], 0.0)
    });
    m.patch(&GREY, 14, 8, true, |u, v| {
        let z = REAR_Z + (-4.16 - REAR_Z) * v;
        let x = (u * 2.0 - 1.0) * (wall_x(z) + 0.012);
        (Vec3::new(x, deck_y(x, z), z), [x, z], 0.0)
    });
    // The canopy actuator's housing and an avionics box on the deck.
    let at = |x: f32, z: f32, h: f32| Vec3::new(x, deck_y(x, z) + h, z);
    m.rbox(&GREY_DK, at(0.0, -4.40, 0.035), Vec3::new(0.08, 0.045, 0.16), 0.02, Quat::IDENTITY);
    m.tube(&CHROME, &[at(0.0, -4.55, 0.05), at(0.0, -4.28, 0.10)], &|_| 0.012, 12, true);
    for s in [-1.0f32, 1.0] {
        m.rbox(&BLK, at(s * 0.22, -4.45, 0.022), Vec3::new(0.07, 0.026, 0.10), 0.006, Quat::IDENTITY);
    }
    // (facing forward, toward the seat)
    let f = Face::new(Vec3::new(-0.20, deck_y(0.0, REAR_Z) - 0.12, REAR_Z - 0.0005), -Vec3::X, Vec3::Y);
    rect(m, &legend_mat(style::RED_PLACARD, sid("DANGER"), GREY.col), f.o, f.ax, f.ay, 0.035, 0.012);
}

// ---------------------------------------------------------------- instrument panel

/// The panel, leaning back 15 degrees.
fn main_face() -> Face {
    let t = 15f32.to_radians();
    Face::new(Vec3::new(0.0, 0.60, -6.19), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}
/// The pedestal between the knees: it hangs from the panel's foot, leaning back further.
fn pedestal_face() -> Face {
    let t = 40f32.to_radians();
    let ay = Vec3::new(0.0, t.cos(), -t.sin());
    Face::new(main_face().at(0.0, -0.17) - ay * 0.10, Vec3::X, ay)
}
/// A face at (u, v) on f, turned about its up axis to look toward the eye by `share`.
fn toward_eye(f: &Face, u: f32, v: f32, share: f32) -> Face {
    let o = f.at(u, v);
    let n = f.n().lerp((EYE - o).normalize(), share).normalize();
    let ax = f.ay.cross(n).normalize();
    Face::new(o, ax, n.cross(ax))
}

/// A multifunction display: a square screen `s` across (half) in a flat bezel
/// with five buttons along each side, its case `depth` deep behind them.
fn mfd(m: &mut Mesh, f: &Face, id: f32, s: f32, depth: f32) {
    let h = s + 0.016;
    m.slab(&BEZEL, &hp::rrect(h, h, 0.006), &[hp::ccw(hp::rrect(s + 0.002, s + 0.002, 0.003))], f.at_h(0.0, 0.0, 0.012), f.ax, f.ay, depth);
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(0.0, 0.0, 0.004), f.ax, f.ay, s, s);
    let g = f.sub(0.0, 0.0, 0.012);
    let e = h - 0.008;
    for k in 0..5 {
        let t = (k as f32 - 2.0) * s * 0.36;
        for (u, v, rot) in [(t, e, 0.0f32), (t, -e, 0.0), (-e, t, FRAC_PI_2), (e, t, FRAC_PI_2)] {
            m.rbox(&BTN, g.at_h(u, v, 0.0018), Vec3::new(0.0056, 0.0042, 0.0020), 0.0014, g.rot() * Quat::from_rotation_z(rot));
        }
    }
    for (du, dv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        m.screw(&SCREW_BLACK, g.at(du * (h - 0.005), dv * (h - 0.005)), g.n(), 0.0016, id + du + 2.0 * dv);
    }
}

/// Where the gear handle's slot is on the panel.
const GEAR_U: f32 = -0.372;
const GEAR_V: f32 = -0.040;

fn main_panel(m: &mut Mesh, f: &Face) {
    // Wide below, where the displays are; its shoulders cut back under the glare shield.
    let outline = vec![
        Vec2::new(0.405, -0.17), Vec2::new(0.405, 0.03), Vec2::new(0.30, 0.10), Vec2::new(0.19, 0.17),
        Vec2::new(-0.19, 0.17), Vec2::new(-0.30, 0.10), Vec2::new(-0.405, 0.03), Vec2::new(-0.405, -0.17),
    ];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    // The primary display, and a secondary one each side canted toward the pilot.
    mfd(m, &f.sub(0.0, -0.052, 0.0), screens::AMPCD, 0.095, 0.012);
    // (Standing proud of the panel, so that their inner edges clear it.)
    let proud = f.sub(0.0, 0.0, 0.020);
    mfd(m, &toward_eye(&proud, -0.215, -0.070, 0.25), screens::DDI_L, 0.072, 0.05);
    mfd(m, &toward_eye(&proud, 0.215, -0.070, 0.25), screens::DDI_R, 0.072, 0.05);
    // Under the HUD: the integrated control panel, an up-front display each
    // side of it (engines on the left, fuel and the clock on the right).
    icp(m, &f.sub(0.0, 0.115, 0.0));
    for (u, id) in [(-0.125, screens::IFEI_L), (0.125, screens::IFEI_R)] {
        let g = f.sub(u, 0.112, 0.0);
        m.rbox(&BEZEL, g.at_h(0.0, 0.0, 0.004), Vec3::new(0.044, 0.051, 0.004), 0.003, g.rot());
        m.rbox(&BLK, g.at_h(0.0, 0.0, 0.0084), Vec3::new(0.039, 0.046, 0.0012), 0.0015, g.rot());
        screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), g.at_h(0.0, 0.0, 0.0098), g.ax, g.ay, 0.036, 0.043);
    }
    // The standby flight group at the shoulders and down the right edge.
    instrument(m, f, -0.235, 0.058, 0.024, 1.0, 5.0);
    instrument(m, f, 0.235, 0.058, 0.024, 0.0, 6.0);
    instrument(m, f, 0.360, -0.055, 0.024, 2.0, 7.0);
    hp::light(m, f, 0.360, 0.008, 0.022, 0.010, sid("MASTER CAUTION"), &mt(mat::LAMP, 1.0, 0.55, 0.05).x(lamp::ANNUNC));
    hp::toggle(m, f, 0.345, -0.125, false);
    hp::guarded(m, f, 0.378, -0.125, true);
    hp::word(m, f, 0.360, -0.103, 0.0026, "MASTER ARM");
    hp::word(m, f, 0.360, -0.150, 0.0026, "EMERG JETT");
    // Down the left edge: the fire light, the landing gear's position lights
    // beside the handle's slot, the flap switch.
    hp::light(m, f, -0.360, 0.008, 0.022, 0.010, sid("FIRE"), &RED_LENS);
    hp::word(m, f, -0.360, -0.006, 0.0026, "LDG GEAR");
    for (i, s) in ["NOSE", "LEFT", "RIGHT"].iter().enumerate() {
        hp::light(m, f, -0.342, -0.022 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), f.at_h(GEAR_U, GEAR_V, 0.0), Vec3::new(0.005, 0.034, 0.002), 0.002, f.rot());
    hp::toggle(m, f, -0.360, -0.125, true);
    hp::word(m, f, -0.360, -0.103, 0.0026, "FLAPS");
    hp::word(m, f, -0.360, -0.150, 0.0024, "AUTO");
}

/// A standby instrument in its black case, the face under a raised rim.
fn instrument(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, id: f32, seed: f32) {
    m.rbox(&BLK, f.at_h(u, v, 0.005), Vec3::new(r + 0.008, r + 0.008, 0.005), 0.004, f.rot());
    let c = f.at_h(u, v, 0.0104);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, f.n(), f.ay, r, 36);
    m.lathe(&BLK, c, f.n(), &[[r, 0.0], [r, 0.004], [r + 0.003, 0.0055], [r + 0.006, 0.005]], 36, false);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (r + 0.0045), v + dv * (r + 0.0045), 0.010), f.n(), 0.0018, seed + k as f32);
    }
}

/// The integrated control panel: a scratchpad window over two rows of keys and a row of function buttons.
fn icp(m: &mut Mesh, f: &Face) {
    m.rbox(&BEZEL, f.at_h(0.0, 0.0, 0.005), Vec3::new(0.068, 0.048, 0.005), 0.004, f.rot());
    let g = f.sub(0.0, 0.0, 0.010);
    m.rbox(&BLK, g.at_h(0.0, 0.034, 0.0004), Vec3::new(0.037, 0.0105, 0.0012), 0.0012, g.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(screens::SCRATCH), g.at_h(0.0, 0.034, 0.0018), g.ax, g.ay, 0.034, 0.0085);
    hp::knob(m, &g, -0.054, 0.034, 0.0045, 3.0, false);
    hp::knob(m, &g, 0.054, 0.034, 0.0045, 4.0, false);
    let keys = [["1", "2", "3", "4", "5", "6"], ["7", "8", "9", "0", "CLR", "ENT"], ["A/P", "IFF", "TCN", "ILS", "D/L", "BCN"]];
    for (r, row) in keys.iter().enumerate() {
        for (c, k) in row.iter().enumerate() {
            hp::pbutton(m, &g, -0.050 + c as f32 * 0.020, 0.010 - r as f32 * 0.019, 0.0078, 0.0066, style::KEY, sid(k));
        }
    }
}

/// The third secondary display, on the pedestal between the knees.
fn pedestal(m: &mut Mesh) {
    let f = pedestal_face();
    m.slab(&BLK, &hp::rrect(0.095, 0.10, 0.008), &[], f.o, f.ax, f.ay, 0.06);
    mfd(m, &f.sub(0.0, 0.010, 0.0), screens::RWR, 0.068, 0.012);
    // Its foot down to the floor.
    let foot = f.at_h(0.0, -0.10, -0.03);
    m.rbox(&GREY_DK, Vec3::new(0.0, (foot.y + FLOOR) * 0.5, foot.z - 0.03), Vec3::new(0.085, (foot.y - FLOOR) * 0.5 + 0.01, 0.03), 0.008, Quat::IDENTITY);
}

/// The glare shield over the panel, running forward under the canopy to its nose.
fn hood(m: &mut Mesh) {
    // Its rear edge seen from the seat: a flat top over the HUD and the
    // up-front displays, the shoulders coming down outside the side displays.
    let rear: Vec<Vec3> = hp::catmull(&[
        Vec3::new(-0.425, 0.46, -6.10), Vec3::new(-0.430, 0.58, -6.105), Vec3::new(-0.415, 0.66, -6.115), Vec3::new(-0.33, 0.735, -6.14),
        Vec3::new(-0.21, 0.790, -6.165), Vec3::new(0.0, 0.798, -6.175), Vec3::new(0.21, 0.790, -6.165), Vec3::new(0.33, 0.735, -6.14),
        Vec3::new(0.415, 0.66, -6.115), Vec3::new(0.430, 0.58, -6.105), Vec3::new(0.425, 0.46, -6.10),
    ], 5);
    let nz = 14;
    let front_z = -7.06;
    let rows: Vec<Vec<SP>> = (0..=nz).map(|i| {
        let t = i as f32 / nz as f32;
        rear.iter().map(|p| {
            let z = p.z + (front_z - p.z) * t;
            // Forward, it narrows with the canopy and drops to the sill.
            let x = p.x * ((canopy_at(z)[1] - 0.035) / 0.43).min(1.0);
            let e = t * t * (3.0 - 2.0 * t);
            let lim = glass_y(z, x) - 0.02;
            let y0 = p.y.min(lim);
            let foot = (sill_y(z) + 0.025).min(y0);
            (Vec3::new(x, (y0 + (foot - y0) * e * 0.85).min(lim), z), [p.x, z], 0.0)
        }).collect()
    }).collect();
    m.grid(&GLARE.pat(pat::PIT), &rows, false, true);
    // Its underside back from the lip, and the rolled lip itself.
    let under: Vec<Vec<SP>> = (0..=3).map(|i| {
        let t = i as f32 / 3.0 * 0.09;
        rear.iter().map(|p| (Vec3::new(p.x * 0.985, p.y - 0.016, p.z - t), [p.x, t], 0.0)).collect()
    }).collect();
    m.grid(&GLARE, &under, false, false);
    let lip: Vec<Vec3> = rear.iter().map(|p| *p + Vec3::new(0.0, -0.007, 0.004)).collect();
    m.tube(&GLARE, &lip, &|_| 0.0095, 12, true);
}

/// The combiner's foot (f22.py has nothing here: plane.wgsl `F22_HUD` finds the glass by this box).
const HUD_BASE: Vec3 = Vec3::new(0.0, 0.872, -6.162);

/// The HUD on top of the glare shield: a low, wide body, the combiner's two posts and the bar at their foot.
fn hud(m: &mut Mesh) {
    m.rbox(&BLK, Vec3::new(0.0, 0.805, -6.275), Vec3::new(0.125, 0.052, 0.125), 0.012, Quat::from_rotation_x(0.05));
    // Its rear face, sloping, with the lock and shoot lights.
    let rf = Face::new(Vec3::new(0.0, 0.828, -6.142), Vec3::X, Vec3::new(0.0, 1.0, 0.25));
    m.rbox(&BEZEL, rf.o, Vec3::new(0.118, 0.026, 0.006), 0.004, rf.rot());
    let g = rf.sub(0.0, 0.0, 0.006);
    m.lathe(&GREEN_LENS, g.at(-0.012, 0.0), g.n(), &[[0.0045, 0.0], [0.0045, 0.002], [0.0, 0.003]], 12, true);
    m.lathe(&mt(mat::LAMP, 1.0, 0.85, 0.1).x(lamp::GEAR_UP), g.at(0.012, 0.0), g.n(), &[[0.0045, 0.0], [0.0045, 0.002], [0.0, 0.003]], 12, true);
    hp::word(m, &g, -0.040, 0.0, 0.0026, "LOCK");
    hp::word(m, &g, 0.042, 0.0, 0.0026, "SHOOT");
    hp::knob(m, &g, -0.085, 0.0, 0.004, 7.0, false);
    hp::knob(m, &g, 0.085, 0.0, 0.004, 8.0, false);
    let up = Vec3::new(0.0, 1.0, 0.30).normalize();
    let tilt = Quat::from_rotation_arc(Vec3::Y, up);
    for s in [-1.0f32, 1.0] {
        m.rbox(&COMBINER_FRAME, HUD_BASE + Vec3::X * (s * 0.118) + up * 0.062, Vec3::new(0.004, 0.068, 0.010), 0.002, tilt);
    }
    m.rbox(&COMBINER_FRAME, HUD_BASE + Vec3::new(0.0, -0.006, -0.012), Vec3::new(0.124, 0.007, 0.012), 0.004, Quat::IDENTITY);
}

/// The combiner glass, wide and leaning aft at the top (drawn in the glass
/// pass; pit18.wgsl draws the symbology on it).
fn combiner(m: &mut Mesh) {
    let pts = [(-0.112, 0.0), (0.112, 0.0), (0.114, 0.13), (0.082, 0.180), (-0.082, 0.180), (-0.114, 0.13)];
    let up = Vec3::new(0.0, 1.0, 0.30).normalize();
    let ring: Vec<Vec3> = pts.iter().map(|&(x, y)| HUD_BASE + Vec3::X * x + up * y).collect();
    let c = ring.iter().fold(Vec3::ZERO, |a, p| a + *p) / ring.len() as f32;
    m.fan(&mt(mat::GLASS, 0.1, 0.1, 0.1), c, &ring, Vec3::X.cross(up));
}

// ---------------------------------------------------------------- side consoles

/// The throttle's two slots in the Hornet's quadrant, here.
const THR_X: [f32; 2] = [hp::THR_X[0], hp::THR_X[1]];

fn consoles(m: &mut Mesh) {
    let xo = |z: f32| wall_x(z) + 0.004;
    for s in [-1.0f32, 1.0] {
        // Inner side down to the floor, the front facing the pedals, the back.
        hp::quad_to(m, &GREY, [Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z1), Vec3::new(s * CON_X, FLOOR - 0.01, CON_Z0), Vec3::new(s * CON_X, CON_Y, CON_Z0), Vec3::new(s * CON_X, CON_Y, CON_Z1)], Vec3::new(0.0, 0.3, -5.5));
        for (z, toward) in [(CON_Z0, -7.0), (CON_Z1, -4.0)] {
            hp::quad_to(m, &GREY, [Vec3::new(s * CON_X, FLOOR - 0.01, z), Vec3::new(s * xo(z), FLOOR - 0.01, z), Vec3::new(s * xo(z), CON_Y, z), Vec3::new(s * CON_X, CON_Y, z)], Vec3::new(0.0, 0.3, toward));
        }
        // The top: a grey tray the panels sit in, its rounded inner edge.
        m.patch(&GREY, 2, 16, s > 0.0, |u, v| {
            let z = CON_Z0 + (CON_Z1 - CON_Z0) * v;
            let x = s * (CON_X + (xo(z) - CON_X) * u);
            (Vec3::new(x, CON_Y, z), [x, z], 0.0)
        });
        m.tube(&GREY, &[Vec3::new(s * CON_X, CON_Y - 0.004, CON_Z0), Vec3::new(s * CON_X, CON_Y - 0.004, CON_Z1)], &|_| 0.006, 10, true);
    }
    // Panels, front to back; an empty title is the throttle's or the stick's place.
    let left: [(f32, f32, &str); 7] = [(-6.28, -6.12, "ELEC"), (-6.12, -5.94, "FUEL"), (-5.94, -5.82, "EXT LT"), (-5.82, -5.38, ""), (-5.38, -5.16, "APU"), (-5.16, -4.94, "FCS"), (-4.94, -4.72, "COMM")];
    let right: [(f32, f32, &str); 6] = [(-6.28, -6.08, "ECS"), (-6.08, -5.88, "INTR LT"), (-5.88, -5.36, ""), (-5.36, -5.14, "SNSR"), (-5.14, -4.92, "KY-58"), (-4.92, -4.72, "DEFOG")];
    // Looking down from the seat: u is x, v forward.
    let f = Face::new(Vec3::new(0.0, CON_Y, 0.0), Vec3::X, -Vec3::Z);
    let mut seed = 150.0;
    for (s, list) in [(-1.0f32, &left[..]), (1.0, &right[..])] {
        for &(z0, z1, title) in list {
            seed += 7.0;
            let out = wall_x(z0).min(wall_x(z1)) - 0.002;
            let (ua, ub) = if s < 0.0 { (-out, -CON_X - 0.003) } else { (CON_X + 0.003, out) };
            let (va, vb) = (-z1, -z0);
            if title.is_empty() && s < 0.0 {
                // The throttle quadrant, a narrow panel each side of it.
                hp::plate(m, &f, ua, va, THR_X[0] - 0.035, vb, seed);
                hp::plate(m, &f, THR_X[1] + 0.035, va, ub, vb, seed + 1.0);
                hp::quadrant(m, &f, THR_X[0] - 0.035, THR_X[1] + 0.035, va, vb);
                continue;
            }
            hp::plate(m, &f, ua, va, ub, vb, seed);
            if !title.is_empty() {
                hp::populate(m, &f, ua + 0.004, va + 0.004, ub - 0.004, vb - 0.004, seed, Some(title));
            }
        }
    }
    // The side-stick's housing and the armrest behind it.
    m.lathe(&GREY_DK, Vec3::new(STICK.x, CON_Y + 0.004, STICK.z), Vec3::Y, &[[0.058, 0.0], [0.058, 0.010], [0.046, 0.016], [0.0, 0.016]], 24, false);
    m.rbox(&GREY_DK, Vec3::new(STICK.x + 0.01, CON_Y + 0.020, STICK.z + 0.24), Vec3::new(0.060, 0.018, 0.125), 0.008, Quat::IDENTITY);
    m.rbox(&PAD, Vec3::new(STICK.x + 0.01, CON_Y + 0.046, STICK.z + 0.24), Vec3::new(0.055, 0.012, 0.118), 0.010, Quat::IDENTITY);
    // The canopy switch on the right wall, under the sill.
    let z = -5.62;
    let wf = Face::new(Vec3::new(wall_x(z) - 0.001, CON_Y + 0.085, z), Vec3::Z, Vec3::Y);
    hp::word(m, &wf, 0.0, 0.0, 0.004, "CANOPY");
    m.rbox(&BLK, wf.at_h(0.0, -0.024, 0.004), Vec3::new(0.02, 0.014, 0.004), 0.003, wf.rot());
    hp::toggle(m, &wf.sub(0.0, -0.024, 0.008), 0.0, 0.0, true);
}

// ---------------------------------------------------------------- the seat

/// An ACES II: the Hornet's bucket, cushions, headbox and harness where this
/// seat is, without the Martin-Baker's firing loop between the knees: its
/// ejection handles are either side of the pan.
fn seat(m: &mut Mesh) {
    hp::seat(m);
    // (The loop and its housing: everything in this box at the front of the pan.)
    let c = hp::SRP;
    let looped = |v: &crate::render::PartVertex| v.pos[0].abs() < 0.07 && v.pos[1] > c.y && v.pos[1] < c.y + 0.11 && v.pos[2] > c.z - 0.50 && v.pos[2] < c.z - 0.44;
    m.i = m.i.chunks(3).filter(|t| !t.iter().all(|&i| looped(&m.v[i as usize]))).flatten().copied().collect();
    m.transform(0, Mat4::from_translation(D));
    // (Its rails stand a little taller than this canopy has room for.)
    for v in m.v.iter_mut() {
        v.pos[1] = v.pos[1].min(glass_y(v.pos[2], v.pos[0]) - 0.012);
    }
    for s in [-1.0f32, 1.0] {
        let p = pan(s * 0.200, 0.38, 0.010);
        let grip: Vec<Vec3> = (0..=14).map(|i| {
            let a = PI * i as f32 / 14.0;
            p + Vec3::new(s * 0.010 * a.sin(), 0.058 * a.sin(), 0.042 * a.cos())
        }).collect();
        m.tube(&hp::stripes(0.014, true), &grip, &|_| 0.0075, 10, true);
        m.rbox(&SEAT_GREY, p + Vec3::new(0.0, -0.004, 0.0), Vec3::new(0.012, 0.010, 0.055), 0.004, Quat::from_rotation_x(0.10));
    }
}

// ---------------------------------------------------------------- what moves

fn controls(b: &mut Builder, f: &Face) {
    // The side-stick: its boot rocks fore and aft, the grip in it from side to side; small throws.
    let mut boot = Mesh::default();
    let prof: Vec<[f32; 2]> = (0..=8).map(|i| {
        let t = i as f32 / 8.0;
        [0.040 - 0.020 * t + 0.004 * (i % 2) as f32, t * 0.045]
    }).collect();
    boot.lathe(&mt(mat::RUBBER, 0.03, 0.03, 0.032), STICK, Vec3::Y, &prof, 18, true);
    let g = b.add("stick gimbal", G_ENGINE, boot, Anim::hinge(Drive::new(Input::Pitch, 0.09)), STICK, Vec3::X, None);
    let mut st = Mesh::default();
    stick(&mut st);
    b.add("stick", G_ENGINE, st, Anim::hinge(Drive::new(Input::Roll, -0.10)), STICK, Vec3::Z, Some(g));

    // The throttle: an arm up through each slot to the one grip.
    let mut t = Mesh::default();
    throttle(&mut t);
    b.add("throttle", G_ENGINE, t, Anim::hinge(Drive::new(Input::Throttle, -0.55).offset(0.30)), THR_PIVOT, Vec3::X, None);

    // The landing gear handle: a wheel-shaped knob (red light inside) on an arm moving up and down its slot.
    let w = f.sub(GEAR_U, GEAR_V, 0.0);
    let mut gh = Mesh::default();
    let arm_top = w.at_h(0.0, 0.0, 0.05);
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), arm_top], &|_| 0.0045, 10, false);
    let c = arm_top + w.n() * 0.004;
    gh.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.016, 0.0], [0.019, 0.003], [0.019, 0.021], [0.016, 0.024], [0.0, 0.024]], 22, false);
    gh.lathe(&RED_LENS, c - w.ax * 0.013, w.ax, &[[0.0, 0.0], [0.009, 0.0], [0.009, 0.026], [0.0, 0.026]], 14, false);
    b.add("gear handle", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, 0.028), w.ay), w.o, Vec3::X, None);

    // The Hornet's rudder pedals and the box they hang from.
    let mut h = Builder::default();
    hp::controls(&mut h);
    for mut p in h.pieces {
        if p.name == "rudder pedal" || p.name == "pedal box" {
            p.mesh.transform(0, Mat4::from_translation(D));
            p.pivot += D;
            p.parent = None;
            b.pieces.push(p);
        }
    }
}

/// The side-stick's grip: short, leaning in toward the hand, the trigger in
/// front and the weapon release, trim hat and paddle on its head.
fn stick(m: &mut Mesh) {
    let s0 = STICK + Vec3::Y * 0.035;
    m.tube(&mt(mat::BLACK, 0.025, 0.025, 0.027), &[STICK + Vec3::Y * 0.01, s0], &|_| 0.012, 12, false);
    let spine = hp::catmull(&[s0, s0 + Vec3::new(-0.004, 0.035, -0.002), s0 + Vec3::new(-0.010, 0.080, -0.010), s0 + Vec3::new(-0.014, 0.112, -0.026)], 6);
    let grip = mt(mat::PLASTIC, 0.022, 0.022, 0.024).pat(pat::KNURL);
    m.tube(&grip, &spine, &|t| 0.0155 + 0.0065 * (t * PI).sin() + 0.004 * smoothstep(0.7, 0.95, t), 16, true);
    let top = spine[spine.len() - 1];
    m.dome(&grip, top, Vec3::new(-0.1, 0.5, -1.0).normalize(), 0.018, 0.010, 14, 0.0);
    // Head: the weapon release (red), the trim hat, the paddle low in front.
    m.dome(&mt(mat::PLASTIC, 0.40, 0.025, 0.02), top + Vec3::new(-0.010, 0.014, 0.004), Vec3::new(-0.3, 1.0, 0.1).normalize(), 0.0050, 0.0036, 12, 0.0);
    m.lathe(&BLK, top + Vec3::new(0.006, 0.016, 0.000), Vec3::Y, &[[0.0042, 0.0], [0.0042, 0.003], [0.0024, 0.0044], [0.0, 0.0046]], 10, false);
    m.tube(&BLK, &hp::catmull(&[s0 + Vec3::new(-0.008, 0.085, -0.020), s0 + Vec3::new(-0.008, 0.070, -0.034), s0 + Vec3::new(-0.006, 0.048, -0.028)], 4), &|_| 0.004, 8, true);
    m.rbox(&BLK, s0 + Vec3::new(-0.002, 0.030, -0.020), Vec3::new(0.010, 0.009, 0.004), 0.002, Quat::from_rotation_x(0.3));
}

/// The throttle: both engines under one broad grip, its switches under the fingers and thumb.
fn throttle(m: &mut Mesh) {
    let steel = mt(mat::STEEL, 0.25, 0.25, 0.26);
    for x in THR_X {
        m.rbox(&steel, Vec3::new(x, THR_PIVOT.y + 0.17, THR_PIVOT.z), Vec3::new(0.005, 0.08, 0.012), 0.003, Quat::IDENTITY);
    }
    let grip = mt(mat::PLASTIC, 0.024, 0.024, 0.026);
    let c = THR_PIVOT + Vec3::new(0.0, 0.270, -0.008);
    m.rbox(&grip, c, Vec3::new(0.038, 0.040, 0.034), 0.016, Quat::from_rotation_x(-0.15));
    m.rbox(&grip, c + Vec3::new(0.0, 0.030, -0.024), Vec3::new(0.034, 0.016, 0.022), 0.012, Quat::from_rotation_x(-0.5));
    // Speedbrake (a slide switch) and a comm hat on the inboard side, the cursor pad on top.
    m.rbox(&BLK, c + Vec3::new(0.040, 0.014, -0.012), Vec3::new(0.003, 0.012, 0.004), 0.0015, Quat::IDENTITY);
    m.lathe(&BLK, c + Vec3::new(0.038, -0.010, 0.006), Vec3::X, &[[0.004, 0.0], [0.004, 0.003], [0.0, 0.004]], 10, false);
    m.lathe(&mt(mat::RUBBER, 0.05, 0.05, 0.05), c + Vec3::new(-0.008, 0.046, -0.030), Vec3::new(0.0, 1.0, -0.4).normalize(), &[[0.0065, 0.0], [0.0065, 0.003], [0.005, 0.005], [0.0, 0.0055]], 12, false);
}

/// Looking down or behind from the seat, the view is mostly of the cockpit.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let h = Vec2::new(d.x, d.z);
    let hl = h.length().max(1e-4);
    if d.z > 0.0 && d.x.abs() < 0.35 * hl && d.y < 0.2 {
        return true; // the headbox
    }
    // How far out the cockpit's edge is that way, and how far below the eye
    // (the glare shield ahead, the low sills to the sides).
    let fwd = (-d.z / hl).max(0.0);
    let side = (d.x / hl).abs();
    let reach = 1.0 / ((fwd / 0.66).powi(2) + (side / 0.45).powi(2)).sqrt().max(1e-3);
    let drop = eye.y - (0.80 * fwd + 0.55 * (1.0 - fwd)).max(0.53);
    d.y / hl < -drop / reach
}
