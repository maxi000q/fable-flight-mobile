//! Lockheed SR-71A Blackbird: a two-seat, twin-engine Mach 3 reconnaissance aircraft.
//!
//! Like the Hornet, the airframe is modelled in Blender
//! (`assets/blender/sr71.py`, which also saves `sr71.blend`) and exported to
//! `assets/sr71.bin.gz`: one mesh per piece, with the hinge each one turns
//! about. How the pieces move, what they belong to and what they are to the
//! damage model is set here, by name. Rebuild the asset with
//!
//!   blender -b --factory-startup --python assets/blender/sr71.py -- assets
//!
//! The cockpits inside are built here in Rust (`pit.rs`).
//!
//! Its paint is a picture (`assets/sr71_paint.jpg`, see
//! `assets/blender/sr71_paint.py`), projected onto the skin by plane.wgsl as
//! the Hornet's is; so are its exhaust plumes (the "afterburner" piece, a
//! long one behind each nacelle) and its engines' sound (audio.rs `Fighter`,
//! as turbojets).
//!
//! The figures are the SR-71A's: 32.74 m long, 16.94 m across, 5.64 m tall, a
//! 170 m² wing, 30,617 kg empty, 46,255 l of JP-7, two J58s of 25,000 lbf,
//! 34,000 lbf in the afterburner it cruises in, Mach 3.2 at 80,000 ft.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, origin at the centre
//! of gravity. The model's own origin is 3.3 m ahead of that (18 m behind the
//! tip of the pitot boom): its pieces are moved forward by as much as they
//! are loaded (`CG`), and stations here are given as the model's (`st`).
use super::*;
use crate::airframe::*;
use std::collections::HashMap;

mod pit;

const KT: f64 = 1.0 / 1.94384;

/// How far behind the model's origin the centre of gravity is (m): a quarter
/// of the way down the wing's mean chord, just ahead of the main wheels.
const CG: f64 = 3.3;
/// Where a station of the model (m ahead of its origin, as sr71.py has it) is in the body frame.
const fn st(y: f64) -> f64 {
    -y - CG
}

// ---------------------------------------------------------------- geometry (in step with sr71.py)

