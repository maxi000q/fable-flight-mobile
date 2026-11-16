//! The aircraft you can fly. Each type is one module with its model (built
//! with the toolkit in `airframe.rs`) and a `Spec`: everything the flight
//! model, the damage model, the landing gear, the engines, the camera and the
//! sound need to know about it. Adding a type is a new module and an entry in
//! `ALL`; nothing else in the game is specific to one aircraft except its
//! paint and surface detail, which `plane.wgsl` picks by `Spec::id`.
//!
//! Body frame (as in airframe.rs): +X right, +Y up, -Z forward, the origin at
//! the centre of gravity.
pub mod b747;
pub mod c172;
pub mod concorde;
pub mod f22;
pub mod f4;
pub mod fa18;
pub mod sr71;
pub mod starship;

use crate::airframe::{Model, Sect};
use glam::{DVec3, Vec3};

/// Every type, in the order the menu lists them.
pub static ALL: [&Spec; 8] = [&c172::SPEC, &b747::SPEC, &fa18::SPEC, &f4::SPEC, &f22::SPEC, &starship::SPEC, &concorde::SPEC, &sr71::SPEC];

/// A type by its short name (`c172`, `747`, ...), as typed on the command line.
pub fn by_key(key: &str) -> Option<&'static Spec> {
    let k = key.to_ascii_lowercase();
    ALL.iter().copied().find(|s| s.keys.iter().any(|x| *x == k))
}

/// The most landing gear legs any type has.
pub const MAX_GEAR: usize = 6;

/// What a contact point on the airframe belongs to (see `Plane::contacts`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    /// spinner and cowling, or a jet's nose
    Nose,
    /// the propeller's tips
    Prop,
    /// the bottom of an engine pod hanging under that wing (0 left)
    Pod(usize),
    Belly,
    /// the cabin's sides
    Side,
    /// over the cabin and behind the wing
    Roof,
    /// the tail cone
    Tail,
    Fin,
    Stab(usize),
    /// a wing (side) at a span station
    Wing(usize, f32),
    /// a wing that is hanging from its stump
    Fold(usize),
}

/// One landing gear leg: where its tyres meet the ground fully extended
/// (body frame), and how it carries, steers and brakes the aircraft.
pub struct Leg {
    pub at: DVec3,
    pub wheel_r: f64,
    /// oleo travel (m), and the spring and damper (N/m, N s/m) with the most force it takes
    pub travel: f64,
    pub k: f64,
    pub c: f64,
    pub nf_max: f64,
    /// how much of the steering angle it follows (1 a nose wheel)
    pub steer: f64,
    pub brakes: bool,
    /// tyre side force per m/s of sideways slip and its friction limit; rolling
    /// resistance likewise; braking force per m/s and its friction limit
    pub side_k: f64,
    pub side_mu: f64,
    pub roll_k: f64,
    pub roll_mu: f64,
    pub brake_k: f64,
    pub brake_mu: f64,
    /// sink rate (m/s) that folds it, and how it throws the aircraft about when it does
    pub fold_vn: f64,
    pub fold_kick: DVec3,
    /// what the HUD calls it
    pub name: &'static str,
}

/// How the airframe itself slides along the ground once it is down on it.
pub struct Hull {
    pub k: f64,
    pub c: f64,
    pub nf_max: f64,
    /// friction force per m/s of sliding (before the friction limit)
    pub drag: f64,
}

/// Where the tail surfaces are, for breaking them and for what still touches the ground.
pub struct Tail {
    /// height of the fin's root and how tall it is above that; the same for the rudder
    pub fin_y0: f32,
    pub fin_h: f32,
    pub rudder_y0: f32,
    pub rudder_h: f32,
    /// |x| beyond which a stabiliser half breaks off, and an elevator half
    pub stab_root: f32,
    pub elev_root: f32,
    /// where along the tail cone it snaps (z range)
    pub break_z: [f32; 2],
    /// the fin's root and tip, and a stabiliser tip (right side), as contact points
    pub fin_base: DVec3,
    pub fin_tip: DVec3,
    pub stab_tip: DVec3,
}

/// A piston engine turning a fixed-pitch propeller.
pub struct Piston {
    /// rated power (W) at rated revs (rad/s)
    pub power: f64,
    pub w_rated: f64,
    /// propeller diameter (m); thrust and power coefficients at rest; the
    /// advance ratios where thrust and then the power it takes run out
    pub prop_d: f64,
    pub ct0: f64,
    pub cp0: f64,
    pub j_t: f64,
    pub j_p: f64,
    /// propeller, crankshaft and flywheel (kg m²)
    pub prop_i: f64,
    /// about the static thrust at full power (scale for torque and slipstream effects)
    pub static_thrust: f64,
    /// friction and pumping losses: constant and at rated revs (N m)
    pub fric0: f64,
    pub fric1: f64,
    /// height of the propeller hub and where a broken blade snaps (body y)
    pub blade_break_y: f32,
}

