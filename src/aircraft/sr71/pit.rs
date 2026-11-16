//! The SR-71A's two cockpits, one behind the other and each under its own
//! small flat-paned canopy: narrow black offices of round dials. In front, the
//! pilot's panel under a deep glare shield: the attitude director in the
//! middle with the horizontal situation indicator below it, Mach and airspeed
//! to the left, the fuel and altitude and vertical speed to the right, the
//! engines' instruments in two columns on the far right (rpm, exhaust
//! temperature, nozzle position, fuel flow), the warning lights, the inlet
//! spikes' position and inlet temperature and the landing gear handle on the left, the drag
//! chute's striped handle in the corner; the periscope's housing hangs from
//! the windscreen's bow. Behind a leaning bulkhead, the reconnaissance systems
//! officer's panel: a big viewsight scope under its hood, the map projector's
//! screen, a few flight instruments and the sensors' panels; no stick.
//!
//! The seats, the stick, the throttles and the pedals are the Hornet's
//! (`fa18::pit`), moved to where they sit here, as in the Phantom; so are the
//! switches, knobs and plates the consoles are filled with. The dials are
//! faces of plane.wgsl's `gauge`, from the instrument readings (the spikes'
//! position and the compressor inlet temperature among them); the officer's
//! scope and map projector show the Hornet's radar and map pages
//! (pit18.wgsl). Everything here is body
//! frame (+X right, +Y up, -Z forward): stations are the model's, moved
//! forward to the centre of gravity (`st`, `row`).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, screen_quad, sid, style, Face};
use crate::aircraft::c172::cabin::gauge;
use crate::aircraft::fa18::pit as hp;
use glam::{Mat4, Quat, Vec2, Vec3};
use hp::{screens, BEZEL, BLK, GLARE, GREEN_LENS, RED_LENS};

// The tub is dark, not the Hornet's light grey: black panels in a black-grey structure.
const GREY: Mat = mt(mat::SATIN, 0.046, 0.047, 0.051).pat(pat::PIT);
const GREY_DK: Mat = mt(mat::SATIN, 0.026, 0.027, 0.030).pat(pat::PIT);
const HEADREST: Mat = mt(mat::FABRIC, 0.030, 0.031, 0.034);

// ---------------------------------------------------------------- where things are

/// From the Hornet's cockpit to the front one here (eye to eye, the seat set
/// low under the small canopy), and to the rear one.
const FRONT: Vec3 = Vec3::new(0.0, -0.13, -9.75);
const REAR: Vec3 = Vec3::new(0.0, -0.13, -8.40);
/// The floor (one level through both), and the tops of the side consoles (inner edge `hp::CON_X`).
const FLOOR: f32 = hp::FLOOR + FRONT.y;
const CON_Y: f32 = hp::CON_Y + FRONT.y;
/// Front of the pedal well, where the skin's opening starts, the back of the rear cockpit.
const Z0: f32 = -15.76;
const Z_OPEN: f32 = -15.30;
const Z1: f32 = -12.34;
/// The bulkhead between the cockpits leans with the front seat's back: where it is at height y.
fn bulk_z(y: f32) -> f32 {
    -13.95 + (y - FLOOR) * 0.245
}
const BULK_TOP: f32 = 0.90;

/// The canopies (sr71.py CANOPY): Blender station, half width at the sill, height of the top.
const CANOPY: [[f32; 3]; 10] = [
    [12.35, 0.20, 0.60], [12.00, 0.32, 0.86], [11.60, 0.40, 1.06], [11.20, 0.43, 1.14], [10.70, 0.44, 1.16],
    [10.20, 0.44, 1.16], [9.70, 0.44, 1.15], [9.20, 0.42, 1.10], [8.80, 0.34, 0.98], [8.50, 0.22, 0.86],
];
/// How thick the chine is where it leaves the body: above, below (sr71.py CHINE_T).
const CHINE_T: [f32; 2] = [0.15, 0.11];

