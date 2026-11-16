//! Aérospatiale/BAC Concorde: the four-engine supersonic airliner.
//!
//! Like the Hornet, the airframe is modelled in Blender
//! (`assets/blender/concorde.py`, which also saves `concorde.blend`) and
//! exported to `assets/concorde.bin.gz`: one mesh per piece, with the hinge
//! each one turns about. How the pieces move, what they belong to and what
//! they are to the damage model is set here, by name. Rebuild the asset with
//!
//!   blender -b --factory-startup --python assets/blender/concorde.py -- assets
//!
//! The flight deck inside is built here in Rust (`pit.rs`), like the 747's.
//!
//! Its paint is a picture (`assets/concorde_paint.jpg`), projected onto the
//! skin by plane.wgsl as the Hornet's is; so are its reheat plumes (the
//! "afterburner" piece, a set behind each of the four engines) and its
//! engines' sound (audio.rs, the turbojets').
//!
//! It has no flaps and no tailplane: six elevons along the wing's trailing
//! edge fly it in pitch and roll. The flap lever works its nose instead: up
//! with the visor raised for supersonic flight, the visor down, the nose
//! drooped 5 degrees to taxi and take off, and 12.5 degrees to land, when it
//! flies so nose-high that nothing could be seen over it otherwise.
//!
//! The figures are Concorde's: 61.66 m long, 25.6 m across, 12.2 m tall, a
//! 358.25 m² wing, 78,700 kg empty and 185,070 kg at most, 95,680 kg of fuel,
//! four Olympus 593 Mk 610s of 140 kN each (169.3 kN with reheat), Mach 2.04
//! (1,165 kt in the cruise, without reheat) up to 60,000 ft. (Not modelled:
//! the fuel pumped aft and forward to trim it through Mach 1.)
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, origin at the centre
//! of gravity.
use super::*;
use crate::airframe::*;
use std::collections::HashMap;

mod pit;

const KT: f64 = 1.0 / 1.94384;

// ---------------------------------------------------------------- geometry (in step with concorde.py)

/// Fuselage stations: y forward (Blender frame), half width, top, height of
/// the widest point, bottom (an ellipse above and below the widest point).
const FUSE: [[f32; 5]; 29] = [
    [33.00, 0.00, -0.80, -0.80, -0.80],
    [32.60, 0.06, -0.74, -0.80, -0.87],
    [32.00, 0.14, -0.65, -0.80, -0.95],
    [31.00, 0.27, -0.50, -0.79, -1.06],
    [30.00, 0.39, -0.35, -0.77, -1.15],
    [29.00, 0.51, -0.19, -0.74, -1.23],
    [28.00, 0.62, -0.03, -0.70, -1.30],
    [27.00, 0.73, 0.14, -0.65, -1.37],
    [26.00, 0.84, 0.32, -0.59, -1.43],
    [25.00, 0.94, 0.52, -0.52, -1.48],
    [24.00, 1.04, 0.76, -0.44, -1.53],
    [23.00, 1.13, 1.02, -0.35, -1.57],
    [22.00, 1.22, 1.27, -0.25, -1.60],
    [21.00, 1.30, 1.46, -0.15, -1.63],
    [20.00, 1.36, 1.58, -0.07, -1.65],
    [18.00, 1.44, 1.66, 0.00, -1.66],
    [16.00, 1.44, 1.66, 0.00, -1.66],
    [-9.00, 1.44, 1.66, 0.00, -1.66],
    [-11.00, 1.44, 1.66, 0.00, -1.66],
    [-13.00, 1.42, 1.66, 0.04, -1.60],
    [-15.00, 1.36, 1.65, 0.14, -1.46],
    [-17.00, 1.26, 1.63, 0.28, -1.24],
    [-19.00, 1.12, 1.60, 0.44, -0.96],
    [-21.00, 0.95, 1.55, 0.60, -0.64],
    [-23.00, 0.76, 1.48, 0.76, -0.28],
    [-25.00, 0.55, 1.39, 0.90, 0.12],
    [-27.00, 0.30, 1.26, 1.00, 0.56],
    [-28.00, 0.14, 1.16, 1.03, 0.82],
    [-28.66, 0.00, 1.05, 1.05, 1.05],
];

