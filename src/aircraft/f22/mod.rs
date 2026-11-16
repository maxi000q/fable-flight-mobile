//! Lockheed Martin F-22A Raptor: a twin-engine, single-seat stealth fighter.
//!
//! Like the Hornet, the airframe is modelled in Blender
//! (`assets/blender/f22.py`, which also saves `f22.blend`) and exported to
//! `assets/f22.bin.gz`: one mesh per piece, with the hinge each one turns
//! about. How the pieces move, what they belong to and what they are to the
//! damage model is set here, by name. Rebuild the asset with
//!
//!   blender -b --factory-startup --python assets/blender/f22.py -- assets
//!
//! The cockpit inside is built here in Rust (`pit.rs`).
//!
//! Its paint is a picture (`assets/f22_paint.jpg`, see
//! `assets/blender/f22_paint.py`), projected onto the skin by plane.wgsl as the
//! Hornet's is; so are its exhaust plume (the "afterburner" piece, flattened
//! behind its two-dimensional nozzles) and its engines' sound (audio.rs
//! `Fighter`).
//!
//! The figures are the F-22A's: 18.92 m long, 13.56 m across, 5.08 m tall, a
//! 78.04 m² wing, 19,700 kg empty, 8,200 kg of fuel inside, two F119-PW-100s
//! of 26,000 lbf dry and 35,000 lbf in afterburner, Mach 2.25 at altitude
//! (1.76 without afterburner), +9 g.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, origin at the centre
//! of gravity.
use super::*;
use crate::airframe::*;
use std::collections::HashMap;

mod pit;

const KT: f64 = 1.0 / 1.94384;

// ---------------------------------------------------------------- geometry (in step with f22.py)

/// Fuselage stations: y forward (Blender frame), half width at the chine, top,
/// height of the chine, bottom; then how its faceted section is shaped: half
/// width of the flat across the top and across the bottom, and how much the
/// upper and lower facets bulge (1: flat).
const FUSE: [[f32; 9]; 16] = [
    [9.90, 0.00, -0.10, -0.10, -0.10, 0.00, 0.00, 1.6, 1.5],
    [9.60, 0.15, 0.02, -0.10, -0.23, 0.02, 0.03, 1.6, 1.5],
    [9.10, 0.32, 0.16, -0.10, -0.39, 0.04, 0.06, 1.6, 1.5],
    [8.40, 0.50, 0.31, -0.09, -0.55, 0.07, 0.10, 1.6, 1.5],
    [7.60, 0.68, 0.45, -0.07, -0.69, 0.12, 0.16, 1.6, 1.5],
    [6.80, 0.82, 0.55, -0.04, -0.79, 0.20, 0.22, 1.6, 1.5],
    [5.90, 0.94, 0.62, 0.00, -0.86, 0.28, 0.30, 1.6, 1.5],
    [5.00, 1.03, 0.66, 0.05, -0.92, 0.34, 0.40, 1.5, 1.4],
    [4.20, 1.12, 0.67, 0.09, -0.95, 0.40, 0.55, 1.5, 1.4],
    [3.20, 1.50, 0.66, 0.14, -0.98, 0.55, 0.95, 1.4, 1.3],
    [2.00, 1.86, 0.64, 0.18, -1.00, 0.75, 1.40, 1.3, 1.2],
    [0.50, 1.92, 0.62, 0.18, -1.00, 0.88, 1.45, 1.3, 1.2],
    [-1.50, 1.92, 0.58, 0.16, -0.98, 0.92, 1.45, 1.3, 1.2],
    [-3.50, 1.92, 0.52, 0.12, -0.92, 0.95, 1.42, 1.3, 1.2],
    [-5.00, 1.94, 0.44, 0.08, -0.80, 1.00, 1.40, 1.3, 1.2],
    [-6.00, 1.96, 0.36, 0.04, -0.60, 1.36, 1.36, 1.3, 1.2],
];