/// The fuselage's section at body station z.
fn row(z: f32) -> [f32; 6] {
    fuse_at(-z - CG as f32)
}
fn canopy_at(z: f32) -> [f32; 3] {
    let y = -z - CG as f32;
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
/// How far above (or below) the chine's height the skin is, ax out (sr71.py `section_h`).
fn section_h(r: &[f32; 6], ax: f32, upper: bool) -> f32 {
    let [_, hw, top, mid, bot, rb] = *r;
    if hw < 1e-6 {
        return 0.0;
    }
    let ax = ax.abs().min(hw);
    let e = if upper { top - mid } else { mid - bot };
    let body = if rb > 1e-6 && ax < rb { e * (1.0 - (ax / rb).powi(2)).max(0.0).sqrt() } else { 0.0 };
    let chine = CHINE_T[if upper { 0 } else { 1 }].min(e) * (1.0 - ax / hw).powf(0.75);
    (body.powi(3) + chine.powi(3)).cbrt()
}
/// Height of the fuselage's upper skin at station z, x out.
fn skin_y(z: f32, x: f32) -> f32 {
    let r = row(z);
    r[3] + section_h(&r, x, true)
}
/// Half width of the fuselage's skin at station z, height y (above the chine).
fn skin_hw(z: f32, y: f32) -> f32 {
    let r = row(z);
    let want = (y - r[3]).max(0.0);
    if want >= section_h(&r, 0.0, true) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, r[1]);
    for _ in 0..24 {
        let m = (lo + hi) * 0.5;
        if section_h(&r, m, true) >= want { lo = m } else { hi = m }
    }
    lo
}
/// Is the skin cut away over station z (sr71.py TUB_Y)?
fn open(z: f32) -> bool {
    z > Z_OPEN && z < -12.3
}
/// The canopy's edge on the skin (sr71.py `sill_z`), and the inner face of the sill above it.
fn sill_y(z: f32) -> f32 {
    skin_y(z, canopy_at(z)[1]) - 0.02
}
fn sill_x(z: f32) -> f32 {
    canopy_at(z)[1] - 0.034
}
/// The cockpits' side walls: inside the sill, and inside the skin below it.
fn wall_x(z: f32, y: f32) -> f32 {
    let body = skin_hw(z, y.min(sill_y(z) - 0.04)) - 0.035;
    if open(z) { sill_x(z).min(body) } else { body.min(0.30) }
}
/// Height of the canopy's inside at station z, x out: flat panes, six round the arc (sr71.py `canopy_arc`).
fn glass_y(z: f32, x: f32) -> f32 {
    let [_, w, zt] = canopy_at(z);
    let z0 = sill_y(z);
    let c = (x.abs() / w).min(1.0);
    // the panes' corners: (cos, sin) of 0, 30, 60 and 90 degrees
    let k = [(1.0f32, 0.0f32), (0.8660254, 0.5), (0.5, 0.8660254), (0.0, 1.0)];
    for p in k.windows(2) {
        if c <= p[0].0 && c >= p[1].0 {
            let t = (p[0].0 - c) / (p[0].0 - p[1].0);
            return z0 + (zt - z0) * (p[0].1 + (p[1].1 - p[0].1) * t) - 0.006;
        }
    }
    z0
}
/// How far out the inside reaches at station z, height y: to the glass above the sill, to the wall below it.
fn reach(z: f32, y: f32) -> f32 {
    let z0 = sill_y(z);
    if y <= z0 {
        return wall_x(z, y);
    }
    let [_, w, _] = canopy_at(z);
    let (mut lo, mut hi) = (0.0, w);
    for _ in 0..20 {
        let m = (lo + hi) * 0.5;
        if glass_y(z, m) >= y { lo = m } else { hi = m }
    }
    lo - 0.015
}