fn fuse_at(y: f32) -> [f32; 5] {
    if y >= FUSE[0][0] {
        return FUSE[0];
    }
    for w in FUSE.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a[0] >= y && y >= b[0] {
            let t = (a[0] - y) / (a[0] - b[0]);
            let mut r = [0.0; 5];
            for k in 0..5 {
                r[k] = a[k] + (b[k] - a[k]) * t;
            }
            return r;
        }
    }
    FUSE[FUSE.len() - 1]
}

/// Fuselage at body station z: top, the height of its widest point, bottom.
fn fuse(z: f32) -> (f32, f32, f32) {
    let r = fuse_at(-z);
    (r[2], r[3], r[4])
}

// The wing, an ogee: the leading edge's station (forward of the datum) at each
// span station, a smooth curve through these; the trailing edge a straight
// line swept a little forward; the chord line's height, dropping outboard of
// the fuselage's side.
const W_LE: [[f32; 2]; 11] = [[0.9, 19.8], [1.44, 15.0], [2.0, 10.0], [3.0, 3.5], [4.0, -0.5], [5.5, -4.3], [7.0, -7.2], [9.0, -10.5], [11.0, -13.6], [12.2, -15.6], [12.8, -16.8]];
const W_X1: f32 = 12.8;
const W_TE0: f32 = -19.0;
const W_TE_K: f32 = 0.11;
const W_Z0: f32 = -1.05;
const W_ANH: f32 = 0.03;
const W_ANH_X: f32 = 1.44;

/// The leading edge's station at span station x: a cubic through `W_LE` with
/// finite-difference slopes (concorde.py `hermite`).
fn wing_le(x: f32) -> f32 {
    let t = &W_LE;
    let n = t.len();
    let x = x.clamp(t[0][0], t[n - 1][0]);
    for i in 0..n - 1 {
        let (x0, x1) = (t[i][0], t[i + 1][0]);
        if x0 <= x && x <= x1 {
            let (v0, v1) = (t[i][1], t[i + 1][1]);
            let (a, d) = (t[i.saturating_sub(1)], t[(i + 2).min(n - 1)]);
            let m0 = (v1 - a[1]) / (x1 - a[0]);
            let m1 = (d[1] - v0) / (d[0] - x0);
            let h = x1 - x0;
            let u = (x - x0) / h;
            let (u2, u3) = (u * u, u * u * u);
            return (2.0 * u3 - 3.0 * u2 + 1.0) * v0 + (u3 - 2.0 * u2 + u) * h * m0 + (-2.0 * u3 + 3.0 * u2) * v1 + (u3 - u2) * h * m1;
        }
    }
    t[n - 1][1]
}

pub fn wing_sect(x: f32) -> Sect {
    let x = x.max(0.0);
    let le = wing_le(x);
    let c = le - (W_TE0 + W_TE_K * x.min(W_X1));
    Sect { le: Vec3::new(x, W_Z0 - W_ANH * (x - W_ANH_X).max(0.0), -le), c, cdir: Vec3::Z, up: Vec3::new(W_ANH, 1.0, 0.0).normalize(), tf: 1.0 }
}

/// The captain's eye: the left seat, behind the windscreen and beside the
/// forward side window (under the glass's top edge, clear of the roof).
const EYE: Vec3 = Vec3::new(-0.42, 0.60, -23.15);

/// The flap lever's notches, in degrees of nose droop as the flight model
/// counts them (`Spec::flaps`): nose and visor up; the visor down (it has
/// slid away by 0.3 of the travel); the nose at 5 degrees; at 12.5.
const NOSE: [f64; 4] = [0.0, 3.75, 7.54, 12.5];
/// How far along the lever's travel the visor is down and the nose starts to droop.
const VISOR_DOWN: f32 = 0.3;

// ---------------------------------------------------------------- the model

