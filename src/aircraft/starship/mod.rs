//! SpaceX Starship: the Super Heavy booster with the Starship upper stage on
//! top, a two-stage rocket of stainless steel burning methane and oxygen.
//!
//! It is flown by plane/rocket.rs, not the aeroplane's flight model: it
//! stands on the pad on its skirt, climbs on whichever group of the booster's
//! 33 Raptors is lit (`Plane::next_engines`), drops the booster
//! (`Plane::stage_sep`) and flies on as the ship alone on its six.
//!
//! The model is made in Blender (`assets/blender/starship.py`, which also
//! saves `starship.blend`) and exported to `assets/starship.bin.gz`, one mesh
//! per piece as for the fighters (see fa18/mod.rs). Rebuild it with
//!
//!   blender -b --factory-startup --python assets/blender/starship.py -- assets
//!
//! It has no paint picture: bare steel, black heat-shield tiles on the ship's
//! windward side. There is nobody aboard and no cockpit.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward (the nose, so up the
//! stack), the origin 48 m above the booster's engines; the ship's tiles face
//! -Y, its bare steel side +Y.
use super::*;
use crate::airframe::*;
use std::collections::HashMap;

/// Stations (body z): the nose, where the ship's skirt sits on the booster's
/// hot-staging ring (and its engines end), the booster's engines' exit.
const NOSE: f32 = -72.95;
const JOIN: f32 = -23.0;
const BASE: f32 = 48.0;
const R: f32 = 4.5;

/// The body's radius at station z (the ogive nose ahead of the barrel).
fn radius(z: f32) -> f32 {
    if z < NOSE || z > BASE {
        0.0
    } else if z < -55.0 {
        ((38.25f32 * 38.25 - (-z - 55.0).powi(2)).max(0.0).sqrt() - 33.75).max(0.0)
    } else {
        R
    }
}
fn fuse(z: f32) -> (f32, f32, f32) {
    let r = radius(z);
    (r, 0.0, -r)
}
/// (It has no wing; the ship's aft flap stands in where the game asks for one.)
fn wing_sect(x: f32) -> Sect {
    Sect { le: Vec3::new(x.max(0.0), 0.0, -36.0), c: 12.0, cdir: Vec3::Z, up: Vec3::Y, tf: 1.0 }
}
/// The camera a real one carries: high on the ship's steel side, looking up the nose.
const EYE: Vec3 = Vec3::new(0.0, 5.2, -50.0);
fn sees_inside(_eye: Vec3, _d: Vec3) -> bool {
    false
}

/// Every piece: name (as in the .blend) and breakable group. The booster's
/// are the "tail": staging lets that group go as one body.
const PIECES: &[(&str, usize)] = &[
    ("booster body", G_TAIL),
    ("booster centre engines", G_TAIL),
    ("booster outer engines", G_TAIL),
    ("booster grid fin 1", G_TAIL),
    ("booster grid fin 2", G_TAIL),
    ("booster grid fin 3", G_TAIL),
    ("booster grid fin 4", G_TAIL),
    ("booster hot stage ring", G_TAIL),
    ("ship body", G_FUSE),
    ("ship right forward flap", G_FUSE),
    ("ship left forward flap", G_FUSE),
    ("ship right aft flap", G_FUSE),
    ("ship left aft flap", G_FUSE),
    ("ship sea level engines", G_FUSE),
    ("ship vacuum engines", G_FUSE),
    // The exhaust plumes: drawn after everything else, in the transparent pass.
    ("booster plume", G_TAIL),
    ("ship plume", G_FUSE),
];

/// How a piece moves: the gimballed engine clusters swing with the pitch
/// control, the grid fins twist with roll, and the ship's flaps (modelled
/// flat; a positive angle folds one toward the steel side) fold and unfold
/// against each other, fore against aft for pitch and left against right for
/// roll, as rocket.rs has them work (it puts what they do in the "stick").
fn anim(name: &str) -> Anim {
    let s = if name.contains("left") { -1.0 } else { 1.0 };
    if name == "booster centre engines" || name == "ship sea level engines" {
        Anim::hinge(Drive::new(Input::Pitch, -0.15))
    } else if name.contains("grid fin") {
        Anim::hinge(Drive::new(Input::Roll, 0.35))
    } else if name.ends_with("forward flap") {
        Anim::hinge2(Drive::new(Input::Pitch, -0.45).offset(0.5), Drive::new(Input::Aileron(s), -0.25))
    } else if name.ends_with("aft flap") {
        Anim::hinge2(Drive::new(Input::Pitch, 0.45).offset(0.5), Drive::new(Input::Aileron(s), -0.25))
    } else {
        Anim::Fixed
    }
}