/// A piece of the Hornet's cockpit, moved by `d` and tucked in under this
/// canopy (the seats' rails and headboxes stand a little taller than it).
fn moved(build: impl Fn(&mut Mesh), d: Vec3) -> Mesh {
    let mut m = Mesh::default();
    build(&mut m);
    m.transform(0, Mat4::from_translation(d));
    for v in &mut m.v {
        v.pos[1] = v.pos[1].min(glass_y(v.pos[2], v.pos[0]) - 0.03);
    }
    m
}

/// The seat: the Hornet's bucket, cushions and harness with the D-ring between
/// the knees, under the Blackbird seat's square headrest box.
fn seat(m: &mut Mesh) {
    hp::seat(m);
    let up = Vec3::new(0.0, 1.0, 0.24).normalize();
    let fwd = Vec3::new(0.0, up.z, -up.y);
    let tilt = Quat::from_rotation_arc(Vec3::Y, up);
    let at = |b: f32, h: f32| hp::SRP + up * b + fwd * h;
    m.rbox(&hp::SEAT_GREY, at(0.76, -0.085), Vec3::new(0.170, 0.155, 0.098), 0.006, tilt);
    m.rbox(&HEADREST, at(0.74, 0.030), Vec3::new(0.120, 0.095, 0.034), 0.006, tilt);
}

// ---------------------------------------------------------------- the cockpits

pub fn build(b: &mut Builder) {
    let mut tub = Mesh::default();
    walls(&mut tub);
    bulkheads(&mut tub);
    periscope(&mut tub);
    b.fixed("cockpit tub", G_ENGINE, tub);

    let mut p = Mesh::default();
    let f = panel_face();
    panel(&mut p, &f);
    hood(&mut p);
    rear_panel(&mut p);
    b.fixed("instrument panel", G_ENGINE, p);

    let mut c = Mesh::default();
    // Panels, front to back: (from, to, title); an untitled one is the throttle quadrant, "-" a blank plate with a map case.
    consoles(
        &mut c, -15.13, -13.93, 60.0,
        &[(-15.13, -14.95, "TEB"), (-14.95, -14.67, "FUEL"), (-14.67, -14.23, ""), (-14.23, -14.07, "OXY"), (-14.07, -13.93, "UHF")],
        &[(-15.13, -14.93, "LIGHTS"), (-14.93, -14.70, "INS"), (-14.70, -14.47, "TACAN"), (-14.47, -14.22, "DEF"), (-14.22, -13.93, "IFF")],
    );
    consoles(
        &mut c, -13.85, -12.50, 160.0,
        &[(-13.85, -13.55, "SENSOR"), (-13.55, -13.25, "ECM"), (-13.25, -12.95, "MAP PROJ"), (-12.95, -12.50, "-")],
        &[(-13.85, -13.52, "NAV"), (-13.52, -13.20, "INS"), (-13.20, -12.90, "RADAR"), (-12.90, -12.50, "-")],
    );
    b.fixed("consoles", G_ENGINE, c);

    b.fixed("ejection seat", G_ENGINE, moved(seat, FRONT));
    b.fixed("rear ejection seat", G_ENGINE, moved(seat, REAR));

    controls(b, &f);
}

// ---------------------------------------------------------------- walls, floor, bulkheads