/// Every piece: name (as in the .blend), breakable group, what it is to the
/// damage model, and the piece it hangs from (listed before it). The nose and
/// its visor stand in for flaps: flown too fast drooped, they are what the
/// air takes.
const PIECES: &[(&str, usize, Role, Option<&str>)] = &[
    ("nose", G_PROP, Role::Flap(0), None),
    ("visor", G_PROP, Role::Flap(1), Some("nose")),
    ("visor frame", G_PROP, Role::Flap(1), Some("nose")),
    ("nose section", G_ENGINE, Role::Other, None),
    ("fuselage", G_FUSE, Role::Other, None),
    ("tail cone", G_TAIL, Role::TailCone, None),
    ("canopy", G_ENGINE, Role::Other, None),
    ("canopy frame", G_ENGINE, Role::Other, None),
    ("left wing", G_LWING, Role::Other, None),
    ("right wing", G_RWING, Role::Other, None),
    ("left intake", G_LWING, Role::Other, None),
    ("right intake", G_RWING, Role::Other, None),
    ("left nozzle", G_LWING, Role::Other, None),
    ("right nozzle", G_RWING, Role::Other, None),
    // (the inner pair are its elevators to the damage model, the others its ailerons)
    ("left inner elevon", G_LWING, Role::Elevator, None),
    ("right inner elevon", G_RWING, Role::Elevator, None),
    ("left middle elevon", G_LWING, Role::Aileron(0), None),
    ("right middle elevon", G_RWING, Role::Aileron(1), None),
    ("left outer elevon", G_LWING, Role::Aileron(0), None),
    ("right outer elevon", G_RWING, Role::Aileron(1), None),
    ("left nav light", G_LWING, Role::Other, None),
    ("right nav light", G_RWING, Role::Other, None),
    ("fin", G_TAIL, Role::Stab, None),
    ("lower rudder", G_TAIL, Role::Rudder, None),
    ("upper rudder", G_TAIL, Role::Rudder, None),
    ("tail light", G_TAIL, Role::Other, None),
    ("nose leg", G_GEAR, Role::Gear(0), None),
    ("nose fork", G_GEAR, Role::Gear(0), Some("nose leg")),
    ("nose wheel", G_GEAR, Role::Gear(0), Some("nose fork")),
    ("left main leg", G_GEAR, Role::Gear(1), None),
    ("left main strut", G_GEAR, Role::Gear(1), Some("left main leg")),
    ("left main bogie", G_GEAR, Role::Gear(1), Some("left main strut")),
    ("left main wheel", G_GEAR, Role::Gear(1), Some("left main bogie")),
    ("left rear main wheel", G_GEAR, Role::Gear(1), Some("left main bogie")),
    ("right main leg", G_GEAR, Role::Gear(2), None),
    ("right main strut", G_GEAR, Role::Gear(2), Some("right main leg")),
    ("right main bogie", G_GEAR, Role::Gear(2), Some("right main strut")),
    ("right main wheel", G_GEAR, Role::Gear(2), Some("right main bogie")),
    ("right rear main wheel", G_GEAR, Role::Gear(2), Some("right main bogie")),
    ("left nose door", G_ENGINE, Role::Other, None),
    ("right nose door", G_ENGINE, Role::Other, None),
    ("left main door", G_FUSE, Role::Other, None),
    ("right main door", G_FUSE, Role::Other, None),
    // The reheat plumes: drawn after everything else, in the transparent pass.
    ("afterburner", G_TAIL, Role::Other, None),
];