pub fn build() -> Model {
    let objs = fa18::load(include_bytes!("../../../assets/starship.bin.gz"), "starship");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("starship model has no piece called {name:?}"));
        let k = b.add(name, group, o.mesh.clone(), anim(name), o.pivot, o.axis, None);
        if name.ends_with("plume") {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.3, reach: 12.0, rays: 20, open: &|_, _| false });
    Model::finish(b.pieces)
}

// ---------------------------------------------------------------- the type

/// Block 1/2 as flown. The booster: 275 t empty, 3,400 t of propellant, 33
/// Raptor 2s making 73.5 MN together (327 s at sea level, 347 s in a vacuum):
/// 13 in the middle gimbal, 20 round the rim are fixed. The ship: about 100 t
/// empty, 1,200 t of propellant, three sea-level Raptors that gimbal and
/// three vacuum ones that do not, 12.25 MN (327 s / 380 s).
/// Estimates: where the air's side force acts, what the grid fins and flaps
/// can push with, and the loads that break it.
const ROCKET: Rocket = Rocket {
    stages: [
        Stage {
            name: "SUPER HEAVY",
            dry: 275_000.0,
            prop: 3_400_000.0,
            thrust: 73.5e6,
            n: 33,
            isp: [327.0, 347.0],
            groups: &[("ALL 33", 33, 13), ("INNER 13", 13, 13), ("CENTRE 3", 3, 3)],
            z: [JOIN as f64, BASE as f64],
            cp: 0.53,
            fins: 40.0,
            q_alpha: 9.0e3,
        },
        Stage {
            name: "STARSHIP",
            dry: 100_000.0,
            prop: 1_200_000.0,
            thrust: 12.25e6,
            n: 6,
            isp: [327.0, 380.0],
            groups: &[("ALL 6", 6, 3), ("SEA LEVEL 3", 3, 3)],
            z: [NOSE as f64, -23.2],
            cp: 0.60,
            fins: 140.0,
            q_alpha: 9.0e4,
        },
    ],
    radius: R as f64,
    throttle_min: 0.4,
    gimbal: 0.26,
    flaps: [[21.3, 5.6, -62.6], [41.4, 6.45, -29.0]],
};