fn walls(m: &mut Mesh) {
    for s in [-1.0f32, 1.0] {
        // The side wall: from the floor up to the canopy sill, following the
        // skin (and staying under it ahead of the opening).
        m.patch(&GREY, 72, 10, s < 0.0, |u, v| {
            let z = Z0 + (Z1 - Z0) * u;
            let top = sill_y(z) + 0.012;
            let y = FLOOR - 0.02 + (top - FLOOR + 0.02) * v;
            let x = wall_x(z, y);
            let y = if open(z) { y } else { y.min(skin_y(z, x) - 0.02) };
            (Vec3::new(s * x, y, z), [z, y], 0.0)
        });
        // The rounded inner lip of the sills, with the canopies' lock hooks.
        let lip: Vec<Vec3> = (0..=56).map(|i| {
            let z = Z_OPEN + 0.04 + (Z1 - Z_OPEN - 0.06) * i as f32 / 56.0;
            Vec3::new(s * (sill_x(z) - 0.004), sill_y(z) + 0.012, z)
        }).collect();
        m.tube(&GREY, &lip, &|_| 0.009, 10, true);
        for k in 0..8 {
            let z = -14.95 + k as f32 * 0.30 + if k > 3 { 0.25 } else { 0.0 };
            let p = Vec3::new(s * (sill_x(z) - 0.014), sill_y(z) + 0.006, z);
            m.rbox(&GREY_DK, p, Vec3::new(0.007, 0.011, 0.015), 0.003, Quat::IDENTITY);
            m.bolt(&STEEL, p + Vec3::new(-s * 0.007, 0.0, 0.0), Vec3::X * -s, 0.0032);
        }
        // A row of screws along each wall's panel joint, above the consoles.
        for (za, zb) in [(-15.05, -14.00), (-13.80, -12.55)] {
            for k in 0..20 {
                let z = za + (zb - za) * k as f32 / 19.0;
                let row = CON_Y + 0.05;
                m.screw(&SCREW_PAINT, Vec3::new(s * (wall_x(z, row) - 0.0005), row, z), Vec3::X * -s, 0.0022, k as f32 + za);
            }
        }
    }
    // The floor with its gritty walkway.
    m.patch(&hp::FLOOR_M, 6, 32, true, |u, v| {
        let z = Z0 + (Z1 - Z0) * v;
        let x = (u * 2.0 - 1.0) * (wall_x(z, FLOOR) + 0.01);
        (Vec3::new(x, FLOOR, z), [x, z], 0.0)
    });
}

/// The pedal well's front, the leaning bulkhead between the cockpits, and
/// the back of the rear one.
fn bulkheads(m: &mut Mesh) {
    let hw = wall_x(Z0, FLOOR) + 0.01;
    m.patch(&GREY_DK, 10, 5, false, |u, v| {
        let x = (u * 2.0 - 1.0) * hw;
        let y = FLOOR - 0.02 + (skin_y(Z0, x) - 0.02 - FLOOR + 0.02) * v;
        (Vec3::new(x, y, Z0 + 0.005), [x, y], 0.0)
    });
    // Between the seats: its front faces the pilot's seat, its back the officer's feet.
    for (dz, aft) in [(-0.012f32, false), (0.012, true)] {
        m.patch(&GREY, 12, 10, aft, |u, v| {
            let y = FLOOR - 0.02 + (BULK_TOP - FLOOR + 0.02) * v;
            let z = bulk_z(y) + dz;
            let x = (u * 2.0 - 1.0) * (reach(z, y) + 0.008);
            (Vec3::new(x, y, z), [x, y], 0.0)
        });
    }
    let zt = bulk_z(BULK_TOP);
    let w = reach(zt, BULK_TOP);
    m.tube(&GREY_DK, &[Vec3::new(-w, BULK_TOP, zt), Vec3::new(w, BULK_TOP, zt)], &|_| 0.014, 10, true);
    // Behind the rear seat: the bulkhead under the back of its canopy.
    m.patch(&GREY, 12, 8, true, |u, v| {
        let y = FLOOR - 0.02 + (1.02 - FLOOR + 0.02) * v;
        let x = (u * 2.0 - 1.0) * (reach(Z1, y) + 0.008);
        (Vec3::new(x, y.min(glass_y(Z1, x) - 0.02), Z1 - 0.005), [x, y], 0.0)
    });
    let pl = Face::new(Vec3::new(-0.18, 0.60, Z1 - 0.0055), -Vec3::X, Vec3::Y);
    rect(m, &legend_mat(style::RED_PLACARD, sid("DANGER"), GREY.col), pl.o, pl.ax, pl.ay, 0.035, 0.012);
}

