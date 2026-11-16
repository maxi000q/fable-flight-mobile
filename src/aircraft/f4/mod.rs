//! McDonnell Douglas F-4E Phantom II: a twin-engine, two-seat fighter.
//!
//! Like the Hornet, the airframe is modelled in Blender
//! (`assets/blender/f4.py`, which also saves `f4.blend`) and exported to
//! `assets/f4.bin.gz`: one mesh per piece, with the hinge each one turns
//! about. How the pieces move, what they belong to and what they are to the
//! damage model is set here, by name. Rebuild the asset with
//!
//!   blender -b --factory-startup --python assets/blender/f4.py -- assets
//!
//! The two cockpits inside are built here in Rust (`pit.rs`).
//!
//! Its paint is a picture (`assets/f4_paint.jpg`, see
//! `assets/blender/f4_paint.py`), projected onto the skin by plane.wgsl as the
//! Hornet's is; so are its exhaust plume (the "afterburner" piece) and its
//! engines' sound (audio.rs `Fighter`, as turbojets).
//!
//! The figures are the F-4E's: 19.2 m long, 11.7 m across, 5.0 m tall, a
//! 49.2 m² wing, 13,757 kg empty, 5,879 kg of fuel inside, two J79-GE-17As of
//! 52.96 kN dry and 79.38 kN in afterburner, Mach 2.23 at 40,000 ft.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, origin at the centre
//! of gravity.
use super::*;
use crate::airframe::*;
use std::collections::HashMap;

mod pit;

const KT: f64 = 1.0 / 1.94384;

// ---------------------------------------------------------------- geometry (in step with f4.py)

/// Fuselage stations: y forward (Blender frame), half width, top, height of
/// the widest point, bottom, superellipse exponent. Aft of the nozzles
/// (-4.05) it is the tail boom alone.
const FUSE: [[f32; 6]; 23] = [
    [9.80, 0.000, -0.33, -0.33, -0.33, 2.0],
    [9.55, 0.110, -0.20, -0.32, -0.44, 2.0],
    [9.10, 0.220, -0.06, -0.31, -0.55, 2.0],
    [8.50, 0.320, 0.08, -0.30, -0.66, 2.0],
    [7.90, 0.400, 0.19, -0.29, -0.76, 2.0],
    [7.20, 0.455, 0.31, -0.27, -0.83, 2.1],
    [6.50, 0.500, 0.42, -0.25, -0.88, 2.2],
    [5.80, 0.540, 0.52, -0.22, -0.92, 2.3],
    [5.00, 0.580, 0.60, -0.20, -0.95, 2.4],
    [4.20, 0.620, 0.66, -0.18, -0.96, 2.5],
    [3.40, 0.660, 0.72, -0.16, -0.96, 2.6],
    [2.60, 0.720, 0.78, -0.15, -0.96, 2.8],
    [1.80, 1.000, 0.84, -0.15, -0.96, 3.2],
    [0.80, 1.300, 0.90, -0.15, -0.96, 3.6],
    [-0.50, 1.340, 0.93, -0.15, -0.95, 3.8],
    [-1.80, 1.280, 0.94, -0.15, -0.92, 3.6],
    [-3.00, 1.120, 0.95, -0.12, -0.86, 3.2],
    [-3.70, 1.020, 0.95, -0.05, -0.80, 3.0],
    [-4.05, 0.600, 0.95, 0.50, 0.10, 2.6],
    [-5.00, 0.520, 0.95, 0.54, 0.16, 2.4],
    [-6.50, 0.420, 0.93, 0.58, 0.26, 2.2],
    [-8.00, 0.270, 0.86, 0.62, 0.40, 2.0],
    [-8.90, 0.030, 0.66, 0.64, 0.62, 2.0],
];