/// How a piece moves. Hinges point so that a positive angle puts a trailing
/// edge down (the rudders: to the right); the gear is modelled down and locked,
/// its doors open, the nose up and the visor raised. A piece's `angle` is how
/// far it turns to stow (the legs, the doors), to droop fully (the nose), or
/// how far it slides along its axis (in metres): the visor into the nose, and
/// the main legs' lower parts up into them as they retract (they shorten, as
/// the real ones do, or the bogies would meet under the cabin floor).
fn anim(name: &str, stow: f32, axis: Vec3) -> Anim {
    let left = name.starts_with("left");
    let s = if left { -1.0 } else { 1.0 };
    let leg = if name.starts_with("nose") { 0 } else if left { 1 } else { 2 };
    if name == "nose" {
        // droops once the visor is out of its way
        Anim::hinge(Drive::new(Input::Flaps, stow).smooth(VISOR_DOWN, 1.0))
    } else if name.starts_with("visor") {
        Anim::slide(Drive::new(Input::Flaps, stow).smooth(0.02, VISOR_DOWN - 0.02), axis)
    } else if name.ends_with("inner elevon") {
        // all six for pitch; the inner pair do less of the rolling
        Anim::hinge2(Drive::new(Input::Elevator, 0.30), Drive::new(Input::Aileron(s), 0.12))
    } else if name.ends_with("elevon") {
        Anim::hinge2(Drive::new(Input::Elevator, 0.30), Drive::new(Input::Aileron(s), 0.26))
    } else if name.ends_with("rudder") {
        Anim::hinge(Drive::new(Input::Rudder, 0.52))
    } else if name.ends_with(" leg") {
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow))
    } else if name.ends_with("main strut") {
        Anim::slide(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow), axis)
    } else if name.ends_with("main door") {
        // open for the bogie to pass, shut again behind it either way
        Anim::hinge(Drive::new(Input::Gear, -stow).pulse(0.0, 0.18, 0.88, 1.0).offset(stow))
    } else if name.ends_with(" door") {
        // open before the leg comes down, shut once it is up
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.0, 0.2).offset(stow))
    } else if name == "nose fork" {
        Anim::turn_slide(Drive::new(Input::Steer, 1.0), Drive::new(Input::Compress(0), 1.0), Vec3::Y)
    } else if name == "nose wheel" {
        Anim::hinge(Drive::new(Input::Wheel(0), -1.0))
    } else if name.ends_with("main bogie") {
        // the oleo: the bogie and its four wheels ride up the leg
        Anim::slide(Drive::new(Input::Compress(leg), 1.0), Vec3::Y)
    } else if name.ends_with("main wheel") {
        Anim::hinge(Drive::new(Input::Wheel(leg), -1.0))
    } else {
        Anim::Fixed
    }
}

pub fn build() -> Model {
    let objs = fa18::load(include_bytes!("../../../assets/concorde.bin.gz"), "concorde");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group, role, parent) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("concorde model has no piece called {name:?}"));
        let k = b.add(name, group, o.mesh.clone(), anim(name, o.angle, o.axis), o.pivot, o.axis, parent.map(|p| at[p]));
        b.role(k, role);
        if name == "canopy" || name == "visor" || name == "afterburner" {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.12, reach: 5.0, rays: 24, open: &|_, _| false });
    // The flight deck gets its own, much finer bake, shut in by the nose
    // section's skin and the windscreen's frame; the light comes in through the glass.
    let mut c = Builder::default();
    pit::build(&mut c);
    let n_pit = c.pieces.len();
    if n_pit > 0 {
        for name in ["nose section", "canopy frame"] {
            if let Some(k) = b.pieces.iter().position(|p| p.name == name) {
                let mut skin = b.pieces[k].mesh.clone();
                skin.i = skin.i.chunks(3).filter(|t| t.iter().all(|&i| { let z = skin.v[i as usize].pos[2]; z > -25.4 && z < -20.0 })).flatten().copied().collect();
                c.fixed("skin (occluder)", G_ENGINE, skin);
            }
        }
        bake_ao(&mut c.pieces, &AoBake { vox: 0.012, reach: 1.6, rays: 48, open: &|_, _| false });
    }
    let off = b.pieces.len();
    for mut p in c.pieces.into_iter().take(n_pit) {
        p.parent = p.parent.map(|i| i + off);
        b.pieces.push(p);
    }
    Model::finish(b.pieces)
}

// ---------------------------------------------------------------- the type

// (The gear's springs and dampers are estimates: the 747's, for a four-wheel bogie carrying 85 t.)
const fn main_leg(x: f64, name: &'static str, kick: f64) -> Leg {
    Leg {
        at: DVec3::new(x, -5.60, 2.60),
        wheel_r: 0.60,
        travel: 0.50,
        k: 3.4e6,
        c: 5.5e5,
        nf_max: 1.0e7,
        steer: 0.0,
        brakes: true,
        side_k: 1.0e6,
        side_mu: 0.8,
        roll_k: 1.8e4,
        roll_mu: 0.015,
        // carbon brakes
        brake_k: 5.0e5,
        brake_mu: 0.55,
        fold_vn: 4.5,
        fold_kick: DVec3::new(0.3, 0.0, kick),
        name,
    }
}