/// The periscope's housing, hanging from the windscreen's bow: its eyepiece
/// looks down and back at the pilot.
fn periscope(m: &mut Mesh) {
    let z = -14.84;
    let top = glass_y(z, 0.0) - 0.022;
    m.rbox(&BLK, Vec3::new(0.0, top - 0.020, z), Vec3::new(0.030, 0.020, 0.045), 0.008, Quat::IDENTITY);
    let n = Vec3::new(0.0, -0.45, 1.0).normalize();
    let c = Vec3::new(0.0, top - 0.026, z + 0.046);
    m.lathe(&BEZEL, c, n, &[[0.024, 0.0], [0.024, 0.008], [0.019, 0.010], [0.019, 0.003]], 20, false);
    m.disc(&mt(mat::GLASS, 0.03, 0.035, 0.05), c + n * 0.003, n, Vec3::Y, 0.019, 20);
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
    Face::new(Vec3::new(0.0, 0.555, -15.0), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}

/// Where the landing gear handle's slot is on it.
const GEAR_U: f32 = -0.292;
const GEAR_V: f32 = -0.105;

fn panel(m: &mut Mesh, f: &Face) {
    // The panel: its upper corners cut to clear the windscreen's panes.
    let outline = vec![Vec2::new(0.33, -0.17), Vec2::new(-0.33, -0.17), Vec2::new(-0.33, 0.09), Vec2::new(-0.295, 0.15), Vec2::new(0.295, 0.15), Vec2::new(0.33, 0.09)];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    // The flight instruments: attitude over heading in the middle, Mach and
    // speed to the left, fuel, height and climb to the right.
    dial(m, f, 0.0, 0.070, 0.048, gauge::AI, 1.0);
    dial(m, f, 0.0, -0.082, 0.044, gauge::HI, 2.0);
    dial(m, f, -0.112, 0.100, 0.030, gauge::MACH, 3.0);
    dial(m, f, -0.112, 0.014, 0.036, gauge::ASI, 4.0);
    dial(m, f, -0.112, -0.078, 0.028, gauge::CLOCK, 5.0);
    dial(m, f, 0.112, 0.100, 0.030, gauge::JET_FUEL, 6.0);
    dial(m, f, 0.112, 0.014, 0.036, gauge::ALT, 7.0);
    dial(m, f, 0.112, -0.078, 0.030, gauge::VSI, 8.0);
    let g = f.sub(0.0, 0.0, 0.0004);
    // The engines' instruments, a column for each: rpm, exhaust temperature, nozzle position, fuel flow.
    for (r, (id, name)) in [(gauge::JET_RPM, "RPM"), (gauge::JET_EGT, "TEMP"), (gauge::NOZZLE, "NOZ"), (gauge::JET_FF, "FF")].iter().enumerate() {
        let v = 0.100 - r as f32 * 0.062;
        for (k, u) in [0.196f32, 0.258].iter().enumerate() {
            dial(m, f, *u, v, 0.022, *id, 11.0 + (r * 2 + k) as f32);
        }
        hp::word(m, &g, 0.308, v, 0.0024, name);
    }
    // Warning lights: a fire light for each engine, the master caution, the annunciators under them.
    let amber = mt(mat::LAMP, 1.0, 0.55, 0.05).x(lamp::ANNUNC);
    for u in [-0.212f32, -0.170] {
        hp::light(m, f, u, 0.122, 0.016, 0.009, sid("FIRE"), &RED_LENS);
    }
    hp::light(m, f, -0.191, 0.096, 0.034, 0.008, sid("MASTER CAUTION"), &amber);
    let notes = ["SPIKE", "FWD BYPASS", "AFT BYPASS", "L UNSTART", "R UNSTART", "CIT", "FUEL LOW", "CG", "SAS", "PITOT HEAT", "CANOPY", "OIL PRESS"];
    for (i, s) in notes.iter().enumerate() {
        hp::light(m, f, -0.236 + (i % 3) as f32 * 0.030, 0.072 - (i / 3) as f32 * 0.018, 0.0115, 0.0065, sid(s), &amber);
    }
    // The inlet spikes' position, a dial for each (they start aft at Mach 1.6), and
    // the compressor inlet temperature under them: what its speed is flown by.
    hp::word(m, &g, -0.205, 0.000, 0.0024, "SPIKE");
    for (k, (u, name)) in [(-0.229f32, "L"), (-0.181, "R")].iter().enumerate() {
        dial(m, f, *u, -0.030, 0.015, gauge::SPIKE, 21.0 + k as f32);
        hp::word(m, &g, *u, -0.058, 0.0024, name);
    }
    dial(m, f, -0.205, -0.094, 0.018, gauge::CIT, 23.0);
    hp::word(m, &g, -0.205, -0.126, 0.0024, "CIT");
    // The landing gear panel at the far left: position lights over the handle's slot (the handle is a piece of its own).
    hp::word(m, &g, -0.285, 0.062, 0.0024, "LDG GEAR");
    for (i, s) in ["NOSE", "LEFT", "RIGHT"].iter().enumerate() {
        hp::light(m, f, -0.285, 0.042 - i as f32 * 0.018, 0.012, 0.0065, sid(s), &GREEN_LENS);
    }
    m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), f.at_h(GEAR_U, GEAR_V + 0.03, 0.0), Vec3::new(0.005, 0.034, 0.002), 0.002, f.rot());
    // The drag chute's handle in the upper left corner: a striped T on its stalk.
    let c = f.at_h(-0.272, 0.108, 0.0);
    m.tube(&STEEL, &[c, c + f.n() * 0.028], &|_| 0.0035, 8, false);
    m.tube(&hp::stripes(0.010, true), &[c + f.n() * 0.030 - f.ax * 0.024, c + f.n() * 0.030 + f.ax * 0.024], &|_| 0.0062, 10, true);
    // Under the right instruments: a row of switches.
    hp::plate(m, f, 0.065, -0.166, 0.325, -0.124, 33.0);
    let gg = f.sub(0.0, 0.0, 0.0043);
    for i in 0..6 {
        hp::toggle(m, &gg, 0.095 + i as f32 * 0.032, -0.147, i % 2 == 0);
    }
    hp::guarded(m, &gg, 0.295, -0.144, true);
    // The pedestal between the knees: the autopilot and stability augmentation panel.
    let p = f.sub(0.0, -0.262, -0.012);
    m.slab(&GREY, &hp::rrect(0.100, 0.086, 0.008), &[], p.o, p.ax, p.ay, 0.03);
    hp::plate(m, &p, -0.092, -0.078, 0.092, 0.012, 35.0);
    hp::populate(m, &p, -0.088, -0.074, 0.088, 0.008, 35.0, Some("AUTOPILOT"));
    hp::plate(m, &p, -0.092, 0.016, 0.092, 0.078, 36.0);
    let pp = p.sub(0.0, 0.0, 0.0043);
    for i in 0..4 {
        hp::toggle(m, &pp, -0.057 + i as f32 * 0.038, 0.040, i != 3);
    }
    hp::word(m, &pp, 0.0, 0.068, 0.0026, "SAS");
}