pub static SPEC: Spec = Spec {
    id: 5,
    name: "SpaceX Starship",
    keys: &["starship", "spacex", "rocket"],
    blurb: "Two-stage rocket, 121 m and 5,000 t on the pad. Throttle up to lift off on 33 engines, then hold W a few seconds to tip it over. 1 picks which engines burn, Backspace lights the ship and drops the booster, 2 has the ship land itself.",
    still: include_bytes!("../../../assets/aircraft/starship.png"),
    paint: None,
    build,
    groups: [("ship", 100_000.0), ("booster", 275_000.0), ("left flaps", 4_000.0), ("right flaps", 4_000.0), ("ship engines", 10_000.0), ("nose", 3_000.0), ("skirt", 2_000.0)],
    // (Everything a wing, a tail or wheels would be is a stand-in the rocket's own flight model never reads.)
    span: 9.0,
    wing_sect,
    fuse,
    aileron: [4.5, 9.0],
    tank: [0.0, 4.5],
    tail: Tail {
        fin_y0: 0.0,
        fin_h: 4.5,
        rudder_y0: 0.0,
        rudder_h: 4.5,
        stab_root: 4.5,
        elev_root: 4.5,
        break_z: [-23.0, -22.0],
        fin_base: DVec3::new(0.0, 4.5, -19.0),
        fin_tip: DVec3::new(0.0, 8.2, -19.0),
        stab_tip: DVec3::new(8.2, 0.0, -19.0),
    },
    eye: EYE,
    eye2: None,
    sees_inside,
    contacts: &[([0.0, 0.0, -72.95], Part::Nose), ([0.0, -4.5, -40.0], Part::Belly), ([0.0, 4.5, -40.0], Part::Roof), ([-4.5, 0.0, 0.0], Part::Side), ([4.5, 0.0, 0.0], Part::Side)],
    tail_pts: &[[0.0, -4.5, 48.0], [0.0, 4.5, 48.0], [-4.5, 0.0, 48.0], [4.5, 0.0, 48.0]],
    wing_st: [0.5, 0.6, 0.7, 0.8, 0.9],
    hard: [
        DVec3::new(0.0, 0.0, -72.95),
        DVec3::new(-9.0, 0.0, -30.0),
        DVec3::new(9.0, 0.0, -30.0),
        DVec3::new(0.0, 0.0, 48.0),
        DVec3::new(0.0, 8.2, -19.0),
        DVec3::new(0.0, -4.5, 0.0),
        DVec3::new(0.0, -4.5, 48.0),
    ],
    gear: &[],
    hull: Hull { k: 5.0e7, c: 6.0e6, nf_max: 2.0e8, drag: 2.0e6 },
    hole: Hole { fus: [4.5, 4.5, 0.0, 4.5], wing: [0.0, 0.0, 0.2, 0.2], stab: [0.0, 0.2, 9.0], fin: [0.0, 0.0, 0.0, 0.0], pods: [[0.0; 3]; 2] },
    // On the pad, full: 275 t and 100 t of steel, 3,400 t and 1,200 t of propellant (rocket.rs works out what it weighs as that burns).
    mass: 4_975_000.0,
    // The booster's propellant; all 33 engines at full thrust burn 22.9 t a second.
    fuel_kg: 3_400_000.0,
    fuel_flow: [0.0, 22_900.0, 22_900.0],
    payload: [0.0, 0.0, 0.0],
    mac: 9.0,
    cg: [0.5, 0.0, 1.0],
    fuel_arm: 0.0,
    inertia: DVec3::new(4.2e9, 4.2e9, 5.0e7),
    wing_area: 63.6,
    b: 9.0,
    cl0: 0.0,
    cl_a: 2.0,
    cd0: 0.3,
    k_ind: 0.0,
    cd_gear: 0.0,
    cd_flap: [0.0, 0.0],
    flap_cl: 0.0,
    stall_alpha: 1.5,
    stall_flap: 0.0,
    stall_slat: 0.0,
    alpha_trim: 0.0,
    mcrit: 0.0,
    cd_wave: 0.0,
    spoiler_cd: 0.0,
    spoiler_cl: 0.0,
    chute: None,
    flaps: &[0.0],
    vfe: &[1e9],
    flap_rate: 1.0,
    slats_at: 0,
    lever: None,
    gear_rate: 1.0,
    vne: 1e9,
    mmo: 0.0,
    flutter_v: 1e9,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    g_limit: 6.0,
    g_break: 9.0,
    fbw_g: 0.0,
    steer_max: 0.0,
    steer_fade: 0.0,
    engine: Engine::Rocket(ROCKET),
    handling: Handling {
        q_ref: 8_000.0,
        a_max: 1.0,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 1.0,
        pitch_k: 0.0,
        pitch_k_lost: 0.0,
        pitch_ctl: 0.0,
        alpha_ctl: 0.0,
        pitch_d: 0.0,
        roll_ctl: 0.0,
        roll_d: 0.0,
        roll_beta: 0.0,
        roll_clean: 1.0,
        yaw_ctl: 0.0,
        yaw_beta: 0.0,
        yaw_d: 0.0,
        dihedral: 0.0,
        adverse: 0.0,
        stall_pitch: 0.0,
        stall_drop: 0.0,
        stall_yaw: 0.0,
    },
    // An air start: already climbing at 100 m/s.
    cruise: 100.0,
    approach: (100.0, 0),
    cam_dist: 230.0,
    shadow_r: 70.0,
    sound: Sound::Rocket,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builds_whole() {
        let m = build();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in m.pieces.iter().filter(|p| !p.glass) {
            assert!(p.mesh.v.iter().all(|v| v.pos.iter().chain(v.nrm.iter()).all(|x| x.is_finite())), "{} has NaNs", p.name);
            assert!(!p.mesh.i.is_empty(), "{} is empty", p.name);
            for v in &p.mesh.v {
                lo = lo.min(Vec3::from(v.pos));
                hi = hi.max(Vec3::from(v.pos));
            }
        }
        // 121 m from the engines' bells to the tip of the nose, 9 m across the barrel (18 m across the aft flaps).
        println!("{} pieces, {:?} .. {:?}", m.pieces.len(), lo, hi);
        assert!((hi.z - BASE).abs() < 0.3 && (lo.z - NOSE).abs() < 0.3, "{lo:?} {hi:?}");
        assert!((hi.x - 9.0).abs() < 0.3 && (lo.x + 9.0).abs() < 0.3, "{lo:?} {hi:?}");
        // Staging drops the booster's pieces and none of the ship's.
        for p in &m.pieces {
            assert_eq!(p.group == G_TAIL, p.name.starts_with("booster"), "{}", p.name);
        }
    }
}