/// High-bypass turbofans.
pub struct Fan {
    pub n: usize,
    /// sea-level static thrust of each (N)
    pub thrust: f64,
    /// fan speed at idle (fraction of N1) and the thrust it gives (fraction)
    pub idle: f64,
    pub idle_thrust: f64,
    /// fan revs at 100% N1
    pub n1_rpm: f64,
    /// how quickly the fan spools up and down (1/s near the top)
    pub spool_up: f64,
    pub spool_down: f64,
    /// reverse thrust as a fraction of forward, and how long the reversers take to deploy (s)
    pub reverse: f64,
    pub reverser_time: f64,
    /// thrust with the boost key held, as a multiple of full dry thrust (an afterburner)
    pub boost: f64,
    /// thrust lapse with Mach number m: 1 - a m + b m² (a high-bypass fan loses thrust
    /// as it speeds up; a low-bypass afterburning engine gains it from the ram air)
    pub mach_lapse: [f64; 2],
    /// station of the nozzle exits (body z), where the afterburners' light comes from
    pub nozzle_z: f64,
}

/// One stage of a rocket. Stations are body z (the nose is at -z).
pub struct Stage {
    pub name: &'static str,
    /// empty and propellant masses (kg)
    pub dry: f64,
    pub prop: f64,
    /// thrust of all its engines together at sea level (N), and how many there are
    pub thrust: f64,
    pub n: u32,
    /// specific impulse at sea level and in a vacuum (s): thrust grows as the air thins
    pub isp: [f64; 2],
    /// the sets of engines that can be lit: name, how many burn, how many of those gimbal
    pub groups: &'static [(&'static str, u32, u32)],
    /// its forward end and its engines' exit plane
    pub z: [f64; 2],
    /// where the air's side force acts on it when it flies alone, as a fraction of its length from the front
    pub cp: f64,
    /// side force its control surfaces (grid fins, flaps) give per Pa of dynamic pressure at full deflection (N/Pa)
    pub fins: f64,
    /// dynamic pressure x the sine of the angle of attack (Pa) that breaks it up
    pub q_alpha: f64,
}

/// A liquid-fuelled rocket of two stages: the first (a booster) carries the
/// second until it is dropped (`Plane::stage`), see plane/rocket.rs.
pub struct Rocket {
    pub stages: [Stage; 2],
    pub radius: f64,
    /// the lowest the engines throttle to (fraction of full thrust), and how far they gimbal (rad)
    pub throttle_min: f64,
    pub gimbal: f64,
    /// the second stage's flaps, the forward pair and the aft: the area of
    /// each (m²), how far out it is and its station (body x, z)
    pub flaps: [[f64; 3]; 2],
}

pub enum Engine {
    Piston(Piston),
    Fan(Fan),
    Rocket(Rocket),
}

/// The handling: angular accelerations straight from dynamic pressure (see
/// `Plane::step`). `q_ref` is the dynamic pressure (Pa) at which control
/// authority is "1"; `damp_v` turns rotation rates into damping.
pub struct Handling {
    pub q_ref: f64,
    pub a_max: f64,
    pub damp_v: f64,
    pub ad_min: f64,
    pub ad_max: f64,
    /// pitch: stiffness about the trimmed angle of attack (and without a
    /// tailplane), stick authority, how far the stick moves the angle of attack, damping
    pub pitch_k: f64,
    pub pitch_k_lost: f64,
    pub pitch_ctl: f64,
    pub alpha_ctl: f64,
    pub pitch_d: f64,
    /// roll: authority, damping, roll from sideslip
    pub roll_ctl: f64,
    pub roll_d: f64,
    pub roll_beta: f64,
    /// how much roll authority is left at high speed with the flaps up (outboard ailerons locked out)
    pub roll_clean: f64,
    /// yaw: rudder authority, weathercock stiffness, damping
    pub yaw_ctl: f64,
    pub yaw_beta: f64,
    pub yaw_d: f64,
    /// a banked aircraft rolling back level; adverse yaw from the ailerons
    pub dihedral: f64,
    pub adverse: f64,
    /// a stall: nose drop, wing drop, and how much a yaw rate feeds the wing drop
    pub stall_pitch: f64,
    pub stall_drop: f64,
    pub stall_yaw: f64,
}

/// The head-on outline of the hole it punches through a building (m, body
/// frame seen from ahead), see `destruct::Hitter`.
pub struct Hole {
    /// fuselage half width and half height, its centre's height, corner radius
    pub fus: [f32; 4],
    /// wing: root height, dihedral (rise per metre out), half thickness at root and tip
    pub wing: [f32; 4],
    /// stabiliser height, half thickness, half span
    pub stab: [f32; 3],
    /// fin root height, height, half width at root and tip
    pub fin: [f32; 4],
    /// engine pods: (|x|, y, radius); radius 0 for none
    pub pods: [[f32; 3]; 2],
}