/// The glare shield over the panel, running forward and down to the skin
/// under the windscreen.
fn hood(m: &mut Mesh) {
    let half = [(0.345f32, 0.40f32, -14.93f32), (0.345, 0.52, -14.945), (0.335, 0.64, -14.96), (0.29, 0.725, -14.97), (0.14, 0.748, -14.975)];
    let mut ctl: Vec<Vec3> = half.iter().map(|&(x, y, z)| Vec3::new(-x.min(reach(z, y) - 0.008), y, z)).collect();
    ctl.push(Vec3::new(0.0, 0.752, -14.975));
    ctl.extend(half.iter().rev().map(|&(x, y, z)| Vec3::new(x.min(reach(z, y) - 0.008), y, z)));
    let rear = hp::catmull(&ctl, 5);
    let (nz, front_z) = (12, Z_OPEN - 0.02);
    let rows: Vec<Vec<SP>> = (0..=nz).map(|i| {
        let t = i as f32 / nz as f32;
        rear.iter().map(|p| {
            let z = p.z + (front_z - p.z) * t;
            // Forward, it narrows with the glass and drops to the skin ahead of the opening.
            let e = t * t * (3.0 - 2.0 * t);
            let x = p.x * (1.0 - t * (1.0 - canopy_at(z)[1] / canopy_at(p.z)[1]));
            let y = (p.y + (skin_y(front_z, x) - 0.002 - p.y) * e).min(glass_y(z, x) - 0.02);
            let lim = (reach(z, y) - 0.004).max(0.02);
            (Vec3::new(x.clamp(-lim, lim), y, z), [p.x, z], 0.0)
        }).collect()
    }).collect();
    m.grid(&GLARE.pat(pat::PIT), &rows, false, true);
    let under: Vec<Vec<SP>> = (0..=3).map(|i| {
        let t = i as f32 / 3.0 * 0.08;
        rear.iter().map(|p| (Vec3::new(p.x * 0.985, p.y - 0.016, p.z - t), [p.x, t], 0.0)).collect()
    }).collect();
    m.grid(&GLARE, &under, false, false);
    let lip: Vec<Vec3> = rear.iter().map(|p| *p + Vec3::new(0.0, -0.007, 0.004)).collect();
    m.tube(&GLARE, &lip, &|_| 0.0085, 10, true);
}