#[allow(dead_code)] // (the facet columns are for the cockpit)
fn fuse_at(y: f32) -> [f32; 9] {
    if y >= FUSE[0][0] {
        return FUSE[0];
    }
    for w in FUSE.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a[0] >= y && y >= b[0] {
            let t = (a[0] - y) / (a[0] - b[0]);
            let mut r = [0.0; 9];
            for k in 0..9 {
                r[k] = a[k] + (b[k] - a[k]) * t;
            }
            return r;
        }
    }
    FUSE[FUSE.len() - 1]
}

/// Fuselage at body station z: top, the height of its widest point (the chine), bottom.
fn fuse(z: f32) -> (f32, f32, f32) {
    let r = fuse_at(-z);
    (r[2], r[3], r[4])
}

// The wing, a clipped diamond: the tip's span station, the leading edge on
// the centreline and its sweep (m back per m out: 42 degrees), the chord on
// the centreline and what it loses per m out (the trailing edge is swept
// forward 17 degrees), its height on the centreline and its anhedral.
const W_X1: f32 = 6.78;
const W_LE0: f32 = 3.6;
const W_SWEEP: f32 = 0.9004;
const W_C0: f32 = 9.85;
const W_TAPER: f32 = 1.2061;
const W_Z0: f32 = 0.28;
const W_ANH: f32 = 0.0568;

pub fn wing_sect(x: f32) -> Sect {
    let x = x.max(0.0);
    Sect { le: Vec3::new(x, W_Z0 - W_ANH * x, -(W_LE0 - W_SWEEP * x)), c: W_C0 - W_TAPER * x.min(W_X1), cdir: Vec3::Z, up: Vec3::new(W_ANH, 1.0, 0.0).normalize(), tf: 1.0 }
}

/// The pilot's eye, under the canopy's highest point.
const EYE: Vec3 = Vec3::new(0.0, 1.0, -5.45);

// ---------------------------------------------------------------- the model

/// Every piece: name (as in the .blend), breakable group, what it is to the
/// damage model, and the piece it hangs from (listed before it).
const PIECES: &[(&str, usize, Role, Option<&str>)] = &[
    ("radome", G_PROP, Role::Other, None),
    ("nose section", G_ENGINE, Role::Other, None),
    ("fuselage", G_FUSE, Role::Other, None),
    ("tail cone", G_TAIL, Role::TailCone, None),
    ("left intake", G_FUSE, Role::Other, None),
    ("right intake", G_FUSE, Role::Other, None),
    ("left nozzle", G_TAIL, Role::Other, None),
    ("right nozzle", G_TAIL, Role::Other, None),
    ("canopy", G_ENGINE, Role::Other, None),
    ("canopy frame", G_ENGINE, Role::Other, None),
    ("left wing", G_LWING, Role::Other, None),
    ("right wing", G_RWING, Role::Other, None),
    ("left le flap", G_LWING, Role::Other, None),
    ("right le flap", G_RWING, Role::Other, None),
    // (the flaperons)
    ("left flap", G_LWING, Role::Flap(0), None),
    ("right flap", G_RWING, Role::Flap(1), None),
    ("left aileron", G_LWING, Role::Aileron(0), None),
    ("right aileron", G_RWING, Role::Aileron(1), None),
    ("left nav light", G_LWING, Role::Other, None),
    ("right nav light", G_RWING, Role::Other, None),
    ("left stabilator", G_TAIL, Role::Elevator, None),
    ("right stabilator", G_TAIL, Role::Elevator, None),
    ("left fin", G_TAIL, Role::Stab, None),
    ("right fin", G_TAIL, Role::Stab, None),
    ("left rudder", G_TAIL, Role::Rudder, None),
    ("right rudder", G_TAIL, Role::Rudder, None),
    ("tail light", G_TAIL, Role::Other, None),
    ("nose leg", G_GEAR, Role::Gear(0), None),
    ("nose fork", G_GEAR, Role::Gear(0), Some("nose leg")),
    ("nose wheel", G_GEAR, Role::Gear(0), Some("nose fork")),
    ("left main leg", G_GEAR, Role::Gear(1), None),
    ("left main wheel", G_GEAR, Role::Gear(1), Some("left main leg")),
    ("right main leg", G_GEAR, Role::Gear(2), None),
    ("right main wheel", G_GEAR, Role::Gear(2), Some("right main leg")),
    ("left nose door", G_ENGINE, Role::Other, None),
    ("right nose door", G_ENGINE, Role::Other, None),
    ("left strut door", G_FUSE, Role::Other, None),
    ("right strut door", G_FUSE, Role::Other, None),
    ("left main door", G_FUSE, Role::Other, None),
    ("right main door", G_FUSE, Role::Other, None),
    // The exhaust plume: drawn after everything else, in the transparent pass.
    ("afterburner", G_TAIL, Role::Other, None),
];