/// What a flap lever works on a type with no flaps: Concorde's nose and visor.
/// (To the flight and damage models they are still its flaps: the notches, the
/// speeds they are good for, the two flap roles.)
pub struct Lever {
    /// what it is called, and each notch (English, French)
    pub name: [&'static str; 2],
    pub notches: &'static [[&'static str; 2]],
    /// the pieces with the left and the right flap role, as the damage messages name them
    pub parts: [&'static str; 2],
}

impl Lever {
    pub fn notch(&self, k: u8) -> &'static str {
        self.notches.get(k as usize).map_or("", |n| n[crate::i18n::is_fr() as usize])
    }
}

/// A braking parachute, streamed on landing with the speedbrake key and let
/// go with it again: it is used once.
pub struct Chute {
    /// its drag area (m²: drag coefficient times canopy area)
    pub cda: f64,
    /// the fastest it may be streamed at (m/s indicated; faster tears it away),
    /// and the speed it is let go at by itself, before it drags on the runway
    pub v_max: f64,
    pub v_min: f64,
}

pub enum Sound {
    Piston,
    Jet,
    /// a pair of afterburning engines, from recordings (audio.rs `Fighter`)
    Fighter,
    /// a pair of afterburning turbojets: the fighter's recordings played lower and rougher, with the J79's howl
    Turbojet,
    /// a cluster of rocket engines (audio.rs `Rocket`)
    Rocket,
}

/// A painted skin: pictures of the aircraft seen from straight above, from
/// below and from its left side (and from its right, if `views` is 4;
/// otherwise the left one serves both sides, mirrored), side by side in one
/// JPEG, each nose up. plane.wgsl projects them back onto the skin
/// (`hornet_paint`). `span` x `length` (m) is the rectangle each picture
/// covers, GUIDE_SPAN and GUIDE_LEN in the type's Blender script; `centre`
/// the station ahead of the datum and the height above it at the middle of
/// the pictures (GUIDE_Y, GUIDE_Z).
pub struct Paint {
    pub jpg: &'static [u8],
    pub views: u32,
    pub span: f64,
    pub length: f64,
    pub centre: [f64; 2],
    /// how much relief the picture's lines stand for (1: a line as dark as it
    /// looks is that deep; more for a dark paint, whose seams hardly differ
    /// from it in brightness), and how much its sheen varies with its tone
    /// (0: not at all)
    pub relief: f32,
    pub sheen: f32,
}