// ---------------------------------------------------------------- the rear panel

fn rear_face() -> Face {
    let t = 10f32.to_radians();
    Face::new(Vec3::new(0.0, 0.70, -13.60), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}

/// The reconnaissance systems officer's panel, standing off the bulkhead:
/// the viewsight's scope under its hood, the map projector's screen, a few
/// flight instruments, the sensors' switches.
fn rear_panel(m: &mut Mesh) {
    let f = rear_face();
    let outline = vec![Vec2::new(0.34, -0.17), Vec2::new(-0.34, -0.17), Vec2::new(-0.34, 0.06), Vec2::new(-0.30, 0.14), Vec2::new(0.30, 0.14), Vec2::new(0.34, 0.06)];
    m.slab(&BLK, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    // What holds it off the bulkhead, and the shelf over it.
    m.rbox(&GREY_DK, f.at_h(0.0, -0.02, -0.07), Vec3::new(0.20, 0.12, 0.04), 0.01, f.rot());
    m.rbox(&GLARE, f.at_h(0.0, 0.148, 0.012), Vec3::new(0.295, 0.006, 0.045), 0.004, f.rot());
    let sc = f.sub(0.0, 0.015, 0.0);
    m.lathe(&BEZEL, sc.o, sc.n(), &[[0.092, 0.0], [0.092, 0.014], [0.082, 0.016], [0.082, 0.004]], 40, false);
    m.disc(&mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(screens::DDI_R), sc.at_h(0.0, 0.0, 0.004), sc.n(), sc.ay, 0.082, 40);
    m.lathe(&mt(mat::RUBBER, 0.02, 0.02, 0.022), sc.at_h(0.0, 0.0, 0.014), sc.n(), &[[0.090, 0.0], [0.095, 0.055], [0.090, 0.057], [0.084, 0.0]], 40, true);
    // The map projector: the moving map on a screen in a deep bezel.
    m.rbox(&BEZEL, f.at_h(-0.215, 0.058, 0.006), Vec3::new(0.070, 0.058, 0.006), 0.006, f.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(screens::AMPCD), f.at_h(-0.215, 0.058, 0.0124), f.ax, f.ay, 0.058, 0.048);
    for (k, u) in [-0.262f32, -0.168].iter().enumerate() {
        hp::knob(m, &f.sub(0.0, 0.0, 0.0), *u, -0.008, 0.0042, 70.0 + k as f32, false);
    }
    dial(m, &f, -0.258, -0.068, 0.028, gauge::MACH, 51.0);
    dial(m, &f, -0.172, -0.068, 0.028, gauge::ASI, 52.0);
    dial(m, &f, 0.165, 0.072, 0.032, gauge::AI, 53.0);
    dial(m, &f, 0.165, -0.020, 0.032, gauge::ALT, 54.0);
    dial(m, &f, 0.262, 0.066, 0.026, gauge::HI, 55.0);
    dial(m, &f, 0.262, -0.020, 0.026, gauge::CLOCK, 56.0);
    hp::plate(m, &f, -0.33, -0.166, -0.10, -0.108, 57.0);
    hp::populate(m, &f, -0.326, -0.162, -0.104, -0.112, 57.0, None);
    hp::plate(m, &f, 0.10, -0.166, 0.33, -0.064, 58.0);
    hp::populate(m, &f, 0.104, -0.162, 0.326, -0.068, 58.0, Some("VIEWSIGHT"));
}

// ---------------------------------------------------------------- side consoles

fn consoles(m: &mut Mesh, z0: f32, z1: f32, seed: f32, left: &[(f32, f32, &str)], right: &[(f32, f32, &str)]) {
    let (cx, top, floor) = (hp::CON_X, CON_Y, FLOOR);
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
                // The throttle quadrant fills the narrow console.
                hp::quadrant(m, &f, ua, ub, va, vb);
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
    let arm_top = w.at_h(0.0, 0.0, 0.045);
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), arm_top], &|_| 0.0045, 10, false);
    let c = arm_top + w.n() * 0.004;
    gh.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), c - w.ax * 0.011, w.ax, &[[0.0, 0.0], [0.015, 0.0], [0.018, 0.003], [0.018, 0.019], [0.015, 0.022], [0.0, 0.022]], 22, false);
    gh.lathe(&RED_LENS, c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.0085, 0.0], [0.0085, 0.024], [0.0, 0.024]], 14, false);
    b.add("gear handle", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, 0.028), w.ay), w.o, Vec3::X, None);
}