/// How a piece moves. Hinges point so that a positive angle puts a trailing
/// edge down (the rudders: to the right); the gear is modelled down and locked,
/// its doors open. A piece's `angle` is how far it turns to stow: the legs
/// swing forward, the doors shut. It has no speedbrake: it brakes with its
/// ailerons up and its rudders toed out.
fn anim(name: &str, stow: f32) -> Anim {
    let left = name.starts_with("left");
    let s = if left { -1.0 } else { 1.0 };
    let leg = if name.starts_with("nose") { 0 } else if left { 1 } else { 2 };
    if name.ends_with("le flap") {
        // leading-edge flaps droop with the flaperons
        Anim::hinge(Drive::new(Input::Slats, -0.50))
    } else if name.ends_with(" flap") {
        // flaperons: flaps and ailerons in one
        Anim::hinge2(Drive::new(Input::Flaps, 35f32.to_radians()), Drive::new(Input::Aileron(s), 0.30))
    } else if name.ends_with("aileron") {
        Anim::hinge2(Drive::new(Input::Aileron(s), 0.40), Drive::new(Input::Spoiler(0.0), -0.50))
    } else if name.ends_with("stabilator") {
        // all-moving tailplanes: together for pitch, apart to help the ailerons roll
        Anim::hinge2(Drive::new(Input::Elevator, 0.42), Drive::new(Input::Aileron(s), 0.10))
    } else if name.ends_with("rudder") {
        Anim::hinge2(Drive::new(Input::Rudder, 0.52), Drive::new(Input::Spoiler(0.0), s * 0.45))
    } else if name.ends_with("nozzle") {
        // thrust vectoring, in pitch only, with the tailplanes
        Anim::hinge(Drive::new(Input::Elevator, 0.35))
    } else if name.ends_with(" leg") {
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow))
    } else if name.ends_with("main door") {
        // open for the leg to pass, shut again behind it either way
        Anim::hinge(Drive::new(Input::Gear, -stow).pulse(0.0, 0.18, 0.88, 1.0).offset(stow))
    } else if name.ends_with(" door") {
        // open before the legs come down, shut once they are up
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.0, 0.2).offset(stow))
    } else if name == "nose fork" {
        Anim::turn_slide(Drive::new(Input::Steer, 1.0), Drive::new(Input::Compress(0), 1.0), Vec3::Y)
    } else if name == "nose wheel" {
        Anim::hinge(Drive::new(Input::Wheel(0), -1.0))
    } else if name.ends_with("main wheel") {
        Anim::turn_slide(Drive::new(Input::Wheel(leg), -1.0), Drive::new(Input::Compress(leg), 1.0), Vec3::Y)
    } else {
        Anim::Fixed
    }
}