/// Fuselage stations: y forward (Blender frame), half width out to the
/// chine's edge, top, height of the chine, bottom, half width of the round
/// body inside the chines. (The section: the body an ellipse, the chine a
/// blade thinning to its edge, joined as the cube root of the sum of their
/// cubes: sr71.py `section_h`.)
const FUSE: [[f32; 6]; 22] = [
    [16.70, 0.00, -0.10, -0.10, -0.10, 0.00],
    [16.40, 0.10, -0.02, -0.10, -0.17, 0.07],
    [15.80, 0.24, 0.10, -0.10, -0.27, 0.17],
    [15.00, 0.40, 0.24, -0.10, -0.38, 0.28],
    [14.00, 0.58, 0.38, -0.10, -0.48, 0.40],
    [13.00, 0.74, 0.50, -0.10, -0.56, 0.50],
    [12.00, 0.88, 0.60, -0.10, -0.63, 0.58],
    [11.00, 0.98, 0.68, -0.10, -0.69, 0.64],
    [10.00, 1.06, 0.74, -0.10, -0.74, 0.70],
    [9.00, 1.14, 0.78, -0.10, -0.78, 0.75],
    [8.00, 1.22, 0.80, -0.10, -0.80, 0.79],
    [6.50, 1.36, 0.81, -0.10, -0.81, 0.81],
    [5.00, 1.52, 0.81, -0.10, -0.81, 0.81],
    [3.20, 1.66, 0.81, -0.10, -0.81, 0.81],
    [1.20, 1.72, 0.81, -0.10, -0.81, 0.81],
    [-2.00, 1.72, 0.81, -0.10, -0.83, 0.86],
    [-4.20, 1.72, 0.82, -0.10, -0.86, 0.90],
    [-6.40, 1.72, 0.81, -0.10, -0.83, 0.86],
    [-9.00, 1.68, 0.72, -0.10, -0.70, 0.76],
    [-11.00, 1.50, 0.54, -0.10, -0.48, 0.58],
    [-13.00, 0.90, 0.28, -0.10, -0.28, 0.32],
    [-14.74, 0.04, -0.07, -0.10, -0.13, 0.02],
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

/// Fuselage at body station z: top, the height of its widest point (the chine), bottom.
fn fuse(z: f32) -> (f32, f32, f32) {
    let r = fuse_at(-z - CG as f32);
    (r[2], r[3], r[4])
}

// The wing, a 60 degree delta lying flat: the tip's span station, the leading
// edge on the centreline and its sweep (m back per m out), the trailing edge
// (straight across out to the kink just outside the nacelle, then swept
// forward to the tip), its height.
const W_X1: f32 = 8.47;
const W_LE0: f32 = 4.2;
const W_SWEEP: f32 = 1.732;
const W_TE0: f32 = -11.6;
const W_TE1: f32 = -10.9;
const W_KINK: f32 = 4.95;
const W_Z: f32 = -0.10;

pub fn wing_sect(x: f32) -> Sect {
    let x = x.clamp(0.0, W_X1);
    let te = if x <= W_KINK { W_TE0 } else { W_TE0 + (W_TE1 - W_TE0) * (x - W_KINK) / (W_X1 - W_KINK) };
    let le = W_LE0 - W_SWEEP * x;
    Sect { le: Vec3::new(x, W_Z, st(le as f64) as f32), c: le - te, cdir: Vec3::Z, up: Vec3::Y, tf: 1.0 }
}

/// The engines' centrelines: how far out, and their height.
const ENGINE_X: f32 = 4.12;
const ENGINE_Y: f32 = -0.10;

/// The pilot's eye, in the front cockpit.
const EYE: Vec3 = Vec3::new(0.0, 0.95, st(11.0) as f32);

// ---------------------------------------------------------------- the model

/// Every piece: name (as in the .blend), breakable group, what it is to the
/// damage model, and the piece it hangs from (listed before it). The
/// nacelles, with their spikes, nozzles and fins, go with the wings they
/// stand on.
const PIECES: &[(&str, usize, Role, Option<&str>)] = &[
    ("radome", G_PROP, Role::Other, None),
    ("nose section", G_ENGINE, Role::Other, None),
    ("fuselage", G_FUSE, Role::Other, None),
    ("tail cone", G_TAIL, Role::TailCone, None),
    ("canopy", G_ENGINE, Role::Other, None),
    ("canopy frame", G_ENGINE, Role::Other, None),
    ("left wing", G_LWING, Role::Other, None),
    ("right wing", G_RWING, Role::Other, None),
    ("left nacelle", G_LWING, Role::Other, None),
    ("right nacelle", G_RWING, Role::Other, None),
    ("left spike", G_LWING, Role::Other, None),
    ("right spike", G_RWING, Role::Other, None),
    ("left nozzle", G_LWING, Role::Other, None),
    ("right nozzle", G_RWING, Role::Other, None),
    ("left inner elevon", G_LWING, Role::Elevator, None),
    ("right inner elevon", G_RWING, Role::Elevator, None),
    ("left outer elevon", G_LWING, Role::Aileron(0), None),
    ("right outer elevon", G_RWING, Role::Aileron(1), None),
    // (all-moving: the fins are its rudders. As `Role::Stab` they went with a
    // broken "stabiliser", which here is an inner elevon scraped off.)
    ("left fin", G_LWING, Role::Rudder, None),
    ("right fin", G_RWING, Role::Rudder, None),
    ("left nav light", G_LWING, Role::Other, None),
    ("right nav light", G_RWING, Role::Other, None),
    ("tail light", G_TAIL, Role::Other, None),
    ("nose leg", G_GEAR, Role::Gear(0), None),
    ("nose fork", G_GEAR, Role::Gear(0), Some("nose leg")),
    ("nose wheel", G_GEAR, Role::Gear(0), Some("nose fork")),
    ("left main leg", G_GEAR, Role::Gear(1), None),
    ("left main wheel", G_GEAR, Role::Gear(1), Some("left main leg")),
    ("left strut door", G_GEAR, Role::Gear(1), Some("left main leg")),
    ("right main leg", G_GEAR, Role::Gear(2), None),
    ("right main wheel", G_GEAR, Role::Gear(2), Some("right main leg")),
    ("right strut door", G_GEAR, Role::Gear(2), Some("right main leg")),
    ("left nose door", G_ENGINE, Role::Other, None),
    ("right nose door", G_ENGINE, Role::Other, None),
    ("left main door", G_FUSE, Role::Other, None),
    ("right main door", G_FUSE, Role::Other, None),
    // The exhaust plumes: drawn after everything else, in the transparent pass.
    ("afterburner", G_FUSE, Role::Other, None),
    // The drag chute, streamed: drawn only while it is out (added after the others, see `build`).
    ("drag chute", G_TAIL, Role::Other, None),
];

/// How a piece moves. Hinges point so that a positive angle puts a trailing
/// edge down (the fins: to the right); the gear is modelled down and locked,
/// its doors open. A piece's `angle` is how far it turns to stow: the nose
/// leg swings forward, the main legs inward, the doors shut. It has no flaps
/// and no speedbrake (it lands fast and stops with a parachute, a piece drawn
/// only while it is out): four elevons do for pitch and roll, and the whole
/// fins turn as rudders. The inlet spikes draw back into the nacelles as it
/// goes faster.
fn anim(name: &str, stow: f32) -> Anim {
    let left = name.starts_with("left");
    let s = if left { -1.0 } else { 1.0 };
    let leg = if name.starts_with("nose") { 0 } else if left { 1 } else { 2 };
    if name.ends_with("elevon") {
        // all four for pitch, each side against the other for roll
        Anim::hinge2(Drive::new(Input::Elevator, 0.35), Drive::new(Input::Aileron(s), 0.26))
    } else if name.ends_with(" fin") {
        // all-moving, both the same way
        Anim::hinge(Drive::new(Input::Rudder, 0.35))
    } else if name == "drag chute" {
        // streams out aft from its bay as it is let go
        Anim::slide(Drive::new(Input::Spoiler(0.0), 15.0).offset(-15.0), Vec3::Z)
    } else if name.ends_with("spike") {
        // forward below Mach 1.6, 66 cm back by Mach 3.2
        Anim::slide(Drive::new(Input::Mach, 0.66).smooth(1.6, 3.2), Vec3::Z)
    } else if name.ends_with(" leg") {
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow))
    } else if name.ends_with("strut door") {
        // rigid with its leg
        Anim::Fixed
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
    let objs = fa18::load(include_bytes!("../../../assets/sr71.bin.gz"), "sr71");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group, role, parent) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("sr71 model has no piece called {name:?}"));
        // (from the model's origin to the centre of gravity)
        let mut mesh = o.mesh.clone();
        mesh.v.iter_mut().for_each(|v| v.pos[2] -= CG as f32);
        let k = b.add(name, group, mesh, anim(name, o.angle), o.pivot - Vec3::Z * CG as f32, o.axis, parent.map(|p| at[p]));
        b.role(k, role);
        if name == "canopy" || name == "afterburner" {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    // (The chute is no part of the airframe: it takes no part in the bake, and casts no shade on it.)
    let chute = b.pieces.pop().filter(|p| p.name == "drag chute").expect("the drag chute comes last");
    bake_ao(&mut b.pieces, &AoBake { vox: 0.08, reach: 4.0, rays: 24, open: &|_, _| false });
    b.pieces.push(Piece { vis: Vis::Chute, occluder: false, ..chute });
    // The cockpits get their own, much finer bake, shut in by the nose's skin
    // and the canopy's frame; the light comes in through the glass.
    let mut c = Builder::default();
    pit::build(&mut c);
    let n_pit = c.pieces.len();
    if n_pit > 0 {
        for name in ["nose section", "canopy frame"] {
            if let Some(k) = b.pieces.iter().position(|p| p.name == name) {
                let mut skin = b.pieces[k].mesh.clone();
                skin.i = skin.i.chunks(3).filter(|t| t.iter().all(|&i| { let z = skin.v[i as usize].pos[2]; z > st(12.6) as f32 && z < st(8.3) as f32 })).flatten().copied().collect();
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

// (The gear's springs and dampers are estimates: the Raptor's, stiffened for
// twice the weight. Three tyres abreast on each main leg.)
const fn main_leg(x: f64, name: &'static str, kick: f64) -> Leg {
    Leg {
        at: DVec3::new(x, -2.30, st(-4.20)),
        wheel_r: 0.35,
        travel: 0.35,
        k: 1.8e6,
        c: 2.2e5,
        nf_max: 5.0e6,
        steer: 0.0,
        brakes: true,
        side_k: 4.5e5,
        side_mu: 0.8,
        roll_k: 6.0e3,
        roll_mu: 0.02,
        brake_k: 1.5e5,
        // (small wheels and a lot of energy to lose: the brakes are good for a quarter of a g or so, the parachute for the rest)
        brake_mu: 0.3,
        fold_vn: 3.5,
        fold_kick: DVec3::new(0.3, 0.0, kick),
        name,
    }
}

pub static SPEC: Spec = Spec {
    id: 7,
    name: "SR-71A Blackbird",
    keys: &["sr71", "sr-71", "sr71a", "sr-71a", "blackbird", "habu"],
    blurb: "Two-seat Mach 3 reconnaissance aircraft, 50 t. Hold Space: it cruises in afterburner, Mach 3.2 at 80,000 ft. Keep under 450 kt indicated on the way up and 3 g always. No flaps; / streams the drag chute once it is down. 175 kt approach.",
    still: include_bytes!("../../../assets/aircraft/sr71.png"),
    paint: Some(Paint { jpg: include_bytes!("../../../assets/sr71_paint.jpg"), views: 4, span: 24.0, length: 36.0, centre: [1.63 + CG, 1.0], relief: 12.0, sheen: 0.3 }),
    build,
    groups: [("fuselage", 12_000.0), ("tail", 600.0), ("left wing", 7_500.0), ("right wing", 7_500.0), ("nose section", 1_800.0), ("radome", 400.0), ("gear", 1_800.0)],
    span: W_X1,
    wing_sect,
    fuse,
    // (the outer elevons)
    aileron: [5.02, 8.20],
    tank: [0.9, 3.2],
    tail: Tail {
        // (the fins stand on the nacelles, leaning in 15 degrees: heights are
        // measured straight up; all of a fin above its stub turns)
        fin_y0: 0.66,
        fin_h: 2.68,
        rudder_y0: 1.09,
        rudder_h: 2.25,
        // (it has no tailplane: the inner elevons are what a broken "stabiliser" takes)
        stab_root: 1.76,
        elev_root: 1.76,
        break_z: [st(-11.0) as f32, st(-12.5) as f32],
        fin_base: DVec3::new(4.12, 0.66, st(-8.9)),
        fin_tip: DVec3::new(3.40, 3.34, st(-10.3)),
        stab_tip: DVec3::new(3.22, -0.10, st(-11.6)),
    },
    eye: EYE,
    // (the reconnaissance systems officer's, 1.35 m behind)
    eye2: Some(Vec3::new(0.0, 0.95, st(9.65) as f32)),
    sees_inside: pit::sees_inside,
    contacts: &[
        ([0.0, -0.10, st(16.7)], Part::Nose),
        ([0.0, -0.38, st(15.0)], Part::Nose),
        ([0.0, 0.24, st(15.0)], Part::Nose),
        ([0.0, -0.69, st(11.0)], Part::Belly),
        ([0.0, -0.80, st(8.0)], Part::Belly),
        ([0.0, -0.81, st(3.2)], Part::Belly),
        ([0.0, -0.83, st(-2.0)], Part::Belly),
        ([0.0, -0.83, st(-6.4)], Part::Belly),
        // (the chines' edges)
        ([-0.98, -0.10, st(11.0)], Part::Side),
        ([0.98, -0.10, st(11.0)], Part::Side),
        ([-1.36, -0.10, st(6.5)], Part::Side),
        ([1.36, -0.10, st(6.5)], Part::Side),
        // under each nacelle, front and back
        ([-4.12, -0.98, st(0.3)], Part::Pod(0)),
        ([4.12, -0.98, st(0.3)], Part::Pod(1)),
        ([-4.12, -0.98, st(-9.5)], Part::Pod(0)),
        ([4.12, -0.98, st(-9.5)], Part::Pod(1)),
        ([0.0, 1.16, st(10.5)], Part::Roof),
        ([0.0, 0.81, st(5.0)], Part::Roof),
        ([0.0, 0.82, st(-4.2)], Part::Roof),
    ],
    tail_pts: &[[0.0, -0.48, st(-11.0)], [0.0, -0.28, st(-13.0)], [0.0, -0.13, st(-14.7)]],
    wing_st: [1.9 / W_X1, 3.0 / W_X1, 5.4 / W_X1, 6.8 / W_X1, 8.2 / W_X1],
    hard: [
        DVec3::new(0.0, -0.10, st(18.0)),
        DVec3::new(-8.47, -0.10, st(-10.7)),
        DVec3::new(8.47, -0.10, st(-10.7)),
        DVec3::new(0.0, -0.10, st(-14.74)),
        DVec3::new(3.40, 3.34, st(-10.3)),
        DVec3::new(0.0, -0.86, st(-4.2)),
        DVec3::new(4.12, -0.98, st(-3.0)),
    ],
    gear: &[
        Leg {
            at: DVec3::new(0.0, -2.30, st(7.45)),
            wheel_r: 0.24,
            travel: 0.30,
            k: 1.2e6,
            c: 1.3e5,
            nf_max: 2.4e6,
            steer: 1.0,
            brakes: false,
            side_k: 2.0e5,
            side_mu: 0.8,
            roll_k: 3.0e3,
            roll_mu: 0.02,
            brake_k: 0.0,
            brake_mu: 0.0,
            fold_vn: 3.0,
            fold_kick: DVec3::new(-0.2, 0.0, 0.0),
            name: "NOSE GEAR",
        },
        main_leg(-2.55, "LEFT MAIN GEAR", 0.2),
        main_leg(2.55, "RIGHT MAIN GEAR", -0.2),
    ],
    hull: Hull { k: 3.5e6, c: 4.5e5, nf_max: 8.0e6, drag: 1.4e5 },
    hole: Hole { fus: [0.90, 0.84, -0.02, 0.6], wing: [W_Z, 0.0, 0.18, 0.02], stab: [W_Z, 0.05, 1.7], fin: [0.66, 2.68, 0.09, 0.04], pods: [[ENGINE_X, ENGINE_Y, 0.88], [0.0; 3]] },
    // 30,617 kg empty, 1.6 t of crew and sensors and half its fuel: with the tanks full it is the book's 68.9 t gross (maximum 78.0 t).
    mass: 50_500.0,
    // 46,255 l of JP-7, about 36.5 t. Two J58s: about 0.5 kg/s idling
    // (an estimate), 3.6 at military power (0.8 lb of fuel an hour for each
    // of 18,000 lbf) and 12.2 in full afterburner at sea level (1.9 for each
    // of 25,500 lbf); at Mach 3.2 and 80,000 ft that comes to some 4.3 kg/s
    // (34,000 lb an hour).
    fuel_kg: 36_500.0,
    fuel_flow: [0.5, 3.6, 12.2],
    // (Pilot and reconnaissance systems officer.)
    payload: [200.0, 200.0, 0.0],
    // Balanced at 22% of its mean chord (the real one's ran from 17 to 25%, moved by pumping fuel).
    mac: 11.5,
    cg: [0.22, 0.17, 0.26],
    fuel_arm: 0.5,
    // Pitch, yaw, roll: estimates, from radii of gyration (0.17 of the length; 0.15 of the span, with an engine out on each wing).
    inertia: DVec3::new(1.6e6, 1.9e6, 3.4e5),
    wing_area: 170.0,
    b: 16.94,
    // A thin delta of aspect ratio 1.7, the chines' lift counted in: it flies
    // at about 10 degrees on approach and has little drag once supersonic
    // (lift over drag about 6.5 at its cruise).
    cl0: 0.0,
    cl_a: 2.5,
    cd0: 0.010,
    k_ind: 0.25,
    cd_gear: 0.012,
    cd_flap: [0.0, 0.0],
    flap_cl: 0.0,
    stall_alpha: 0.42,
    stall_flap: 0.0,
    stall_slat: 0.0,
    alpha_trim: 0.24,
    mcrit: 0.92,
    cd_wave: 0.008,
    // No speedbrake: it stops with a 40 ft ribbon parachute (the / key; half
    // its 117 m² counts as drag area), streamed below 210 kt once the main
    // wheels are down and let go at 55 kt.
    spoiler_cd: 0.0,
    spoiler_cl: 0.0,
    chute: Some(Chute { cda: 60.0, v_max: 210.0 * KT, v_min: 55.0 * KT }),
    // No flaps.
    flaps: &[0.0],
    vfe: &[1e9],
    flap_rate: 0.3,
    slats_at: 0,
    lever: None,
    gear_rate: 1.0 / 6.0,
    // 500 kt indicated (it climbs at 450), Mach 3.3.
    vne: 500.0 * KT,
    mmo: 3.3,
    flutter_v: 560.0 * KT,
    inlet_mach: 3.2,
    dry_mach: 2.6,
    // Built light for its size: 2.5 g by the book, less when heavy or fast.
    g_limit: 3.5,
    g_break: 6.0,
    fbw_g: 0.0,
    steer_max: 0.8,
    steer_fade: 15.0,
    // Two J58s (JT11D-20K) as installed: 18,000 lbf (80.1 kN) each at
    // military power, 25,500 lbf (113.4 kN) in afterburner (Space). (The
    // 34,000 lbf often quoted, and the book's thrust over weight of 0.44, are
    // the bare engine's: the installed figures are what give the book's
    // take-off run and rate of climb.) The faster it goes the more of its
    // afterburning thrust is the ram air through the inlets and round the
    // engine: the lapse gives it a quarter more thrust than drag at Mach 3.2
    // and 80,000 ft in full afterburner, and `inlet_mach` holds it there (it
    // settles near Mach 3.2 to 3.25, under its limit). Without the
    // afterburners there is none of that (`dry_mach`): military power will
    // not keep it supersonic, and at Mach 3 it is next to nothing.
    engine: Engine::Fan(Fan { n: 2, thrust: 80_070.0, idle: 0.65, idle_thrust: 0.05, n1_rpm: 7_400.0, spool_up: 0.6, spool_down: 0.5, reverse: 0.0, reverser_time: 1.0, boost: 1.4167, mach_lapse: [0.25, 0.6064], nozzle_z: 12.4 - CG }),
    // Estimates: a big, heavy delta flown through stability augmentation,
    // slow to answer in pitch and roll, with little stick to pull g with.
    handling: Handling {
        // (Its stability augmentation: the same authority and the same damping
        // from approach speed to Mach 3, where the thin air alone would damp
        // next to nothing. About 28 degrees a second of roll at full stick.)
        q_ref: 6_000.0,
        a_max: 0.85,
        damp_v: 68.0,
        ad_min: 0.8,
        ad_max: 2.2,
        pitch_k: 4.0,
        pitch_k_lost: 2.0,
        pitch_ctl: 0.10,
        alpha_ctl: 0.11,
        pitch_d: 4.5,
        roll_ctl: 1.4,
        roll_d: 3.0,
        roll_beta: 0.4,
        roll_clean: 1.0,
        yaw_ctl: 0.5,
        yaw_beta: 2.6,
        yaw_d: 2.2,
        dihedral: 0.05,
        adverse: 0.12,
        stall_pitch: 0.4,
        stall_drop: 0.6,
        stall_yaw: 1.2,
    },
    cruise: 200.0,
    approach: (90.0, 0),
    cam_dist: 46.0,
    shadow_r: 19.0,
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
            // (The plumes trail 30 m behind the nozzles: not part of the airframe.)
            if p.name != "afterburner" && p.name != "drag chute" {
                for v in &p.mesh.v {
                    lo = lo.min(Vec3::from(v.pos));
                    hi = hi.max(Vec3::from(v.pos));
                }
            }
        }
        // 32.74 m long with its pitot boom, 16.94 m across, 5.64 m tall on its wheels.
        let (len, span, top) = (hi.z - lo.z, hi.x - lo.x, hi.y + 2.30);
        println!("{} pieces, length {len:.2} m, span {span:.2} m, height {top:.2} m", m.pieces.len());
        assert!((len - 32.75).abs() < 0.3, "{len}");
        assert!((span - 16.95).abs() < 0.3, "{span}");
        assert!((top - 5.65).abs() < 0.3, "{top}");
        // The wing in the flight and damage models is the one drawn.
        let tip = wing_sect(W_X1);
        assert!((tip.le.y - -0.10).abs() < 0.03 && (tip.le.z - (10.47 - CG as f32)).abs() < 0.05 && (tip.c - 0.43).abs() < 0.03, "{:?} {}", tip.le, tip.c);
    }

    #[test]
    fn gear_stows_inside_the_fuselage() {
        let m = build();
        let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        p.gear = 0.0;
        let pose = p.pose(&m.pieces);
        for (i, pc) in m.pieces.iter().enumerate().filter(|(_, pc)| pc.name.ends_with("wheel")) {
            let c = pose[i].transform_point3(pc.pivot);
            let r = fuse_at(-c.z - CG as f32);
            println!("{} stowed at {c:?}", pc.name);
            assert!(c.y > r[4] && c.y < r[2] && c.x.abs() < r[1], "{} sticks out: {c:?}", pc.name);
        }
    }

    #[test]
    fn spikes_draw_back_with_mach() {
        let m = build();
        let k = m.pieces.iter().position(|p| p.name == "right spike").unwrap();
        let at = |mach: f64| {
            // (at 24 km, where it is flown at Mach 3)
            let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 24_000.0, 0.0, mach * 295.07);
            p.gear = 0.0;
            p.pose(&m.pieces)[k].transform_point3(m.pieces[k].pivot).z
        };
        let (slow, fast) = (at(1.2), at(3.2));
        println!("spike tip at z {slow:.2} below Mach 1.6, {fast:.2} at Mach 3.2");
        assert!((slow - st(4.10) as f32).abs() < 0.01 && (fast - slow - 0.66).abs() < 0.02, "{slow} {fast}");
    }
}