/// Looking down or behind from the front seat, the view is mostly of the cockpit.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let h = Vec2::new(d.x, d.z);
    let hl = h.length().max(1e-4);
    if d.z > 0.0 && d.x.abs() < 0.40 * hl && d.y < 0.25 {
        return true; // the headbox, and the bulkhead behind it
    }
    // How far out the cockpit's edge is that way, and how far below the eye.
    let fwd = (-d.z / hl).max(0.0);
    let side = (d.x / hl).abs();
    let reach = 1.0 / ((fwd / 0.68).powi(2) + (side / 0.40).powi(2)).sqrt().max(1e-3);
    let drop = eye.y - (0.75 * fwd + 0.47 * (1.0 - fwd)).max(0.45);
    d.y / hl < -drop / reach
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing in the cockpits pokes out through the canopies, stands outside
    /// the sills or comes up through the skin ahead of the opening.
    #[test]
    fn cockpits_fit_under_the_canopies() {
        let mut b = Builder::default();
        build(&mut b);
        let (mut n, mut bad) = (0, Vec::new());
        for p in &b.pieces {
            for v in &p.mesh.v {
                let [x, y, z] = v.pos;
                let ok = if open(z) {
                    // (The sills' lips lie against the frame: 2 cm of grace across.)
                    let z0 = sill_y(z);
                    if y > z0 { y < glass_y(z, x.abs() - 0.02) + 0.006 } else { x.abs() < canopy_at(z)[1] + 0.02 }
                } else {
                    y < skin_y(z, x) + 0.002 && x.abs() < row(z)[1]
                };
                if !(ok && y > FLOOR - 0.08 && z > Z0 - 0.01 && z < Z1 + 0.01) {
                    bad.push(format!("{} at {:?}", p.name, v.pos));
                }
                n += 1;
            }
        }
        assert!(bad.is_empty(), "{} vertices outside the cockpits, the first: {:?}", bad.len(), &bad[..bad.len().min(12)]);
        println!("{} cockpit pieces, {n} vertices, all under the canopies", b.pieces.len());
    }
}