pub fn build() -> Model {
    let objs = fa18::load(include_bytes!("../../../assets/f22.bin.gz"), "f22");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group, role, parent) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("f22 model has no piece called {name:?}"));
        let k = b.add(name, group, o.mesh.clone(), anim(name, o.angle), o.pivot, o.axis, parent.map(|p| at[p]));
        b.role(k, role);
        if name == "canopy" || name == "afterburner" {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.06, reach: 3.0, rays: 24, open: &|_, _| false });
    // The cockpit gets its own, much finer bake, shut in by the nose's skin
    // and the canopy's frame; the light comes in through the glass.
    let mut c = Builder::default();
    pit::build(&mut c);
    let n_pit = c.pieces.len();
    if n_pit > 0 {
        for name in ["nose section", "canopy frame"] {
            if let Some(k) = b.pieces.iter().position(|p| p.name == name) {
                let mut skin = b.pieces[k].mesh.clone();
                skin.i = skin.i.chunks(3).filter(|t| t.iter().all(|&i| { let z = skin.v[i as usize].pos[2]; z > -7.4 && z < -3.6 })).flatten().copied().collect();
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

// (The gear's springs and dampers are estimates: the Hornet's, stiffened for the weight.)
const fn main_leg(x: f64, name: &'static str, kick: f64) -> Leg {
    Leg {
        at: DVec3::new(x, -2.00, 0.95),
        wheel_r: 0.36,
        travel: 0.35,
        k: 9.0e5,
        c: 1.1e5,
        nf_max: 2.6e6,
        steer: 0.0,
        brakes: true,
        side_k: 2.4e5,
        side_mu: 0.8,
        roll_k: 3.0e3,
        roll_mu: 0.02,
        brake_k: 8.0e4,
        brake_mu: 0.6,
        // a runway fighter's gear: not a carrier aircraft's
        fold_vn: 4.5,
        fold_kick: DVec3::new(0.3, 0.0, kick),
        name,
    }
}

pub static SPEC: Spec = Spec {
    id: 4,
    name: "F-22A Raptor",
    keys: &["f22", "f-22", "f22a", "f-22a", "raptor"],
    blurb: "Twin-engine stealth fighter, 25 t. Fly-by-wire holds it to 9 g; supersonic without afterburner up high. Space for afterburner, / to brake. 145 kt approach.",
    still: include_bytes!("../../../assets/aircraft/f22.png"),
    paint: Some(Paint { jpg: include_bytes!("../../../assets/f22_paint.jpg"), views: 4, span: 14.4, length: 21.6, centre: [0.4, 1.0], relief: 1.0, sheen: 0.0 }),
    build,
    groups: [("fuselage", 11_500.0), ("tail", 1_800.0), ("left wing", 2_200.0), ("right wing", 2_200.0), ("nose section", 1_300.0), ("radome", 90.0), ("gear", 800.0)],
    span: W_X1,
    wing_sect,
    fuse,
    aileron: [4.3, 6.4],
    tank: [2.0, 4.6],
    tail: Tail {
        // (the fins lean out 28 degrees: heights are measured straight up)
        fin_y0: 0.42,
        fin_h: 2.66,
        rudder_y0: 0.50,
        rudder_h: 1.78,
        stab_root: 1.75,
        elev_root: 1.75,
        break_z: [5.0, 6.0],
        fin_base: DVec3::new(1.45, 0.42, 3.7),
        fin_tip: DVec3::new(2.86, 3.08, 4.5),
        stab_tip: DVec3::new(4.42, 0.05, 7.9),
    },
    eye: EYE,
    eye2: None,
    sees_inside: pit::sees_inside,
    contacts: &[
        ([0.0, -0.10, -9.9], Part::Nose),
        ([0.0, -0.55, -8.4], Part::Nose),
        ([0.0, 0.31, -8.4], Part::Nose),
        ([0.0, -0.86, -5.9], Part::Belly),
        ([0.0, -0.95, -4.2], Part::Belly),
        ([0.0, -1.00, -0.5], Part::Belly),
        ([0.0, -0.98, 1.5], Part::Belly),
        ([-1.40, -1.00, -1.0], Part::Belly),
        ([1.40, -1.00, -1.0], Part::Belly),
        ([-1.92, 0.18, -0.5], Part::Side),
        ([1.92, 0.18, -0.5], Part::Side),
        ([-0.94, 0.0, -5.9], Part::Side),
        ([0.94, 0.0, -5.9], Part::Side),
        ([-1.92, 0.12, 3.5], Part::Side),
        ([1.92, 0.12, 3.5], Part::Side),
        ([0.0, 1.25, -5.2], Part::Roof),
        ([0.0, 0.66, -3.2], Part::Roof),
        ([0.0, 0.58, 1.5], Part::Roof),
    ],
    // under the tail cone, then the nozzles' lower lips
    tail_pts: &[[0.0, -0.80, 5.0], [0.0, -0.60, 6.0], [-0.78, -0.34, 7.3], [0.78, -0.34, 7.3]],
    wing_st: [2.4 / W_X1, 3.4 / W_X1, 4.4 / W_X1, 5.4 / W_X1, 6.5 / W_X1],
    hard: [
        DVec3::new(0.0, -0.10, -9.9),
        DVec3::new(-6.78, -0.10, 3.3),
        DVec3::new(6.78, -0.10, 3.3),
        DVec3::new(0.0, 0.05, 9.0),
        DVec3::new(2.86, 3.08, 4.5),
        DVec3::new(0.0, -1.00, -0.5),
        DVec3::new(0.0, -1.00, 1.0),
    ],
    gear: &[
        Leg {
            at: DVec3::new(0.0, -2.00, -5.00),
            wheel_r: 0.30,
            travel: 0.30,
            k: 4.0e5,
            c: 4.5e4,
            nf_max: 9.0e5,
            steer: 1.0,
            brakes: false,
            side_k: 9.0e4,
            side_mu: 0.8,
            roll_k: 1.5e3,
            roll_mu: 0.02,
            brake_k: 0.0,
            brake_mu: 0.0,
            fold_vn: 4.0,
            fold_kick: DVec3::new(-0.2, 0.0, 0.0),
            name: "NOSE GEAR",
        },
        main_leg(-1.615, "LEFT MAIN GEAR", 0.2),
        main_leg(1.615, "RIGHT MAIN GEAR", -0.2),
    ],
    hull: Hull { k: 1.8e6, c: 2.4e5, nf_max: 4.0e6, drag: 7.0e4 },
    hole: Hole { fus: [1.92, 0.82, -0.18, 0.3], wing: [W_Z0, -W_ANH, 0.20, 0.04], stab: [0.05, 0.06, 4.42], fin: [0.42, 2.66, 0.08, 0.035], pods: [[0.0; 3]; 2] },
    // 19,700 kg empty, its pilot, half its fuel and 1.4 t of missiles in the bays: with the tanks full it is the book's 29.4 t gross (maximum 38 t).
    mass: 25_300.0,
    // 8,200 kg inside; two F119s: about 0.35 kg/s idling, 5.2 at military power, 16.5 in full afterburner (estimates)
    fuel_kg: 8_200.0,
    fuel_flow: [0.35, 5.2, 16.5],
    // (Its pilot is all the payload there is.)
    payload: [100.0, 100.0, 0.0],
    // The model's wing puts the centre of gravity at 19% of its mean chord (the real one's is further aft).
    mac: 6.73,
    cg: [0.19, 0.14, 0.26],
    fuel_arm: 0.4,
    // Pitch, yaw, roll: estimates, from radii of gyration like the Hornet's (0.19 of the length, 0.125 of the span).
    inertia: DVec3::new(328.0e3, 405.0e3, 73.0e3),
    wing_area: 78.04,
    b: 13.56,
    // A big, thin wing of aspect ratio 2.36, helped by the chines' vortices at high angles of attack.
    cl0: 0.03,
    cl_a: 3.3,
    cd0: 0.017,
    k_ind: 0.17,
    cd_gear: 0.015,
    cd_flap: [0.02, 0.03],
    flap_cl: 0.35,
    // It stays under control far beyond where other wings let go.
    stall_alpha: 0.65,
    stall_flap: 0.03,
    stall_slat: 0.05,
    alpha_trim: 0.45,
    mcrit: 0.88,
    cd_wave: 0.0315,
    // (No speedbrake: ailerons up and rudders out.)
    spoiler_cd: 0.04,
    spoiler_cl: 0.0,
    chute: None,
    // Flaperons: up, half, full.
    flaps: &[0.0, 20.0, 35.0],
    vfe: &[1e9, 260.0 * KT, 240.0 * KT],
    flap_rate: 0.4,
    slats_at: 1,
    lever: None,
    gear_rate: 1.0 / 5.0,
    // 800 kt (Mach 1.21 at sea level), Mach 2.25.
    vne: 800.0 * KT,
    mmo: 2.25,
    flutter_v: 870.0 * KT,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    g_limit: 10.5,
    g_break: 14.0,
    fbw_g: 9.0,
    steer_max: 0.9,
    steer_fade: 15.0,
    // Two F119-PW-100s: 26,000 lbf (115.7 kN) each dry, 35,000 lbf (155.7 kN) in
    // afterburner (Space). The lapse and the wave drag are set against each
    // other so that at military power it stays near Mach 1.1 down low but
    // supercruises at 40,000 ft (Mach 1.5 here; 1.76 by the book), and the
    // afterburners take it on to its Mach 2.25 limit there, not far past it.
    engine: Engine::Fan(Fan { n: 2, thrust: 115_700.0, idle: 0.62, idle_thrust: 0.05, n1_rpm: 10_000.0, spool_up: 1.0, spool_down: 0.8, reverse: 0.0, reverser_time: 1.0, boost: 1.346, mach_lapse: [0.1882, 0.3782], nozzle_z: 7.3 }),
    // Estimates: the Hornet's, with more authority about every axis and a gentler stall.
    handling: Handling {
        q_ref: 8_000.0,
        a_max: 4.0,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 4.0,
        pitch_k: 6.0,
        pitch_k_lost: 2.0,
        pitch_ctl: 0.16,
        alpha_ctl: 0.36,
        pitch_d: 5.0,
        roll_ctl: 5.5,
        roll_d: 3.0,
        roll_beta: 0.3,
        roll_clean: 1.0,
        yaw_ctl: 0.9,
        yaw_beta: 3.2,
        yaw_d: 2.8,
        dihedral: 0.04,
        adverse: 0.05,
        stall_pitch: 0.25,
        stall_drop: 0.3,
        stall_yaw: 0.5,
    },
    cruise: 240.0,
    approach: (75.0, 2),
    cam_dist: 29.0,
    shadow_r: 12.5,
    sound: Sound::Fighter,
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
            // (The plume trails 12 m behind the nozzles: not part of the airframe.)
            if p.name != "afterburner" {
                for v in &p.mesh.v {
                    lo = lo.min(Vec3::from(v.pos));
                    hi = hi.max(Vec3::from(v.pos));
                }
            }
        }
        // 18.92 m long, 13.56 m across, 5.08 m tall on its wheels.
        let (len, span, top) = (hi.z - lo.z, hi.x - lo.x, hi.y + 2.00);
        println!("{} pieces, length {len:.2} m, span {span:.2} m, height {top:.2} m", m.pieces.len());
        assert!((len - 18.92).abs() < 0.3, "{len}");
        assert!((span - 13.57).abs() < 0.3, "{span}");
        assert!((top - 5.09).abs() < 0.3, "{top}");
        // The wing in the flight and damage models is the one drawn.
        let tip = wing_sect(W_X1);
        assert!((tip.le.y - -0.105).abs() < 0.03 && (tip.le.z - 2.505).abs() < 0.05 && (tip.c - 1.67).abs() < 0.03, "{:?} {}", tip.le, tip.c);
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
}