pub static SPEC: Spec = Spec {
    id: 6,
    name: "Concorde",
    keys: &["concorde"],
    blurb: "Four-engine supersonic airliner, 137 t. Mach 2 at 55,000 ft without reheat; Space for reheat to take off. No flaps: F droops the nose (5° to take off, 12.5° to land). Rotate at 200 kt to 10° and no more until it is off (the tail touches at 11.5°); 160 kt approach, 10° nose up. Z reversers.",
    still: include_bytes!("../../../assets/aircraft/concorde.png"),
    paint: Some(Paint { jpg: include_bytes!("../../../assets/concorde_paint.jpg"), views: 4, span: 44.0, length: 66.0, centre: [2.17, 1.0], relief: 1.0, sheen: 0.0 }),
    build,
    groups: [("fuselage", 34_000.0), ("tail", 4_500.0), ("left wing", 15_000.0), ("right wing", 15_000.0), ("flight deck", 3_500.0), ("nose", 700.0), ("gear", 6_000.0)],
    span: W_X1,
    wing_sect,
    fuse,
    aileron: [6.75, 12.3],
    tank: [1.5, 9.0],
    tail: Tail {
        fin_y0: 1.45,
        fin_h: 5.15,
        rudder_y0: 1.71,
        rudder_h: 4.58,
        // (No tailplane: the inner elevons stand in for it, and its tip for a scraped trailing edge.)
        stab_root: 1.5,
        elev_root: 1.5,
        break_z: [15.0, 19.0],
        fin_base: DVec3::new(0.0, 1.45, 14.0),
        fin_tip: DVec3::new(0.0, 6.6, 23.5),
        stab_tip: DVec3::new(4.05, -1.15, 18.55),
    },
    eye: EYE,
    eye2: None,
    sees_inside: pit::sees_inside,
    contacts: &[
        ([0.0, -0.80, -33.0], Part::Nose),
        ([0.0, -1.23, -29.0], Part::Nose),
        ([0.0, -0.19, -29.0], Part::Nose),
        ([0.0, -1.48, -25.0], Part::Nose),
        ([0.0, -1.65, -20.0], Part::Belly),
        ([0.0, -1.66, -10.0], Part::Belly),
        ([0.0, -1.66, 0.0], Part::Belly),
        ([0.0, -1.66, 9.0], Part::Belly),
        ([-1.44, 0.0, -16.0], Part::Side),
        ([1.44, 0.0, -16.0], Part::Side),
        ([-1.44, 0.0, 0.0], Part::Side),
        ([1.44, 0.0, 0.0], Part::Side),
        ([-1.42, 0.04, 13.0], Part::Side),
        ([1.42, 0.04, 13.0], Part::Side),
        ([0.0, 1.58, -20.0], Part::Roof),
        ([0.0, 1.66, 0.0], Part::Roof),
        ([0.0, 1.65, 15.0], Part::Roof),
        // the nacelles: their intakes' lower lips, and the nozzles' (what an over-rotation scrapes first)
        ([-5.43, -2.62, 7.6], Part::Pod(0)),
        ([5.43, -2.62, 7.6], Part::Pod(1)),
        ([-5.43, -2.29, 18.9], Part::Pod(0)),
        ([5.43, -2.29, 18.9], Part::Pod(1)),
    ],
    // under the tail cone: its bumper wheel on its long leg (it meets the runway as the nozzles would, at 11.5 degrees nose up), then on up to the tip
    tail_pts: &[[0.0, -1.24, 17.0], [0.0, -1.86, 21.0], [0.0, 0.12, 25.0], [0.0, 1.05, 28.66]],
    wing_st: [2.5 / W_X1, 5.0 / W_X1, 7.5 / W_X1, 10.0 / W_X1, 12.3 / W_X1],
    hard: [
        DVec3::new(0.0, -0.80, -33.0),
        DVec3::new(-12.82, -1.39, 17.0),
        DVec3::new(12.82, -1.39, 17.0),
        DVec3::new(0.0, 1.05, 28.66),
        DVec3::new(0.0, 6.6, 23.5),
        DVec3::new(0.0, -1.66, 0.0),
        DVec3::new(5.43, -2.62, 12.0),
    ],
    gear: &[
        Leg {
            at: DVec3::new(0.0, -5.60, -15.30),
            wheel_r: 0.40,
            travel: 0.40,
            k: 7.0e5,
            c: 1.2e5,
            nf_max: 2.5e6,
            steer: 1.0,
            brakes: false,
            side_k: 2.0e5,
            side_mu: 0.8,
            roll_k: 6.0e3,
            roll_mu: 0.015,
            brake_k: 0.0,
            brake_mu: 0.0,
            fold_vn: 3.8,
            fold_kick: DVec3::new(-0.4, 0.0, 0.0),
            name: "NOSE GEAR",
        },
        main_leg(-3.86, "LEFT MAIN GEAR", 0.3),
        main_leg(3.86, "RIGHT MAIN GEAR", -0.3),
    ],
    hull: Hull { k: 8.0e6, c: 1.2e6, nf_max: 1.6e7, drag: 4.0e5 },
    hole: Hole { fus: [1.44, 1.66, 0.0, 1.4], wing: [W_Z0, -W_ANH, 0.45, 0.02], stab: [0.0, 0.0, 0.0], fin: [1.45, 5.15, 0.16, 0.035], pods: [[5.43, -1.92, 1.1], [0.0; 3]] },
    // 78,700 kg empty, 10 t of crew, passengers and bags, and half its fuel: with the tanks full it is at its 185 t maximum.
    mass: 136_500.0,
    // 95,680 kg; four Olympus 593s: about 0.15 kg/s each idling, 4.5 at full dry thrust on the ground, 6.5 with reheat (estimates:
    // in the cruise, where the thin air leaves them a third of that thrust, it comes to about 22 t an hour)
    fuel_kg: 95_680.0,
    fuel_flow: [0.6, 18.0, 26.0],
    payload: [10_000.0, 13_000.0, 0.0],
    // Its reference chord is the root's, 27.66 m, with the centre of gravity
    // kept between 52% and 59% of it (by pumping fuel, which is not modelled).
    mac: 27.66,
    cg: [0.535, 0.515, 0.59],
    fuel_arm: 0.0,
    // Pitch, yaw, roll: estimates, from radii of gyration of 0.19 of the length and 0.13 of the span.
    inertia: DVec3::new(19.0e6, 20.5e6, 1.5e6),
    wing_area: 358.25,
    b: 25.6,
    // A slender delta: little lift for its angle of attack (though more than
    // its aspect ratio of 1.8 alone would give: the leading edges' vortices),
    // a great deal of drag at low speed, and no stall to speak of. At full
    // weight it leaves the ground at about the book's 217 kt some 11 degrees
    // nose up (its nozzles and tail wheel meet the runway at 11.5), and comes
    // down the approach at 160 kt with 13 degrees of angle of attack: 10
    // degrees nose up, which is what the drooping nose is for.
    cl0: 0.05,
    cl_a: 3.0,
    // Lift over drag by the book: 11.5 at Mach 0.94, 7.1 at Mach 2.04; about 4 to 5 on the approach.
    cd0: 0.008,
    k_ind: 0.19,
    cd_gear: 0.035,
    // (The nose and visor are no flaps: they add no lift and no drag to speak of.)
    cd_flap: [0.0, 0.0],
    flap_cl: 0.0,
    stall_alpha: 0.45,
    stall_flap: 0.0,
    stall_slat: 0.0,
    alpha_trim: 0.30,
    mcrit: 0.93,
    cd_wave: 0.0073,
    // (No speedbrake.)
    spoiler_cd: 0.0,
    spoiler_cl: 0.0,
    chute: None,
    // The nose: up with the visor up, visor down, 5 degrees, 12.5 degrees; the
    // visor and the nose at 5 degrees are good for 325 kt, fully down for 270 kt.
    flaps: &NOSE,
    vfe: &[1e9, 325.0 * KT, 325.0 * KT, 270.0 * KT],
    flap_rate: 1.0 / 10.0,
    slats_at: 0,
    lever: Some(Lever { name: ["NOSE", "NEZ"], notches: &[["NOSE UP", "NEZ RELEVÉ"], ["VISOR DOWN", "VISIÈRE BAISSÉE"], ["NOSE 5°", "NEZ 5°"], ["NOSE 12.5°", "NEZ 12,5°"]], parts: ["NOSE", "VISOR"] }),
    gear_rate: 1.0 / 12.0,
    // VMO 530 kt, Mach 2.04 (set by how hot the nose gets: 127 C).
    vne: 530.0 * KT,
    mmo: 2.04,
    flutter_v: 610.0 * KT,
    // Its intakes' ramps are scheduled for Mach 2: past it they pass less and less, which is what holds it there.
    inlet_mach: 2.02,
    dry_mach: 0.0,
    // Limit load 2.5 g, ultimate 3.75 g.
    g_limit: 2.6,
    g_break: 3.8,
    // (Its controls were signalled electrically, but with nothing between the pilot's hand and the load: no limit here.)
    fbw_g: 0.0,
    steer_max: 1.0,
    steer_fade: 12.0,
    // Four Olympus 593 Mk 610s: 140 kN each dry, 169.3 kN with reheat (Space),
    // bucket reversers on all four. Turbojets fed by ram air: a little less
    // thrust than standing still up to Mach 1, then more and more, until at
    // Mach 2 they make nearly twice what the thin air alone would leave them,
    // which is what cruises it there without reheat (`inlet_mach` stops it there).
    engine: Engine::Fan(Fan { n: 4, thrust: 140_000.0, idle: 0.6, idle_thrust: 0.05, n1_rpm: 6_500.0, spool_up: 0.8, spool_down: 0.6, reverse: 0.3, reverser_time: 2.0, boost: 1.2093, mach_lapse: [0.28, 0.35], nozzle_z: 18.9 }),
    // Estimates: an airliner's, quicker in roll than the 747 (a short span and powerful elevons) and with no stall break.
    handling: Handling {
        q_ref: 11_000.0,
        a_max: 1.8,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 1.8,
        pitch_k: 3.0,
        pitch_k_lost: 1.0,
        pitch_ctl: 0.25,
        alpha_ctl: 0.2,
        pitch_d: 5.5,
        roll_ctl: 1.5,
        roll_d: 2.75,
        roll_beta: 0.25,
        roll_clean: 0.7,
        yaw_ctl: 0.2,
        yaw_beta: 1.4,
        yaw_d: 2.5,
        dihedral: 0.06,
        adverse: 0.1,
        stall_pitch: 0.15,
        stall_drop: 0.2,
        stall_yaw: 0.4,
    },
    cruise: 180.0,
    approach: (82.0, 3),
    cam_dist: 76.0,
    shadow_r: 34.0,
    sound: Sound::Turbojet,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builds_whole() {
        let m = build();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &m.pieces {
            assert!(p.mesh.v.iter().all(|v| v.pos.iter().chain(v.nrm.iter()).all(|x| x.is_finite())), "{} has NaNs", p.name);
            assert!(!p.mesh.i.is_empty(), "{} is empty", p.name);
            // (The plumes trail 17 m behind the nozzles: not part of the airframe.)
            if p.name != "afterburner" {
                for v in &p.mesh.v {
                    lo = lo.min(Vec3::from(v.pos));
                    hi = hi.max(Vec3::from(v.pos));
                }
            }
        }
        // 61.66 m long, 25.6 m across, 12.2 m tall on its wheels.
        let (len, span, top) = (hi.z - lo.z, hi.x - lo.x, hi.y + 5.60);
        println!("{} pieces, length {len:.2} m, span {span:.2} m, height {top:.2} m", m.pieces.len());
        assert!((len - 61.66).abs() < 0.3, "{len}");
        assert!((span - 25.64).abs() < 0.3, "{span}");
        assert!((top - 12.20).abs() < 0.3, "{top}");
        // The wing in the flight and damage models is the one drawn.
        let tip = wing_sect(W_X1);
        assert!((tip.le.y - -1.391).abs() < 0.02 && (tip.le.z - 16.8).abs() < 0.05 && (tip.c - 0.79).abs() < 0.03, "{:?} {}", tip.le, tip.c);
        let mid = wing_sect(7.0);
        assert!((mid.le.z - 7.2).abs() < 0.05 && (mid.c - 11.03).abs() < 0.05, "{:?} {}", mid.le, mid.c);
        // Its area, the fuselage's width counted in, is near the book's 358 m² (the model's wing is a little full).
        let n = 2000;
        let area: f32 = (0..n).map(|i| wing_sect((i as f32 + 0.5) * W_X1 / n as f32).c * W_X1 / n as f32).sum::<f32>() * 2.0;
        println!("wing area {area:.0} m²");
        assert!(area > 350.0 && area < 400.0, "{area}");
    }

    #[test]
    fn gear_stows_inside_the_fuselage() {
        let m = build();
        let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        p.gear = 0.0;
        let pose = p.pose(&m.pieces);
        for (i, pc) in m.pieces.iter().enumerate().filter(|(_, pc)| pc.name.ends_with("wheel")) {
            let c = pose[i].transform_point3(pc.pivot);
            let r = fuse_at(-c.z);
            println!("{} stowed at {c:?}", pc.name);
            assert!(c.y > r[4] && c.y < r[2] && c.x.abs() < r[1], "{} sticks out: {c:?}", pc.name);
        }
    }

    /// The flap lever works the nose: the visor is away inside it before it
    /// starts to droop, to 5 degrees and then 12.5.
    #[test]
    fn the_nose_droops_by_notches() {
        let m = build();
        let at = |name: &str| m.pieces.iter().position(|p| p.name == name).unwrap();
        let (nose, visor) = (at("nose"), at("visor"));
        let tip = Vec3::new(0.0, -0.80, -33.0);
        let hinge = m.pieces[nose].pivot;
        let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        let mut seen = Vec::new();
        for notch in 0..4u8 {
            p.flaps = SPEC.flap_frac(notch);
            let pose = p.pose(&m.pieces);
            let droop = (tip - hinge).angle_between(pose[nose].transform_point3(tip) - hinge).to_degrees();
            let down = pose[nose].transform_point3(tip).y < tip.y + 1e-4;
            // the visor's slide, measured in the nose's own frame
            let slid = (pose[nose].inverse() * pose[visor]).transform_point3(m.pieces[visor].pivot).distance(m.pieces[visor].pivot);
            println!("notch {notch}: lever {:.2}, nose {droop:.1} deg, visor {slid:.2} m", p.flaps);
            assert!(down, "the nose goes up");
            seen.push((droop, slid));
        }
        assert!(seen[0].0 < 0.01 && seen[0].1 < 0.01, "{:?}", seen[0]);
        assert!(seen[1].0 < 0.01 && (seen[1].1 - 1.85).abs() < 0.01, "{:?}", seen[1]);
        assert!((seen[2].0 - 5.0).abs() < 0.1 && (seen[2].1 - 1.85).abs() < 0.01, "{:?}", seen[2]);
        assert!((seen[3].0 - 12.5).abs() < 0.1 && (seen[3].1 - 1.85).abs() < 0.01, "{:?}", seen[3]);
        // On the way down the visor has gone before the nose moves.
        for k in 0..=30 {
            p.flaps = k as f64 / 30.0;
            let pose = p.pose(&m.pieces);
            let droop = (tip - hinge).angle_between(pose[nose].transform_point3(tip) - hinge);
            let slid = (pose[nose].inverse() * pose[visor]).transform_point3(m.pieces[visor].pivot).distance(m.pieces[visor].pivot);
            assert!(droop < 1e-3 || slid > 1.849, "nose {droop} with the visor at {slid}");
        }
    }
}