pub struct Spec {
    /// what plane.wgsl knows it by (its paint scheme, windows, panel lines)
    pub id: u32,
    pub name: &'static str,
    /// names it answers to on the command line
    pub keys: &'static [&'static str],
    pub blurb: &'static str,
    /// its picture in the start screen's aircraft window: a PNG with alpha,
    /// 680 x 392 (the window's 340 x 196 at twice the size, for hi-dpi)
    pub still: &'static [u8],
    /// its painted skin (airframe `pat` 17), if it has one
    pub paint: Option<Paint>,
    pub build: fn() -> Model,
    /// the breakable groups (see airframe::G_*): name, mass (kg)
    pub groups: [(&'static str, f32); 7],

    // ---- geometry the flight and damage models need
    /// span station (m out from the centreline) of the wing tip
    pub span: f32,
    /// the right wing's section at a span station
    pub wing_sect: fn(f32) -> Sect,
    /// fuselage section at station z: top, the height of its widest point, bottom (body y)
    pub fuse: fn(f32) -> (f32, f32, f32),
    /// span stations the ailerons run between, and the fuel tanks
    pub aileron: [f32; 2],
    pub tank: [f32; 2],
    pub tail: Tail,
    /// the pilot's eye
    pub eye: Vec3,
    /// the eye in its other cockpit, if it has two (the view key goes round: outside, front seat, back seat)
    pub eye2: Option<Vec3>,
    /// does a view from the eye along this direction see the cabin (rather than out a window)?
    pub sees_inside: fn(Vec3, Vec3) -> bool,
    /// points of the fuselage that can touch the ground, and of the tail cone
    pub contacts: &'static [([f64; 3], Part)],
    pub tail_pts: &'static [[f64; 3]],
    /// wing contact stations as fractions of the span
    pub wing_st: [f32; 5],
    /// points that must not touch anything: nose, left tip, right tip, tail,
    /// fin top, belly, lowest point of the propeller or engine pods
    pub hard: [DVec3; 7],
    pub gear: &'static [Leg],
    pub hull: Hull,
    pub hole: Hole,

    // ---- mass and aerodynamics
    /// mass with the tanks half full (kg); the fuel on board adds or takes away from it
    pub mass: f64,
    /// usable fuel with the tanks full (kg), and what the engines burn
    /// together (kg/s): idling, at full dry power, and with the boost (afterburners)
    pub fuel_kg: f64,
    pub fuel_flow: [f64; 3],
    /// payload (crew, passengers, baggage, kg): what `mass` includes, the most
    /// it may carry, and how far aft of the reference CG more of it sits (m)
    pub payload: [f64; 3],
    /// balance: the mean aerodynamic chord (m); the centre of gravity at
    /// `mass` and its forward and aft limits (fractions of it); how far aft of
    /// the reference CG the fuel sits (m)
    pub mac: f64,
    pub cg: [f64; 3],
    pub fuel_arm: f64,
    /// pitch, yaw, roll (kg m²)
    pub inertia: DVec3,
    pub wing_area: f64,
    /// wing span (m)
    pub b: f64,
    /// lift at zero angle of attack and its slope per radian
    pub cl0: f64,
    pub cl_a: f64,
    /// drag: clean, the induced-drag factor, the lowered gear, flaps (linear and squared in the extension)
    pub cd0: f64,
    pub k_ind: f64,
    pub cd_gear: f64,
    pub cd_flap: [f64; 2],
    /// lift the flaps add at full travel
    pub flap_cl: f64,
    /// stall angle of attack clean, how much full flaps take off it and leading-edge devices add
    pub stall_alpha: f64,
    pub stall_flap: f64,
    pub stall_slat: f64,
    /// the highest angle of attack the trim will hold
    pub alpha_trim: f64,
    /// Mach number where wave drag starts (0: never flies fast enough to care)
    pub mcrit: f64,
    /// supersonic wave drag it levels off at by `mcrit` + 0.25 (0: Lock's fourth-power rise, for subsonic types)
    pub cd_wave: f64,
    /// spoilers fully up: the drag they add and the lift they dump
    pub spoiler_cd: f64,
    pub spoiler_cl: f64,
    /// a braking parachute on the speedbrake key, for a type with no speedbrake
    pub chute: Option<Chute>,
    /// flap lever notches (degrees) and the fastest each may be out at (m/s indicated)
    pub flaps: &'static [f64],
    pub vfe: &'static [f64],
    /// how quickly the flaps run (fraction of full travel per second)
    pub flap_rate: f64,
    /// from which notch the leading-edge devices are out (0: it has none)
    pub slats_at: u8,
    /// what the flap lever works, when that is not flaps (None: flaps)
    pub lever: Option<Lever>,
    pub gear_rate: f64,
    /// never-exceed speed (m/s indicated) and Mach limit (0: none); flutter sets in at `flutter_v`
    pub vne: f64,
    pub mmo: f64,
    pub flutter_v: f64,
    /// a turbo-ramjet's design Mach number (0: an ordinary engine): past it the
    /// inlets choke and the thrust falls away, and its afterburners' fuel is
    /// metered to the air they get, lapsing with height as the thrust does
    /// (other afterburners burn at their sea-level rate wherever they are)
    pub inlet_mach: f64,
    /// the Mach number by which its thrust without the afterburners has all
    /// but gone (0: dry and afterburning thrust lapse alike): a turbojet built
    /// to cruise in afterburner, whose core does less and less past Mach 2
    pub dry_mach: f64,
    /// load where the spar starts to fatigue, and where it snaps
    pub g_limit: f32,
    pub g_break: f32,
    /// fly-by-wire: the most load (g) the stick can ask for (0: none, the stick moves the elevator)
    pub fbw_g: f64,
    /// nose wheel steering: largest angle, and the speed it fades down to a fifth by (0: it does not)
    pub steer_max: f64,
    pub steer_fade: f64,
    pub engine: Engine,
    pub handling: Handling,

    // ---- the game around it
    /// starting speed in the air, and on a final approach (m/s) with that flap notch
    pub cruise: f64,
    pub approach: (f64, u8),
    /// chase camera distance and the size of its own fine shadow map (m)
    pub cam_dist: f64,
    pub shadow_r: f32,
    pub sound: Sound,
}

impl std::fmt::Debug for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

impl Spec {
    pub fn is_jet(&self) -> bool {
        matches!(self.engine, Engine::Fan(_))
    }
    pub fn rocket(&self) -> Option<&Rocket> {
        match &self.engine {
            Engine::Rocket(r) => Some(r),
            _ => None,
        }
    }
    /// The flap notch fraction (of full travel) for a lever notch.
    pub fn flap_frac(&self, notch: u8) -> f64 {
        let full = *self.flaps.last().unwrap_or(&1.0);
        self.flaps.get(notch as usize).map(|d| d / full.max(1e-6)).unwrap_or(1.0)
    }
    pub fn max_notch(&self) -> u8 {
        (self.flaps.len().max(1) - 1) as u8
    }
}