fn fuse_at(y: f32) -> [f32; 6] {
    if y >= FUSE[0][0] {
        return FUSE[0];
    }
    for w in FUSE.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a[0] >= y && y >= b[0] {
            let t = (a[0] - y) / (a[0] - b[0]);
            let mut r = [0.0; 6];
            for k in 0..6 {
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

// The wing: the tip's span station, the leading edge at the centreline and
// its sweep (m back per m out: 51.4 degrees, 45 at the quarter chord), the
// chords at the centreline and the tip, its height. Outboard of W_FOLD the
// panel rises 12 degrees and its leading edge is W_DOG of the chord further
// forward (the dogtooth).
const W_X1: f32 = 5.85;
const W_LE0: f32 = 4.30;
const W_SWEEP: f32 = 1.2526;
const W_C0: f32 = 7.00;
const W_C1: f32 = 1.40;
const W_Z0: f32 = -0.78;
const W_FOLD: f32 = 4.05;
const W_DIH: f32 = 0.20944;
const W_DOG: f32 = 0.10;

pub fn wing_sect(x: f32) -> Sect {
    let x = x.max(0.0);
    let c = W_C0 + (W_C1 - W_C0) * (x / W_X1).min(1.0);
    let y = W_LE0 - W_SWEEP * x;
    if x > W_FOLD {
        Sect { le: Vec3::new(x, W_Z0 + (x - W_FOLD) * W_DIH.tan(), -(y + W_DOG * c)), c: c * (1.0 + W_DOG), cdir: Vec3::Z, up: Vec3::new(-W_DIH.sin(), W_DIH.cos(), 0.0), tf: 1.0 }
    } else {
        Sect { le: Vec3::new(x, W_Z0, -y), c, cdir: Vec3::Z, up: Vec3::Y, tf: 1.0 }
    }
}

/// The pilot's eye, in the front seat.
const EYE: Vec3 = Vec3::new(0.0, 1.08, -5.1);

// ---------------------------------------------------------------- the model

/// Every piece: name (as in the .blend), breakable group, what it is to the
/// damage model, and the piece it hangs from (listed before it).
const PIECES: &[(&str, usize, Role, Option<&str>)] = &[
    ("radome", G_PROP, Role::Other, None),
    ("nose section", G_ENGINE, Role::Other, None),
    ("fuselage", G_FUSE, Role::Other, None),
    ("tail cone", G_TAIL, Role::TailCone, None),
    ("spine", G_FUSE, Role::Other, None),
    ("left intake", G_FUSE, Role::Other, None),
    ("right intake", G_FUSE, Role::Other, None),
    ("left nozzle", G_FUSE, Role::Other, None),
    ("right nozzle", G_FUSE, Role::Other, None),
    ("canopy", G_ENGINE, Role::Other, None),
    ("canopy frame", G_ENGINE, Role::Other, None),
    ("left wing", G_LWING, Role::Other, None),
    ("right wing", G_RWING, Role::Other, None),
    ("left inner le flap", G_LWING, Role::Other, None),
    ("right inner le flap", G_RWING, Role::Other, None),
    ("left le flap", G_LWING, Role::Other, None),
    ("right le flap", G_RWING, Role::Other, None),
    ("left flap", G_LWING, Role::Flap(0), None),
    ("right flap", G_RWING, Role::Flap(1), None),
    ("left aileron", G_LWING, Role::Aileron(0), None),
    ("right aileron", G_RWING, Role::Aileron(1), None),
    ("left speedbrake", G_LWING, Role::Other, None),
    ("right speedbrake", G_RWING, Role::Other, None),
    ("left nav light", G_LWING, Role::Other, None),
    ("right nav light", G_RWING, Role::Other, None),
    ("left stabilator", G_TAIL, Role::Elevator, None),
    ("right stabilator", G_TAIL, Role::Elevator, None),
    ("fin", G_TAIL, Role::Stab, None),
    ("rudder", G_TAIL, Role::Rudder, None),
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
    ("left main door", G_LWING, Role::Other, None),
    ("right main door", G_RWING, Role::Other, None),
    // The exhaust plume: drawn after everything else, in the transparent pass.
    ("afterburner", G_FUSE, Role::Other, None),
];

/// How a piece moves. Hinges point so that a positive angle puts a trailing
/// edge down (the rudder: to the right); the gear is modelled down and locked,
/// its doors open. A piece's `angle` is how far it turns to stow: the nose leg
/// swings aft, the main legs inward into the wing, the doors shut.
fn anim(name: &str, stow: f32) -> Anim {
    let left = name.starts_with("left");
    let s = if left { -1.0 } else { 1.0 };
    let leg = if name.starts_with("nose") { 0 } else if left { 1 } else { 2 };
    if name.ends_with("le flap") {
        // leading-edge flaps (both panels') droop with the trailing-edge flaps
        Anim::hinge(Drive::new(Input::Slats, -0.52))
    } else if name.ends_with(" flap") {
        Anim::hinge(Drive::new(Input::Flaps, 60f32.to_radians()))
    } else if name.ends_with("aileron") {
        // (The real ones only go down, spoilers on the other wing doing the rest: there are none here.)
        Anim::hinge(Drive::new(Input::Aileron(s), 0.40))
    } else if name.ends_with("stabilator") {
        // one slab on a level shaft through the boom, the halves moving as one:
        // 21 degrees leading edge down (stick back) to 9 up, as the real one's travel
        Anim::hinge(Drive::new(Input::Elevator, 0.26).offset(-0.105))
    } else if name == "rudder" {
        Anim::hinge(Drive::new(Input::Rudder, 0.52))
    } else if name.ends_with("speedbrake") {
        // under each wing, hinged at the front
        Anim::hinge(Drive::new(Input::Spoiler(0.0), 0.87))
    } else if name.ends_with(" leg") {
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow))
    } else if name.ends_with(" door") {
        // open before the legs come down, shut once they are up (the main
        // doors, inboard of the wheels, stay open with the gear down)
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
    let objs = fa18::load(include_bytes!("../../../assets/f4.bin.gz"), "f4");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group, role, parent) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("f4 model has no piece called {name:?}"));
        let k = b.add(name, group, o.mesh.clone(), anim(name, o.angle), o.pivot, o.axis, parent.map(|p| at[p]));
        b.role(k, role);
        if name == "canopy" || name == "afterburner" {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.06, reach: 3.0, rays: 24, open: &|_, _| false });
    // The cockpits get their own, much finer bake, shut in by the nose's skin
    // and the canopy's frame; the light comes in through the glass.
    let mut c = Builder::default();
    pit::build(&mut c);
    let n_pit = c.pieces.len();
    if n_pit > 0 {
        for name in ["nose section", "canopy frame"] {
            if let Some(k) = b.pieces.iter().position(|p| p.name == name) {
                let mut skin = b.pieces[k].mesh.clone();
                skin.i = skin.i.chunks(3).filter(|t| t.iter().all(|&i| { let z = skin.v[i as usize].pos[2]; z > -6.7 && z < -2.3 })).flatten().copied().collect();
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

const fn main_leg(x: f64, name: &'static str, kick: f64) -> Leg {
    Leg {
        at: DVec3::new(x, -2.05, 0.80),
        wheel_r: 0.38,
        travel: 0.35,
        k: 5.5e5,
        c: 7.0e4,
        nf_max: 1.6e6,
        steer: 0.0,
        brakes: true,
        side_k: 1.6e5,
        side_mu: 0.8,
        roll_k: 2.0e3,
        roll_mu: 0.02,
        brake_k: 5.0e4,
        brake_mu: 0.6,
        // naval gear, kept by the Air Force: built for a 7 m/s arrival
        fold_vn: 7.3,
        fold_kick: DVec3::new(0.3, 0.0, kick),
        name,
    }
}

pub static SPEC: Spec = Spec {
    id: 3,
    name: "F-4E Phantom II",
    keys: &["f4", "f-4", "f4e", "f-4e", "phantom"],
    blurb: "Twin-engine two-seat fighter, 17 t. No fly-by-wire: mind the g and the angle of attack; Space for afterburner, / speedbrakes. 150 kt approach.",
    still: include_bytes!("../../../assets/aircraft/f4.png"),
    paint: Some(Paint { jpg: include_bytes!("../../../assets/f4_paint.jpg"), views: 4, span: 13.2, length: 19.8, centre: [0.4, 1.0], relief: 1.0, sheen: 0.0 }),
    build,
    groups: [("fuselage", 8_000.0), ("tail", 1_000.0), ("left wing", 1_500.0), ("right wing", 1_500.0), ("nose section", 1_100.0), ("radome", 80.0), ("gear", 600.0)],
    span: W_X1,
    wing_sect,
    fuse,
    aileron: [2.82, 4.05],
    tank: [0.6, 4.0],
    tail: Tail {
        fin_y0: 0.88,
        fin_h: 2.07,
        rudder_y0: 0.88,
        rudder_h: 1.24,
        stab_root: 0.3,
        elev_root: 0.3,
        break_z: [4.2, 5.5],
        fin_base: DVec3::new(0.0, 0.88, 5.5),
        fin_tip: DVec3::new(0.0, 2.95, 8.4),
        stab_tip: DVec3::new(2.70, -0.50, 9.0),
    },
    eye: EYE,
    // (the weapon systems officer's, 1.45 m behind)
    eye2: Some(Vec3::new(0.0, 1.10, -3.65)),
    sees_inside: pit::sees_inside,
    contacts: &[
        ([0.0, -0.33, -9.8], Part::Nose),
        ([0.0, -0.76, -7.9], Part::Nose),
        ([0.0, 0.19, -7.9], Part::Nose),
        ([0.0, -0.92, -5.8], Part::Belly),
        ([0.0, -0.96, -3.4], Part::Belly),
        ([0.0, -0.96, -0.8], Part::Belly),
        ([0.0, -0.92, 1.8], Part::Belly),
        ([-1.10, -0.90, 0.5], Part::Belly),
        ([1.10, -0.90, 0.5], Part::Belly),
        ([-1.36, -0.15, -0.8], Part::Side),
        ([1.36, -0.15, -0.8], Part::Side),
        ([-0.58, -0.20, -5.0], Part::Side),
        ([0.58, -0.20, -5.0], Part::Side),
        ([-1.12, -0.12, 3.0], Part::Side),
        ([1.12, -0.12, 3.0], Part::Side),
        ([0.0, 1.39, -3.6], Part::Roof),
        ([0.0, 1.17, -0.6], Part::Roof),
        ([0.0, 0.98, 3.0], Part::Roof),
    ],
    // the nozzles' lips, then along under the tail boom
    tail_pts: &[[-0.50, -0.64, 4.8], [0.50, -0.64, 4.8], [0.0, 0.26, 6.5], [0.0, 0.40, 8.0]],
    wing_st: [1.4 / W_X1, 2.4 / W_X1, 3.4 / W_X1, 4.4 / W_X1, 5.5 / W_X1],
    hard: [
        DVec3::new(0.0, -0.33, -9.8),
        DVec3::new(-5.85, -0.40, 3.6),
        DVec3::new(5.85, -0.40, 3.6),
        DVec3::new(0.0, 0.64, 8.9),
        DVec3::new(0.0, 2.95, 8.4),
        DVec3::new(0.0, -0.96, -0.5),
        DVec3::new(0.0, -0.94, 2.0),
    ],
    gear: &[
        Leg {
            at: DVec3::new(0.0, -2.05, -6.30),
            wheel_r: 0.23,
            travel: 0.30,
            k: 2.5e5,
            c: 3.0e4,
            nf_max: 6.0e5,
            steer: 1.0,
            brakes: false,
            side_k: 6.0e4,
            side_mu: 0.8,
            roll_k: 1.0e3,
            roll_mu: 0.02,
            brake_k: 0.0,
            brake_mu: 0.0,
            fold_vn: 6.5,
            fold_kick: DVec3::new(-0.2, 0.0, 0.0),
            name: "NOSE GEAR",
        },
        main_leg(-2.72, "LEFT MAIN GEAR", 0.2),
        main_leg(2.72, "RIGHT MAIN GEAR", -0.2),
    ],
    hull: Hull { k: 1.4e6, c: 1.8e5, nf_max: 3.0e6, drag: 5.5e4 },
    hole: Hole { fus: [1.34, 0.95, 0.0, 0.5], wing: [W_Z0, 0.065, 0.22, 0.02], stab: [0.2, 0.05, 2.7], fin: [0.88, 2.07, 0.08, 0.03], pods: [[0.0; 3]; 2] },
    // 13,757 kg empty, two crew and half its fuel: 19.8 t with the tanks full
    // (the book's 18.8 t gross is with less than that aboard; maximum 28.0 t).
    mass: 16_900.0,
    // 5,879 kg inside; two J79s: about 0.3 kg/s idling, 2.5 at military power, 8.8 in full afterburner
    fuel_kg: 5_879.0,
    fuel_flow: [0.28, 2.5, 8.8],
    // (Pilot and weapon systems officer.)
    payload: [200.0, 200.0, 0.0],
    mac: 4.82,
    cg: [0.30, 0.24, 0.36],
    fuel_arm: 0.3,
    // Pitch, yaw, roll: an estimate (about what is published for an F-4 near 17.7 t: 122,200, 139,800 and 25,000 slug ft²).
    inertia: DVec3::new(165.7e3, 189.5e3, 33.9e3),
    wing_area: 49.2,
    b: 11.7,
    // A thin wing of aspect ratio 2.77; cd0 and the induced drag give the book's best lift to drag of 8.6.
    cl0: 0.02,
    cl_a: 2.8,
    cd0: 0.0224,
    k_ind: 0.152,
    cd_gear: 0.02,
    cd_flap: [0.03, 0.05],
    // (Blown flaps.)
    flap_cl: 0.45,
    // It departs early: buffet, then wing rock and a nose slice not far past 20 degrees.
    stall_alpha: 0.36,
    stall_flap: 0.04,
    stall_slat: 0.05,
    alpha_trim: 0.30,
    mcrit: 0.85,
    cd_wave: 0.034,
    spoiler_cd: 0.035,
    spoiler_cl: 0.0,
    chute: None,
    // Flaps: up, half, full.
    flaps: &[0.0, 30.0, 60.0],
    vfe: &[1e9, 250.0 * KT, 210.0 * KT],
    flap_rate: 0.3,
    slats_at: 1,
    lever: None,
    gear_rate: 1.0 / 6.0,
    // 750 kt, Mach 2.23.
    vne: 750.0 * KT,
    mmo: 2.23,
    flutter_v: 820.0 * KT,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    g_limit: 8.5,
    g_break: 12.0,
    fbw_g: 0.0,
    steer_max: 0.9,
    steer_fade: 15.0,
    // Two J79-GE-17As: 52.96 kN each dry, 79.38 kN in afterburner (Space). The
    // lapse and the wave drag are set against each other so that it stays just
    // subsonic dry down low, and the ram air takes it past Mach 2 at 40,000 ft
    // in afterburner (by the book 2.23; it takes minutes and most of the fuel).
    engine: Engine::Fan(Fan { n: 2, thrust: 52_960.0, idle: 0.65, idle_thrust: 0.05, n1_rpm: 7_460.0, spool_up: 0.7, spool_down: 0.6, reverse: 0.0, reverser_time: 1.0, boost: 1.5, mach_lapse: [0.4085, 0.6885], nozzle_z: 4.8 }),
    // Heavier on the controls than the Hornet and less forgiving: the stick
    // moves the slab directly, the ailerons drag the nose the wrong way at
    // a high angle of attack, and a stalled wing goes with any yaw.
    handling: Handling {
        q_ref: 9_000.0,
        a_max: 3.2,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 3.2,
        pitch_k: 5.0,
        pitch_k_lost: 2.0,
        pitch_ctl: 0.15,
        alpha_ctl: 0.24,
        pitch_d: 4.5,
        roll_ctl: 4.0,
        roll_d: 2.8,
        roll_beta: 0.5,
        roll_clean: 1.0,
        yaw_ctl: 0.7,
        yaw_beta: 2.4,
        yaw_d: 2.0,
        dihedral: 0.06,
        adverse: 0.35,
        stall_pitch: 0.5,
        stall_drop: 1.2,
        stall_yaw: 1.8,
    },
    cruise: 220.0,
    approach: (77.0, 2),
    cam_dist: 28.0,
    shadow_r: 12.0,
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
            // (The plume trails 12 m behind the nozzles: not part of the airframe.)
            if p.name != "afterburner" {
                for v in &p.mesh.v {
                    lo = lo.min(Vec3::from(v.pos));
                    hi = hi.max(Vec3::from(v.pos));
                }
            }
        }
        // 19.2 m long, 11.7 m across, 5.0 m tall on its wheels.
        let (len, span, top) = (hi.z - lo.z, hi.x - lo.x, hi.y + 2.05);
        println!("{} pieces, length {len:.2} m, span {span:.2} m, height {top:.2} m", m.pieces.len());
        assert!((len - 19.2).abs() < 0.3, "{len}");
        assert!((span - 11.71).abs() < 0.3, "{span}");
        assert!((top - 5.0).abs() < 0.3, "{top}");
        // The wing in the flight and damage models is the one drawn.
        let tip = wing_sect(W_X1);
        assert!((tip.le.y - -0.40).abs() < 0.03 && (tip.le.z - 2.87).abs() < 0.05, "{:?}", tip.le);
    }

    /// The stabilator is one slab on a level shaft: its halves pitch as one,
    /// never sideways, and only the stick's fore and aft moves them; the
    /// rudder alone answers the pedals, its trailing edge going to the right
    /// for right rudder.
    #[test]
    fn tail_moves_as_one() {
        use glam::DVec3;
        let m = build();
        let at = |name: &str| m.pieces.iter().position(|p| p.name == name).unwrap();
        // where a piece's aftmost vertex goes for a stick position, and how far any vertex goes sideways
        let moved = |name: &str, stick: DVec3| {
            let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
            p.stick = DVec3::ZERO;
            let rest = p.pose(&m.pieces);
            p.stick = stick;
            let pose = p.pose(&m.pieces);
            let k = at(name);
            let (mut te, mut side) = (Vec3::splat(-1e9), 0.0f32);
            for v in &m.pieces[k].mesh.v {
                let a = Vec3::from(v.pos);
                if a.z > te.z {
                    te = a;
                }
                side = side.max((pose[k].transform_point3(a).x - rest[k].transform_point3(a).x).abs());
            }
            (pose[k].transform_point3(te) - rest[k].transform_point3(te), side)
        };
        for stick in [DVec3::X, -DVec3::X] {
            let ((l, ls), (r, rs)) = (moved("left stabilator", stick), moved("right stabilator", stick));
            println!("stick {:+.0}: stabilator tips move {l:.3?} / {r:.3?}", stick.x);
            assert!(ls < 1e-3 && rs < 1e-3, "the halves scissor: {ls} {rs}");
            assert!((l.y - r.y).abs() < 1e-3 && (l.z - r.z).abs() < 1e-3, "the halves part: {l:?} {r:?}");
            // stick forward: trailing edge down
            assert!(l.y * stick.x as f32 <= -0.2, "{l:?}");
        }
        for stick in [DVec3::Y, DVec3::Z] {
            for name in ["left stabilator", "right stabilator"] {
                assert!(moved(name, stick).0.length() < 1e-4, "{name} answers {stick:?}");
            }
        }
        assert!(moved("rudder", DVec3::Y).0.x > 0.15 && moved("rudder", -DVec3::Y).0.x < -0.15);
        assert!(moved("rudder", DVec3::X).0.length() < 1e-4 && moved("rudder", DVec3::Z).0.length() < 1e-4);
    }

    #[test]
    fn gear_stows_inside() {
        let m = build();
        let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        p.gear = 0.0;
        let pose = p.pose(&m.pieces);
        for (i, pc) in m.pieces.iter().enumerate().filter(|(_, pc)| pc.name.ends_with("wheel")) {
            let c = pose[i].transform_point3(pc.pivot);
            println!("{} stowed at {c:?}", pc.name);
            if pc.name == "nose wheel" {
                // in its well under the front cockpit
                let (top, _, bot) = fuse(c.z);
                assert!(c.y > bot + 0.23 && c.y < top && c.x.abs() < 0.1, "{} sticks out: {c:?}", pc.name);
            } else {
                // lying flat in the wing, under the bulge in its upper skin
                assert!((c.y - W_Z0).abs() < 0.12 && c.x.abs() > 1.5 && c.x.abs() < 2.4, "{} sticks out: {c:?}", pc.name);
            }
        }
    }
}
