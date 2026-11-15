//! The cabin: lining and carpet, the instrument panel with its gauges, radios,
//! switches and breakers, the moving controls, seats and belts, and the glass.
use super::*;
use crate::airframe::*;
use glam::{Mat3, Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const PANEL_Z: f32 = -1.30;
const FLOOR_Y: f32 = -0.585;

/// Instrument ids (vertex `mat.w` on GAUGE); keep in step with plane.wgsl.
pub mod gauge {
    pub const ASI: f32 = 0.0;
    pub const AI: f32 = 1.0;
    pub const ALT: f32 = 2.0;
    pub const TC: f32 = 3.0;
    pub const HI: f32 = 4.0;
    pub const VSI: f32 = 5.0;
    pub const TACH: f32 = 6.0;
    pub const MP: f32 = 7.0;
    pub const FUEL_L: f32 = 8.0;
    pub const FUEL_R: f32 = 9.0;
    pub const OIL_T: f32 = 10.0;
    pub const OIL_P: f32 = 11.0;
    pub const AMPS: f32 = 12.0;
    pub const SUCTION: f32 = 13.0;
    pub const CLOCK: f32 = 14.0;
    pub const CDI: f32 = 15.0;
    pub const EGT: f32 = 16.0;
    pub const COMPASS: f32 = 17.0;
    pub const CHT: f32 = 18.0;
    pub const FUEL_SEL: f32 = 19.0;
    pub const OAT: f32 = 20.0;
}

const PANEL_COL: Mat = mt(mat::PLASTIC, 0.035, 0.036, 0.038);
const TRIM: Mat = mt(mat::PLASTIC, 0.30, 0.29, 0.27);
const UPHOLSTERY: Mat = mt(mat::FABRIC, 0.085, 0.09, 0.105);
const STRAP: Mat = mt(mat::FABRIC, 0.05, 0.055, 0.065);
const KNOB: Mat = mt(mat::PLASTIC, 0.02, 0.02, 0.022);

pub fn build(b: &mut Builder) {
    let mut m = Mesh::default();
    lining(&mut m);
    window_trim(&mut m);
    door_trim(&mut m);
    panel(&mut m);
    seats(&mut m);
    fittings(&mut m);
    extras(&mut m);
    b.fixed("cabin", G_FUSE, m);
    controls(b);
    glass(b);
}

/// Quad with a [-1, 1] local frame, facing ax x ay.
fn rect(m: &mut Mesh, mm: &Mat, c: Vec3, ax: Vec3, ay: Vec3, hx: f32, hy: f32) {
    let n = ax.cross(ay).normalize();
    let base = m.v.len() as u32;
    for (u, v) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        m.push(c + ax * (u * hx) + ay * (v * hy), n, [u, v], mm, 0.0);
    }
    m.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}
fn circle(c: Vec2, r: f32, n: usize) -> Vec<Vec2> {
    (0..n).map(|k| { let a = -(k as f32) / n as f32 * TAU; c + Vec2::new(a.cos(), a.sin()) * r }).collect()
}
/// Strap or bar between two points, `w` wide, lying flat against `n`.
fn strap(m: &mut Mesh, mm: &Mat, a: Vec3, c: Vec3, n: Vec3, w: f32, t: f32) {
    let d = (c - a).normalize();
    let x = n.cross(d).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(x, d, x.cross(d)));
    m.rbox(mm, (a + c) * 0.5, Vec3::new(w * 0.5, (c - a).length() * 0.5, t * 0.5), t * 0.45, rot);
}

// ---------------------------------------------------------------- shell

fn inner(z: f32) -> Sec {
    let mut s = sec(z);
    s.w -= 0.035;
    s.top -= 0.045;
    s
}
/// Half width of the cabin lining at station z, height y.
fn inner_x(z: f32, y: f32) -> f32 {
    let s = inner(z);
    let (mut a, mut c) = (0.0f32, PI);
    for _ in 0..30 {
        let mid = (a + c) * 0.5;
        if sec_pt(&s, mid).y > y { a = mid } else { c = mid }
    }
    sec_pt(&s, (a + c) * 0.5).x
}

/// Height of the cabin lining's roof at station z, lateral position x.
fn inner_top_y(z: f32, x: f32) -> f32 {
    let s = inner(z);
    let x = x.abs();
    let (mut a, mut c) = (0.0f32, FRAC_PI_2);
    for _ in 0..30 {
        let mid = (a + c) * 0.5;
        if sec_pt(&s, mid).x < x { a = mid } else { c = mid }
    }
    sec_pt(&s, (a + c) * 0.5).y
}

fn lining(m: &mut Mesh) {
    // Side walls and headliner, one side then its mirror; the windows are cut
    // out of it in the shader, as they are out of the skin.
    let (z0, z1) = (-2.03f32, 1.55f32);
    let nz = 110;
    let nt = 60;
    let mut side = Mesh::default();
    let rows: Vec<Vec<_>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            let s = inner(z);
            let (mut a, mut c) = (0.0f32, PI);
            // (A little below the carpet, so no light shows along the seam.)
            for _ in 0..30 {
                let mid = (a + c) * 0.5;
                if sec_pt(&s, mid).y > FLOOR_Y - 0.015 { a = mid } else { c = mid }
            }
            let thf = a;
            let mut arc = 0.0;
            let mut prev = sec_pt(&s, 0.0);
            (0..=nt)
                .map(|k| {
                    let p = sec_pt(&s, k as f32 / nt as f32 * thf);
                    arc += (p - prev).length();
                    prev = p;
                    (Vec3::new(p.x, p.y, z), [z, arc], 0.0)
                })
                .collect()
        })
        .collect();
    side.grid(&TRIM.pat(pat::LINING), &rows, false, true);
    for v in &mut side.v {
        let y = v.pos[1];
        let c = if y > 0.47 {
            Vec3::new(0.50, 0.49, 0.46)
        } else if y < -0.36 {
            Vec3::new(0.10, 0.10, 0.10)
        } else {
            Vec3::new(0.30, 0.29, 0.27)
        };
        v.col = [c.x, c.y, c.z, 1.0];
        if y > 0.47 {
            v.mat[0] = mat::FABRIC;
        }
    }
    let l = side.mirror_x();
    m.append(&side);
    m.append(&l);
    // Baggage bulkhead.
    let s = inner(z1);
    let ring: Vec<Vec3> = (0..48).map(|k| { let p = sec_pt(&s, k as f32 / 48.0 * TAU); Vec3::new(p.x, p.y.max(FLOOR_Y), z1) }).collect();
    m.fan(&TRIM, Vec3::new(0.0, 0.0, z1), &ring, Vec3::NEG_Z);
    // Carpet, rising toward the firewall in the footwells, with the nose wheel hump.
    let floor_y = |z: f32| if z > -1.62 { FLOOR_Y } else { FLOOR_Y + (-1.62 - z) * 0.28 };
    let carpet = mt(mat::FABRIC, 0.045, 0.048, 0.055).pat(pat::CARPET);
    m.patch(&carpet, 80, 16, false, |a, c| {
        let z = z0 + (z1 - z0) * a;
        // Wall to wall: tucked a little under the lining where it meets the floor.
        let x = (c - 0.5) * 2.0 * (inner_x(z, floor_y(z)) + 0.008);
        (Vec3::new(x, floor_y(z), z), [z, x], 0.0)
    });
    // The nose wheel stows under the hump, through a notch in the firewall.
    let hump = [(-2.07f32, -0.17f32), (-1.86, -0.17), (-1.86, 0.17), (-2.07, 0.17)];
    cover(m, &carpet, &hump, -0.29, &[true, true, true, false]);
    // The main gear legs fold into troughs under the rear floor, the wheels
    // into tubs in the baggage area.
    let mut tubs = Mesh::default();
    cover(&mut tubs, &carpet, &[(0.20, 0.355), (1.13, 0.17), (1.13, 0.46), (0.20, 0.46)], -0.455, &[true, false, true, true]);
    cover(&mut tubs, &carpet, &[(1.13, 0.0), (1.549, 0.0), (1.549, 0.46), (1.13, 0.46)], -0.235, &[false, true, true, true]);
    m.append(&tubs.mirror_x());
    m.append(&tubs);
    // Firewall.
    for (x0, x1, y0) in [(-0.55f32, -0.145f32, -0.47f32), (-0.145, 0.145, -0.30), (0.145, 0.55, -0.47)] {
        // Inside the lining at its front edge: its top corners would
        // otherwise stand up through the cowl deck in front of the windshield.
        m.patch(&GREY, 6, 6, false, |a, c| {
            let x = x0 + (x1 - x0) * a;
            let top = 0.30f32.min(inner_top_y(-2.03, x) - 0.005);
            let y = y0 + (top - y0) * c;
            let x = x.signum() * x.abs().min(inner_x(-2.03, y) - 0.005);
            (Vec3::new(x, y, -2.03), [x, y], 0.0)
        });
    }
    // Seat rails.
    for xs in [-0.27f32, 0.27] {
        for dx in [-0.15f32, 0.15] {
            m.rbox(&ALU.pat(pat::KNURL), Vec3::new(xs + dx, FLOOR_Y + 0.008, -0.62), Vec3::new(0.012, 0.009, 0.42), 0.003, Quat::IDENTITY);
        }
    }
    // Door armrests, handles and window latches.
    for sx in [-1.0f32, 1.0] {
        m.rbox(&mt(mat::PLASTIC, 0.22, 0.21, 0.2), Vec3::new(sx * 0.515, -0.07, -0.95), Vec3::new(0.035, 0.028, 0.2), 0.02, Quat::IDENTITY);
        // Inside door handle in its recessed escutcheon, aft of the armrest.
        let hx = inner_x(-0.62, -0.01);
        m.rbox(&BLACK, Vec3::new(sx * (hx - 0.004), -0.01, -0.62), Vec3::new(0.004, 0.025, 0.06), 0.004, Quat::IDENTITY);
        m.rbox(&CHROME, Vec3::new(sx * (hx - 0.016), -0.01, -0.62), Vec3::new(0.008, 0.008, 0.05), 0.006, Quat::from_rotation_x(0.3));
        // Air vents at the top corners of the windshield.
        let c = Vec3::new(sx * 0.47, 0.545, -0.93);
        m.lathe(&KNOB, c, Vec3::new(-sx * 0.3, -1.0, 0.5), &[[0.03, -0.004], [0.03, 0.0], [0.024, 0.006], [0.02, 0.006]], 20, false);
        m.dome(&mt(mat::PLASTIC, 0.12, 0.12, 0.12), c, Vec3::new(-sx * 0.3, -1.0, 0.5), 0.02, 0.012, 14, 0.0);
        // Sun visors, folded up.
        m.rbox(&mt(mat::PLASTIC, 0.06, 0.07, 0.06), Vec3::new(sx * 0.27, 0.565, -0.93), Vec3::new(0.16, 0.004, 0.065), 0.004, Quat::from_rotation_x(0.12));
        m.tube(&CHROME, &[Vec3::new(sx * 0.10, 0.575, -0.995), Vec3::new(sx * 0.44, 0.575, -0.995)], &|_| 0.004, 6, true);
    }
    // Dome light.
    m.lathe(&mt(mat::PLASTIC, 0.7, 0.7, 0.66), Vec3::new(0.0, inner(-0.2).top, -0.2), Vec3::NEG_Y, &[[0.04, 0.0], [0.04, 0.006], [0.032, 0.012], [0.0, 0.014]], 20, true);
}

// ---------------------------------------------------------------- window trim

/// Moulded window surrounds, slightly lighter than the walls they sit on.
const SURROUND: Mat = mt(mat::PLASTIC, 0.34, 0.33, 0.31);
/// Scale on the polar angle so the (z, angle) plane is roughly isotropic.
const POLAR_R: f32 = 0.55;

/// Where a ray from the waist centre of section `s` at polar angle `phi`
/// (0 straight up, positive toward the right) meets it.
fn ray_hit(s: &Sec, phi: f32) -> Vec2 {
    let a = phi.abs().min(PI);
    let (mut lo, mut hi) = (0.0f32, PI);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        let p = sec_pt(s, mid);
        if p.x.atan2(p.y - s.mid) < a { lo = mid } else { hi = mid }
    }
    let p = sec_pt(s, (lo + hi) * 0.5);
    Vec2::new(if phi < 0.0 { -p.x } else { p.x }, p.y)
}
/// The skin and the lining over a (station, scaled polar angle) plane, so a
/// point of one has the other radially inside or outside it.
fn skin_at(q: Vec2) -> Vec3 {
    let p = ray_hit(&sec(q.x), q.y / POLAR_R);
    Vec3::new(p.x, p.y, q.x)
}
fn lin_at(q: Vec2) -> Vec3 {
    let p = ray_hit(&inner(q.x), q.y / POLAR_R);
    Vec3::new(p.x, p.y, q.x)
}
/// Outward normal of either surface at q.
fn surf_n(f: fn(Vec2) -> Vec3, q: Vec2) -> Vec3 {
    let e = 0.002;
    let dz = f(q + Vec2::X * e) - f(q - Vec2::X * e);
    let dp = f(q + Vec2::Y * e) - f(q - Vec2::Y * e);
    dz.cross(dp).normalize()
}
fn skin_window(q: Vec2) -> f32 {
    window(skin_at(q))
}
/// Unit direction in the plane away from the opening (up the distance field).
fn away(q: Vec2) -> Vec2 {
    let e = 0.002;
    let g = Vec2::new(skin_window(q + Vec2::X * e) - skin_window(q - Vec2::X * e), skin_window(q + Vec2::Y * e) - skin_window(q - Vec2::Y * e));
    g.normalize_or_zero()
}

/// The edge of the skin's opening round centre `c`, every ~8 mm, closed.
fn trace_opening(c: Vec2) -> Vec<Vec2> {
    let n = 720;
    let raw: Vec<Vec2> = (0..n)
        .map(|k| {
            let a = k as f32 / n as f32 * TAU;
            let d = Vec2::new(a.cos(), a.sin());
            let step = 0.008;
            let mut t = 0.0;
            while t < 1.6 && skin_window(c + d * (t + step)) < 0.0 {
                t += step;
            }
            let (mut lo, mut hi) = (t, t + step);
            for _ in 0..18 {
                let m = (lo + hi) * 0.5;
                if skin_window(c + d * m) < 0.0 { lo = m } else { hi = m }
            }
            c + d * ((lo + hi) * 0.5)
        })
        .collect();
    // Even spacing along the skin.
    let pts: Vec<Vec3> = raw.iter().map(|&q| skin_at(q)).collect();
    let mut acc = vec![0.0f32];
    for k in 0..n {
        acc.push(acc[k] + (pts[(k + 1) % n] - pts[k]).length());
    }
    let total = acc[n];
    let m = (total / 0.008).round() as usize;
    let mut out = Vec::with_capacity(m);
    let mut j = 0;
    for i in 0..m {
        let s = total * i as f32 / m as f32;
        while acc[j + 1] < s {
            j += 1;
        }
        let t = (s - acc[j]) / (acc[j + 1] - acc[j]).max(1e-9);
        out.push(raw[j].lerp(raw[(j + 1) % n], t));
    }
    out
}

/// One window's frame from inside: a rubber gasket where the glass meets the
/// skin, a reveal straight in to the lining, and a moulded surround with a
/// rounded lip that laps over the lining's own cut edge. Returns points along
/// the middle of the surround (with its normal) for screws.
fn frame_window(m: &mut Mesh, c: Vec2, flange: f32, cut: &dyn Fn(Vec3) -> bool) -> Vec<(Vec3, Vec3, f32)> {
    let loop_q = trace_opening(c);
    struct Pt {
        q: Vec2,
        g: Vec2,
        s: Vec3,
        ns: Vec3,
        u: Vec3,
        nl: Vec3,
        k: f32, // plane units per metre along the lining, toward g
    }
    let pts: Vec<Pt> = loop_q
        .iter()
        .map(|&q| {
            let g = away(q);
            let s = skin_at(q);
            let ns = surf_n(skin_at, q);
            let u = (skin_at(q + g * 0.003) - s).normalize();
            let l = lin_at(q);
            let nl = surf_n(lin_at, q);
            let k = 0.003 / (lin_at(q + g * 0.003) - l).length().max(1e-6);
            Pt { q, g, s, ns, u, nl, k }
        })
        .collect();
    // How far out the surround has to reach to cover the lining's cut
    // (the lining is cut where its own point lies over an opening).
    let mut need = flange;
    for p in &pts {
        let mut a = 0.0;
        while a < 0.08 && window(lin_at(p.q + p.g * (a * p.k))) < 0.006 {
            a += 0.002;
        }
        need = need.max(a + 0.008);
    }
    let w = need.min(0.075);
    // Lining point `a` metres out from the reveal, lifted `h` into the cabin.
    let on_lin = |p: &Pt, a: f32, h: f32| lin_at(p.q + p.g * (a * p.k)) - p.nl * h;
    let flange_prof: [(f32, f32); 9] = [(-0.004, 0.0065), (-0.001, 0.0105), (0.004, 0.0125), (0.010, 0.0118), (0.30, 0.0085), (0.55, 0.0055), (0.78, 0.003), (0.93, 0.0015), (1.0, 0.0006)];

    let mut gasket: Vec<Vec<SP>> = Vec::new();
    let mut trim: Vec<Vec<SP>> = Vec::new();
    let mut arc = 0.0;
    let mut screws = Vec::new();
    let mut runs: Vec<(Vec<Vec<SP>>, Vec<Vec<SP>>)> = Vec::new();
    let n = pts.len();
    for i in 0..=n {
        let p = &pts[i % n];
        if i > 0 {
            arc += (p.s - pts[i - 1].s).length();
        }
        if cut(p.s) {
            // Hidden (under the glareshield): end this run here.
            if gasket.len() > 1 {
                runs.push((std::mem::take(&mut gasket), std::mem::take(&mut trim)));
            }
            gasket.clear();
            trim.clear();
            continue;
        }
        let (s, ns, u) = (p.s, p.ns, p.u);
        gasket.push(vec![
            (s - u * 0.009 - ns * 0.0012, [arc, 0.0], 0.0),
            (s - u * 0.007 - ns * 0.0040, [arc, 0.003], 0.0),
            (s - u * 0.0035 - ns * 0.0062, [arc, 0.007], 0.0),
            (s + u * 0.002 - ns * 0.0072, [arc, 0.012], 0.0),
        ]);
        let mut row: Vec<SP> = vec![(s + u * 0.0005 - ns * 0.0055, [arc, 0.0], 0.0)];
        // The reveal: straight in from the skin to the lip.
        let lip = on_lin(p, -0.004, 0.0065);
        for t in [0.33f32, 0.66] {
            row.push((s.lerp(lip, t), [arc, 0.045 * t], 0.0));
        }
        for &(a, h) in &flange_prof {
            let a = if a <= 0.02 { a } else { a * w };
            row.push((on_lin(p, a, h), [arc, 0.05 + a], 0.0));
        }
        trim.push(row);
        if (arc / 0.23).fract() < (0.008 / 0.23) && arc > 0.05 {
            let at = on_lin(p, 0.55 * w, 0.0055 + 0.0006);
            screws.push((at, -p.nl, arc));
        }
    }
    if gasket.len() > 1 {
        runs.push((gasket, trim));
    }
    for (g, t) in runs {
        for (rows, mat, probe) in [(g, RUBBER, 1usize), (t, SURROUND, 5usize)] {
            let start = m.v.len();
            let nc = rows[0].len();
            m.grid(&mat, &rows, false, false);
            // Face the cabin (probe a point on the lip / the gasket's crown).
            let mid = rows.len() / 2;
            let v = &m.v[start + mid * nc + probe];
            let pv = Vec3::from(v.pos);
            // Inward is toward the cabin's centre line at that height.
            let inward = Vec3::new(-pv.x, (0.1 - pv.y) * 0.5, 0.0);
            let inward = if inward.length() < 0.05 { Vec3::NEG_Y } else { inward };
            if Vec3::from(v.nrm).dot(inward) < 0.0 {
                for v in &mut m.v[start..] {
                    v.nrm = (-Vec3::from(v.nrm)).into();
                }
            }
        }
    }
    screws
}

fn window_trim(m: &mut Mesh) {
    let mut side = Mesh::default();
    let mut screws = Vec::new();
    // Door window and rear side window, right side (mirrored below).
    let phi = |z: f32, y: f32| {
        let s = sec(z);
        let th = fuse_th(z, y);
        let p = sec_pt(&s, th);
        p.x.atan2(p.y - s.mid) * POLAR_R
    };
    let never = |_: Vec3| false;
    screws.extend(frame_window(&mut side, Vec2::new(-0.90, phi(-0.90, 0.30)), 0.030, &never));
    screws.extend(frame_window(&mut side, Vec2::new(0.05, phi(0.05, 0.30)), 0.030, &never));
    for (p, n, seed) in screws.drain(..) {
        trim_screw(&mut side, p, n, seed);
    }
    m.append(&side.mirror_x());
    m.append(&side);
    // Windshield (both sides at once) and the rear window in the roof.
    // Ahead of the panel the windshield's lower edge runs under the glareshield.
    let under_gs = |p: Vec3| p.z < PANEL_Z - 0.02 && p.y < 0.30 && p.x.abs() < 0.47;
    let mut s = frame_window(m, Vec2::new(-1.40, 0.0), 0.034, &under_gs);
    s.extend(frame_window(m, Vec2::new(1.16, 0.0), 0.030, &never));
    for (p, n, seed) in s {
        trim_screw(m, p, n, seed);
    }
}

/// A Phillips screw in a cup washer, painted over with the trim.
fn trim_screw(m: &mut Mesh, p: Vec3, n: Vec3, seed: f32) {
    m.lathe(&mt(mat::PLASTIC, 0.30, 0.29, 0.275), p, n, &[[0.0046, 0.0], [0.0046, 0.0005], [0.0034, 0.0013], [0.0, 0.0013]], 16, false);
    m.screw(&mt(mat::SCREW, 0.32, 0.31, 0.29).x(1.0), p + n * 0.001, n, 0.0026, seed);
}

/// The doors from inside: the opening window's hinge and latch, a map
/// pocket, a pull strap, the seal round the frame.
fn door_trim(m: &mut Mesh) {
    let mut d = Mesh::default();
    // The door window opens: a piano hinge along its top, a latch at the bottom.
    let (z0, z1) = (-1.02f32, -0.60f32);
    let hinge: Vec<Vec3> = (0..=12).map(|k| on_side(z0 + (z1 - z0) * k as f32 / 12.0, 0.507, 1.0, -0.010).0).collect();
    d.tube(&mt(mat::ALU, 0.5, 0.5, 0.52), &hinge, &|_| 0.0042, 10, true);
    for k in 0..7 {
        let z = z0 + 0.02 + k as f32 * 0.063;
        let (p, _) = on_side(z, 0.507, 1.0, -0.010);
        d.lathe(&mt(mat::ALU, 0.42, 0.42, 0.44), p - Vec3::Z * 0.004, Vec3::Z, &[[0.0049, 0.0], [0.0049, 0.008], [0.0, 0.008]], 10, true);
    }
    // Latch: a cast body on the window frame, a lever, and its catch on the sill.
    let (lp, ln) = on_side(-0.95, 0.098, 1.0, -0.012);
    let rot = Quat::from_rotation_arc(Vec3::X, -ln);
    d.rbox(&BLACK, lp, Vec3::new(0.006, 0.010, 0.022), 0.004, rot);
    let lever: Vec<Vec3> = [(0.004, 0.004, 0.0), (0.016, 0.010, 0.018), (0.022, 0.012, 0.045)].iter().map(|&(a, b, c)| lp - ln * a + Vec3::Y * b + Vec3::Z * c).collect();
    d.tube(&BLACK, &lever, &|t| 0.0042 - 0.001 * t, 8, true);
    d.dome(&BLACK, *lever.last().unwrap(), Vec3::Z, 0.0055, 0.006, 10, 0.0);
    let (cp, cn) = on_side(-0.95, 0.058, 1.0, -0.042);
    d.rbox(&CHROME, cp, Vec3::new(0.004, 0.008, 0.018), 0.002, Quat::from_rotation_arc(Vec3::X, -cn));
    // The placard under it: the window stays shut above 163 KIAS.
    let (pp, pn) = on_side(-0.88, 0.054, 1.0, -0.0505);
    d.rbox(&PAINT_WHITE.pat(pat::PLACARD), pp, Vec3::new(0.0004, 0.0055, 0.03), 0.0002, Quat::from_rotation_arc(Vec3::X, -pn));

    // Map pocket low on the door: a moulded tray with a stiff lip.
    let pocket = mt(mat::PLASTIC, 0.20, 0.195, 0.185);
    let (zp0, zp1) = (-1.30f32, -0.72f32);
    let rows: Vec<Vec<SP>> = (0..=16)
        .map(|i| {
            let z = zp0 + (zp1 - zp0) * i as f32 / 16.0;
            let ends = (((z - zp0) / (zp1 - zp0) * PI).sin()).powf(0.3);
            let bulge = 0.032 * ends;
            let x = |y: f32| inner_x(z, y);
            let prof = [(-0.40, 0.0015), (-0.385, bulge * 0.6), (-0.36, bulge), (-0.30, bulge), (-0.255, bulge * 0.95), (-0.25, bulge * 0.9), (-0.252, 0.004)];
            prof.iter().enumerate().map(|(j, &(y, o))| (Vec3::new(x(y) - o, y, z), [z, j as f32 * 0.02], 0.0)).collect()
        })
        .collect();
    let start = d.v.len();
    d.grid(&pocket, &rows, false, false);
    if d.v[start + 8 * 7 + 3].nrm[0] > 0.0 {
        for v in &mut d.v[start..] {
            v.nrm = (-Vec3::from(v.nrm)).into();
        }
    }
    // A pen and a folded chart showing over the lip.
    d.tube(&mt(mat::PLASTIC, 0.03, 0.06, 0.25), &[Vec3::new(inner_x(-1.1, -0.25) - 0.02, -0.255, -1.16), Vec3::new(inner_x(-1.0, -0.23) - 0.022, -0.225, -1.03)], &|_| 0.004, 8, true);
    d.rbox(&mt(mat::PLASTIC, 0.62, 0.62, 0.55).pat(pat::PLACARD), Vec3::new(inner_x(-0.9, -0.27) - 0.017, -0.262, -0.90), Vec3::new(0.0015, 0.03, 0.09), 0.001, Quat::from_rotation_x(0.08));

    // Pull strap on the door just under the window, for closing it.
    let (a, an) = on_side(-0.82, 0.03, 1.0, -0.043);
    let (b, _) = on_side(-0.68, 0.03, 1.0, -0.043);
    let sag = (a + b) * 0.5 - an * 0.012 - Vec3::Y * 0.012;
    let pts: Vec<Vec3> = (0..=8).map(|k| { let t = k as f32 / 8.0; a.lerp(b, t) + (sag - (a + b) * 0.5) * (4.0 * t * (1.0 - t)) }).collect();
    for w in pts.windows(2) {
        strap(&mut d, &STRAP, w[0], w[1], -an, 0.022, 0.003);
    }
    for p in [a, b] {
        trim_screw(&mut d, p - an * 0.002, -an, p.z * 9.0);
    }

    m.append(&d.mirror_x());
    m.append(&d);
}

// ---------------------------------------------------------------- instrument panel

/// (instrument, x, y, face radius)
const GAUGES: [(f32, f32, f32, f32); 19] = [
    (gauge::ASI, -0.365, 0.090, 0.040),
    (gauge::AI, -0.270, 0.090, 0.040),
    (gauge::ALT, -0.175, 0.090, 0.040),
    (gauge::TC, -0.365, -0.005, 0.040),
    (gauge::HI, -0.270, -0.005, 0.040),
    (gauge::VSI, -0.175, -0.005, 0.040),
    (gauge::MP, -0.365, -0.100, 0.034),
    (gauge::TACH, -0.270, -0.100, 0.040),
    (gauge::EGT, -0.175, -0.100, 0.030),
    (gauge::CDI, -0.075, 0.090, 0.040),
    (gauge::CDI, -0.075, -0.005, 0.040),
    (gauge::CLOCK, -0.470, 0.095, 0.029),
    (gauge::SUCTION, -0.470, 0.020, 0.026),
    (gauge::AMPS, -0.470, -0.055, 0.026),
    (gauge::FUEL_L, 0.250, 0.100, 0.029),
    (gauge::FUEL_R, 0.320, 0.100, 0.029),
    (gauge::OIL_T, 0.390, 0.100, 0.029),
    (gauge::OIL_P, 0.460, 0.100, 0.029),
    (gauge::CHT, 0.250, 0.035, 0.029),
];
const STACK: [f32; 4] = [-0.022, 0.162, -0.128, 0.152]; // x0, x1, y0, y1

fn panel(m: &mut Mesh) {
    let z = PANEL_Z;
    let outline: Vec<Vec2> = [(-0.545, -0.245), (0.545, -0.245), (0.545, 0.150), (0.30, 0.166), (0.0, 0.171), (-0.30, 0.166), (-0.545, 0.150)].iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let mut holes: Vec<Vec<Vec2>> = GAUGES.iter().map(|g| circle(Vec2::new(g.1, g.2), g.3 + 0.001, 32)).collect();
    holes.push(vec![Vec2::new(STACK[0], STACK[2]), Vec2::new(STACK[0], STACK[3]), Vec2::new(STACK[1], STACK[3]), Vec2::new(STACK[1], STACK[2])]);
    m.slab(&PANEL_COL.pat(pat::PANEL), &outline, &holes, Vec3::new(0.0, 0.0, z), Vec3::X, Vec3::Y, 0.012);
    // The glareshield: a padded shelf from the panel lip to the windshield.
    let gs = mt(mat::FABRIC, 0.03, 0.03, 0.032);
    m.patch(&gs, 24, 10, false, |a, c| {
        let x = (a - 0.5) * 1.09;
        let t = c;
        let zz = z + 0.025 - t * 0.46;
        let arch = 0.008 * (1.0 - (x / 0.55).powi(2));
        let y = 0.166 + arch + 0.022 * (t * PI).sin() * (1.0 - t).max(0.0).sqrt() + 0.12 * t * t * t;
        // Tucked under the cowl deck and the windshield at the sides.
        let y = y.min(fuse_top_y(zz, x) - 0.025);
        // ...and inside the side walls, which close in as they rise toward
        // the windshield (else the corners poke out through the skin).
        let xc = x.signum() * x.abs().min(inner_x(zz, y) - 0.012);
        let y = y.min(fuse_top_y(zz, xc) - 0.025);
        let xc = xc.signum() * xc.abs().min(inner_x(zz, y) - 0.012);
        (Vec3::new(xc, y, zz), [x, t], 0.0)
    });
    m.tube(&gs, &[Vec3::new(-0.545, 0.172, z + 0.022), Vec3::new(0.0, 0.180, z + 0.022), Vec3::new(0.545, 0.172, z + 0.022)], &|_| 0.012, 10, true);

    // Instruments: recessed face, bezel, four mounting screws.
    for (k, &(id, x, y, r)) in GAUGES.iter().enumerate() {
        let c = Vec3::new(x, y, z - 0.010);
        m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(id), c, Vec3::Z, Vec3::Y, r, 40);
        m.lathe(&BLACK, c, Vec3::Z, &[[r, 0.0], [r, 0.0095], [r + 0.0035, 0.0125], [r + 0.0065, 0.0112], [r + 0.0068, 0.0098]], 40, false);
        let q = r * 1.06;
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(x + dx * q, y + dy * q, z + 0.0005), Vec3::Z, 0.0028, k as f32 * 4.0 + dx + dy * 2.0);
        }
    }
    // Radio stack: audio panel, GPS-com, nav-com, transponder, autopilot.
    let units: [(f32, f32, f32); 5] = [(0.152, 0.120, -1.0), (0.118, 0.045, 3.0), (0.043, -0.003, 0.0), (-0.005, -0.050, 2.0), (-0.052, -0.126, 1.0)];
    for (k, &(y1, y0, disp)) in units.iter().enumerate() {
        let (cx, cy) = ((STACK[0] + STACK[1]) * 0.5, (y0 + y1) * 0.5);
        let (hx, hy) = ((STACK[1] - STACK[0]) * 0.5 - 0.001, (y1 - y0) * 0.5 - 0.0012);
        m.rbox(&mt(mat::PLASTIC, 0.045, 0.046, 0.05), Vec3::new(cx, cy, z - 0.003), Vec3::new(hx, hy, 0.008), 0.0025, Quat::IDENTITY);
        let fz = z + 0.0051;
        if disp >= 0.0 {
            let (dw, dh) = if disp == 3.0 { (0.052, 0.027) } else { (0.048, 0.0085) };
            let dx = if disp == 3.0 { cx - 0.012 } else { cx - 0.01 };
            m.rbox(&BLACK, Vec3::new(dx, cy, fz - 0.0005), Vec3::new(dw + 0.003, dh + 0.003, 0.001), 0.001, Quat::IDENTITY);
            rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(disp), Vec3::new(dx, cy, fz + 0.0007), Vec3::X, Vec3::Y, dw, dh);
        }
        // Concentric knobs on the right, buttons along the bottom.
        let kn = Vec3::new(STACK[1] - 0.016, cy - hy * 0.25, fz);
        if disp != -1.0 {
            m.lathe(&KNOB.pat(pat::KNURL), kn, Vec3::Z, &[[0.011, 0.0], [0.011, 0.009], [0.0095, 0.010], [0.0, 0.010]], 20, false);
            m.lathe(&KNOB.pat(pat::KNURL), kn + Vec3::Z * 0.01, Vec3::Z, &[[0.0065, 0.0], [0.0065, 0.009], [0.0055, 0.010], [0.0, 0.010]], 16, false);
            m.lathe(&KNOB, Vec3::new(STACK[0] + 0.011, cy - hy * 0.3, fz), Vec3::Z, &[[0.006, 0.0], [0.006, 0.006], [0.0, 0.007]], 14, false);
        }
        let nb = if disp == -1.0 { 10 } else { 4 };
        for i in 0..nb {
            let bx = if disp == -1.0 { STACK[0] + 0.012 + i as f32 * 0.0155 } else { cx - 0.04 + i as f32 * 0.02 };
            let by = if disp == -1.0 { cy } else { y0 + 0.0065 };
            m.rbox(&mt(mat::PLASTIC, 0.09, 0.09, 0.095).pat(pat::PLACARD), Vec3::new(bx, by, fz + 0.002), Vec3::new(0.0055, 0.0035, 0.0025), 0.0012, Quat::IDENTITY);
            if disp == -1.0 && i % 3 == 0 {
                m.disc(&mt(mat::LAMP, 0.2, 0.9, 0.3).x(lamp::ANNUNC), Vec3::new(bx, by + 0.0062, fz + 0.0005), Vec3::Z, Vec3::Y, 0.0012, 8);
            }
        }
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(cx + dx * (hx - 0.004), cy + dy * (hy - 0.003), fz), Vec3::Z, 0.0018, k as f32 * 3.0 + dx);
        }
    }
    // Ignition key, the rocker switch row, placards.
    let f = z + 0.0005;
    m.lathe(&CHROME, Vec3::new(-0.48, -0.15, f), Vec3::Z, &[[0.016, 0.0], [0.016, 0.004], [0.012, 0.007], [0.0, 0.007]], 24, false);
    m.rbox(&mt(mat::STEEL, 0.6, 0.55, 0.4), Vec3::new(-0.48, -0.15, f + 0.014), Vec3::new(0.003, 0.009, 0.008), 0.0015, Quat::IDENTITY);
    m.rbox(&BLACK, Vec3::new(-0.48, -0.15, f + 0.028), Vec3::new(0.004, 0.012, 0.009), 0.003, Quat::IDENTITY);
    for i in 0..9 {
        let x = -0.525 + i as f32 * 0.03;
        let red = i < 2;
        let col = if red { mt(mat::PLASTIC, 0.35, 0.02, 0.02) } else { mt(mat::PLASTIC, 0.03, 0.03, 0.03) };
        m.rbox(&BLACK, Vec3::new(x, -0.205, f + 0.002), Vec3::new(0.008, 0.013, 0.002), 0.0015, Quat::IDENTITY);
        m.rbox(&col, Vec3::new(x, -0.205, f + 0.006), Vec3::new(0.0062, 0.011, 0.004), 0.002, Quat::from_rotation_x(if i % 3 == 1 { 0.18 } else { -0.18 }));
        m.rbox(&PAINT_WHITE.pat(pat::PLACARD), Vec3::new(x, -0.229, f), Vec3::new(0.011, 0.0035, 0.0004), 0.0002, Quat::IDENTITY);
    }
    for &(x, y, w, h) in &[(-0.27f32, -0.153f32, 0.06f32, 0.011f32), (0.355, 0.058, 0.03, 0.006), (-0.47, 0.135, 0.035, 0.008), (0.07, -0.155, 0.065, 0.009)] {
        m.rbox(&PAINT_WHITE.pat(pat::PLACARD), Vec3::new(x, y, f), Vec3::new(w, h, 0.0004), 0.0002, Quat::IDENTITY);
    }
    // Gear position lights over the gear handle.
    for (k, (x, l)) in [(-0.11f32, lamp::GEAR_DOWN), (-0.07, lamp::GEAR_UP)].iter().enumerate() {
        let col = if k == 0 { mt(mat::LAMP, 0.1, 0.9, 0.15) } else { mt(mat::LAMP, 0.95, 0.5, 0.05) };
        m.lathe(&CHROME, Vec3::new(*x, -0.138, f), Vec3::Z, &[[0.009, 0.0], [0.009, 0.004], [0.007, 0.006]], 16, false);
        m.dome(&col.x(*l), Vec3::new(*x, -0.138, f + 0.005), Vec3::Z, 0.0068, 0.005, 14, 0.0);
    }
    // Push-pull engine controls: prop (blue, crenellated), mixture (red).
    for (x, col) in [(0.048f32, mt(mat::PLASTIC, 0.02, 0.05, 0.25)), (0.094, mt(mat::PLASTIC, 0.35, 0.02, 0.02))] {
        m.tube(&CHROME, &[Vec3::new(x, -0.195, z - 0.02), Vec3::new(x, -0.195, z + 0.02)], &|_| 0.0045, 10, false);
        m.lathe(&col.pat(pat::KNURL), Vec3::new(x, -0.195, z + 0.02), Vec3::Z, &[[0.0045, 0.0], [0.014, 0.004], [0.015, 0.018], [0.013, 0.022], [0.0, 0.023]], 18, true);
    }
    m.lathe(&CHROME, Vec3::new(0.0, -0.195, f), Vec3::Z, &[[0.012, 0.0], [0.012, 0.003], [0.0, 0.003]], 18, false);
    // Flap lever slot with its detent plate.
    m.rbox(&BLACK, Vec3::new(0.15, -0.195, f + 0.001), Vec3::new(0.012, 0.04, 0.002), 0.002, Quat::IDENTITY);
    m.rbox(&PAINT_WHITE.pat(pat::PLACARD), Vec3::new(0.175, -0.195, f), Vec3::new(0.008, 0.04, 0.0004), 0.0002, Quat::IDENTITY);
    // Circuit breakers, two rows.
    for row in 0..2 {
        for i in 0..11 {
            let p = Vec3::new(0.225 + i as f32 * 0.029, -0.172 - row as f32 * 0.036, f);
            m.lathe(&BLACK, p, Vec3::Z, &[[0.0065, 0.0], [0.0065, 0.002], [0.0048, 0.003], [0.0048, 0.009], [0.004, 0.0105], [0.0, 0.0105]], 12, false);
            m.rbox(&PAINT_WHITE.pat(pat::PLACARD), p + Vec3::new(0.0, 0.0115, 0.0), Vec3::new(0.011, 0.003, 0.0004), 0.0002, Quat::IDENTITY);
        }
    }
    // Glovebox door and latch, hour meter.
    m.rbox(&PANEL_COL.pat(pat::PANEL), Vec3::new(0.39, -0.068, f + 0.003), Vec3::new(0.13, 0.045, 0.004), 0.003, Quat::IDENTITY);
    m.rbox(&CHROME, Vec3::new(0.39, -0.035, f + 0.008), Vec3::new(0.02, 0.004, 0.003), 0.002, Quat::IDENTITY);
    m.rbox(&BLACK, Vec3::new(0.45, 0.034, f + 0.002), Vec3::new(0.026, 0.01, 0.003), 0.002, Quat::IDENTITY);
    rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(5.0), Vec3::new(0.45, 0.034, f + 0.0055), Vec3::X, Vec3::Y, 0.021, 0.0065);
    // Screws holding the panel's removable centre section.
    for (x, y) in [(-0.53f32, 0.135f32), (0.53, 0.135), (-0.53, -0.232), (0.53, -0.232), (-0.03, 0.158), (0.17, 0.158), (-0.03, -0.232), (0.17, -0.232)] {
        m.screw(&SCREW_BLACK, Vec3::new(x, y, f), Vec3::Z, 0.0028, x * 11.0 + y);
    }

    // The pedestal under the panel, with the trim wheel slot and fuel selector.
    let ped = mt(mat::PLASTIC, 0.04, 0.041, 0.044).pat(pat::PANEL);
    m.rbox(&ped, Vec3::new(0.0, -0.40, z - 0.03), Vec3::new(0.072, 0.17, 0.05), 0.015, Quat::from_rotation_x(0.12));
    m.rbox(&BLACK, Vec3::new(0.0, -0.40, z + 0.02), Vec3::new(0.012, 0.075, 0.004), 0.003, Quat::from_rotation_x(0.12));
    m.rbox(&PAINT_WHITE.pat(pat::PLACARD), Vec3::new(0.03, -0.40, z + 0.022), Vec3::new(0.012, 0.06, 0.0005), 0.0003, Quat::from_rotation_x(0.12));
    let fs = Vec3::new(0.0, FLOOR_Y + 0.004, -0.98);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(gauge::FUEL_SEL), fs, Vec3::Y, Vec3::NEG_Z, 0.06, 32);
    m.lathe(&CHROME, fs, Vec3::Y, &[[0.062, 0.0], [0.062, 0.004], [0.06, 0.005]], 32, false);
    m.rbox(&mt(mat::PLASTIC, 0.35, 0.03, 0.02), fs + Vec3::Y * 0.012, Vec3::new(0.012, 0.01, 0.05), 0.006, Quat::IDENTITY);
}

/// A carpeted box rising from the floor over an outline in (z, x): a flat top
/// and the side walls whose edges are flagged (the rest stay open). Where it
/// reaches the lining it is trimmed to lean against it.
fn cover(m: &mut Mesh, mat: &Mat, outline: &[(f32, f32)], top: f32, walls: &[bool]) {
    let base = FLOOR_Y - 0.01;
    let at = |z: f32, x: f32, y: f32| Vec3::new(x.signum() * x.abs().min(inner_x(z, y) - 0.008), y, z);
    let mut ring = Vec::new();
    for k in 0..outline.len() {
        let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
        let rows: Vec<Vec<_>> = (0..=8)
            .map(|i| {
                let t = i as f32 / 8.0;
                let (z, x) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                if i < 8 {
                    ring.push(at(z, x, top));
                }
                let (p, q) = (at(z, x, base), at(z, x, top));
                vec![(p, [z + x, p.y], 0.0), (q, [z + x, q.y], 0.0)]
            })
            .collect();
        if walls[k] {
            m.grid(mat, &rows, false, false);
        }
    }
    let c = ring.iter().copied().sum::<Vec3>() / ring.len() as f32;
    m.fan(mat, c, &ring, Vec3::Y);
}

// ---------------------------------------------------------------- seats

fn seats(m: &mut Mesh) {
    let up = UPHOLSTERY.pat(pat::SEAT);
    let side = mt(mat::FABRIC, 0.05, 0.052, 0.06).pat(pat::SEAT);
    for xs in [-0.27f32, 0.27] {
        m.rbox(&up, Vec3::new(xs, -0.36, -0.68), Vec3::new(0.19, 0.045, 0.22), 0.035, Quat::from_rotation_x(-0.06));
        for dx in [-0.205f32, 0.205] {
            m.rbox(&side, Vec3::new(xs + dx, -0.35, -0.66), Vec3::new(0.025, 0.06, 0.22), 0.022, Quat::from_rotation_x(-0.06));
        }
        let tilt = Quat::from_rotation_x(0.26);
        m.rbox(&up, Vec3::new(xs, 0.0, -0.34), Vec3::new(0.2, 0.3, 0.05), 0.04, tilt);
        m.rbox(&side, Vec3::new(xs, 0.0, -0.31), Vec3::new(0.215, 0.29, 0.035), 0.03, tilt);
        m.rbox(&up, Vec3::new(xs, 0.40, -0.21), Vec3::new(0.11, 0.07, 0.04), 0.035, tilt);
        for dx in [-0.07f32, 0.07] {
            m.tube(&CHROME, &[Vec3::new(xs + dx, 0.32, -0.235), Vec3::new(xs + dx, 0.36, -0.225)], &|_| 0.005, 8, false);
        }
        // Frame and adjuster under the cushion.
        for (dx, dz) in [(-0.15f32, -0.88f32), (0.15, -0.88), (-0.15, -0.46), (0.15, -0.46)] {
            m.tube(&ALU, &[Vec3::new(xs + dx, FLOOR_Y + 0.015, dz), Vec3::new(xs + dx, -0.40, dz + 0.02)], &|_| 0.011, 8, false);
        }
        m.tube(&ALU, &[Vec3::new(xs - 0.15, -0.41, -0.88), Vec3::new(xs + 0.15, -0.41, -0.88)], &|_| 0.01, 8, false);
        m.tube(&CHROME, &[Vec3::new(xs - 0.05, -0.43, -0.88), Vec3::new(xs - 0.05, -0.44, -0.93), Vec3::new(xs + 0.05, -0.44, -0.93), Vec3::new(xs + 0.05, -0.43, -0.88)], &|_| 0.0045, 6, false);
        // Lap belt halves lying on the cushion, and the shoulder harness stowed on the wall.
        let sx = xs.signum();
        strap(m, &STRAP, Vec3::new(xs - 0.21, -0.32, -0.50), Vec3::new(xs - 0.03, -0.312, -0.62), Vec3::Y, 0.045, 0.003);
        strap(m, &STRAP, Vec3::new(xs + 0.21, -0.32, -0.50), Vec3::new(xs + 0.04, -0.312, -0.60), Vec3::Y, 0.045, 0.003);
        m.rbox(&CHROME, Vec3::new(xs + 0.01, -0.307, -0.61), Vec3::new(0.03, 0.004, 0.022), 0.003, Quat::from_rotation_y(0.4));
        strap(m, &STRAP, Vec3::new(sx * 0.535, 0.56, -0.25), Vec3::new(sx * 0.52, -0.25, -0.43), Vec3::X, 0.045, 0.003);
        m.rbox(&CHROME, Vec3::new(sx * 0.522, -0.20, -0.42), Vec3::new(0.004, 0.025, 0.018), 0.003, Quat::IDENTITY);
    }
    // Rear bench.
    m.rbox(&up, Vec3::new(0.0, -0.37, 0.42), Vec3::new(0.46, 0.05, 0.22), 0.04, Quat::IDENTITY);
    m.rbox(&up, Vec3::new(0.0, 0.0, 0.72), Vec3::new(0.46, 0.30, 0.05), 0.045, Quat::from_rotation_x(0.2));
    m.rbox(&TRIM, Vec3::new(0.0, -0.48, 0.42), Vec3::new(0.47, 0.06, 0.2), 0.02, Quat::IDENTITY);
    for xs in [-0.24f32, 0.24] {
        strap(m, &STRAP, Vec3::new(xs - 0.18, -0.315, 0.58), Vec3::new(xs, -0.315, 0.42), Vec3::Y, 0.045, 0.003);
    }
}

// ---------------------------------------------------------------- compass, extinguisher

fn fittings(m: &mut Mesh) {
    let c = Vec3::new(0.0, 0.555, -0.965);
    m.rbox(&BLACK, c, Vec3::new(0.036, 0.03, 0.032), 0.012, Quat::IDENTITY);
    rect(m, &mt(mat::GAUGE, 0.02, 0.02, 0.02).x(gauge::COMPASS), c + Vec3::new(0.0, 0.0, 0.0322), Vec3::X, Vec3::Y, 0.026, 0.017);
    m.tube(&BLACK, &[c + Vec3::Y * 0.025, c + Vec3::new(0.0, 0.06, -0.01)], &|_| 0.006, 8, false);
    m.screw(&SCREW_BLACK, c + Vec3::new(0.026, -0.022, 0.0322), Vec3::Z, 0.0022, 1.0);
    m.screw(&SCREW_BLACK, c + Vec3::new(-0.026, -0.022, 0.0322), Vec3::Z, 0.0022, 2.0);
    // Fire extinguisher in its bracket between the front seats.
    let (a, e) = (Vec3::new(0.0, -0.535, -0.56), Vec3::new(0.0, -0.535, -0.28));
    m.tube(&mt(mat::PAINT, 0.45, 0.03, 0.02), &[a, e], &|t| if t < 0.05 || t > 0.95 { 0.03 } else { 0.034 }, 18, true);
    m.lathe(&BLACK, e, Vec3::Z, &[[0.02, 0.0], [0.015, 0.03], [0.008, 0.034]], 12, true);
    m.rbox(&BLACK, e + Vec3::new(0.0, 0.03, 0.025), Vec3::new(0.006, 0.006, 0.035), 0.003, Quat::from_rotation_x(-0.3));
    m.rbox(&mt(mat::PAINT, 0.7, 0.68, 0.6).pat(pat::PLACARD), a.lerp(e, 0.45) + Vec3::Y * 0.0342, Vec3::new(0.018, 0.0004, 0.05), 0.0002, Quat::IDENTITY);
    for t in [0.2f32, 0.8] {
        strap(m, &ALU, a.lerp(e, t) + Vec3::new(-0.04, -0.035, 0.0), a.lerp(e, t) + Vec3::new(0.04, -0.035, 0.0), Vec3::Y, 0.02, 0.002);
    }
}

// ---------------------------------------------------------------- odds and ends

/// The things that make it somebody's aeroplane: the outside air thermometer
/// poking through the windshield, the overhead console, a headset and a
/// checklist left on the right seat, the parking brake and cowl flap handles.
fn extras(m: &mut Mesh) {
    // Outside air thermometer: a dial on a stem through the top left of the windshield.
    let gp = Vec3::new(-0.36, fuse_top_y(-1.2, -0.36), -1.2);
    let n_in = Vec3::new(0.0, -0.89, 0.455).normalize();
    let face = (EYE - (gp + n_in * 0.03)).normalize();
    let c = gp + n_in * 0.03;
    let up = (Vec3::Y - face * face.y).normalize();
    m.tube(&CHROME, &[gp - n_in * 0.05, c - face * 0.006], &|_| 0.0035, 8, true);
    m.lathe(&CHROME, gp, n_in, &[[0.009, -0.002], [0.009, 0.004], [0.0, 0.004]], 14, false);
    m.lathe(&BLACK, c - face * 0.012, face, &[[0.004, 0.0], [0.025, 0.002], [0.026, 0.011], [0.0255, 0.0125], [0.022, 0.013]], 28, false);
    m.disc(&mt(mat::GAUGE, 0.02, 0.02, 0.02).x(gauge::OAT), c + face * 0.0005, face, up, 0.0215, 32);

    // Overhead console: speaker grille, map light, two switches, a placard.
    let (z0, z1) = (-0.88f32, -0.60f32);
    let top = inner(-0.75).top;
    let con = mt(mat::PLASTIC, 0.28, 0.27, 0.255).pat(pat::PANEL);
    m.rbox(&con, Vec3::new(0.0, top - 0.012, (z0 + z1) * 0.5), Vec3::new(0.085, 0.016, (z1 - z0) * 0.5), 0.008, Quat::IDENTITY);
    let fy = top - 0.0285;
    for i in 0..7 {
        for j in 0..5 {
            let p = Vec3::new(-0.045 + j as f32 * 0.0125 + (i % 2) as f32 * 0.006, fy, -0.70 + i as f32 * 0.011);
            m.disc(&BLACK, p, Vec3::NEG_Y, Vec3::Z, 0.0035, 8);
        }
    }
    m.lathe(&mt(mat::PLASTIC, 0.6, 0.6, 0.57), Vec3::new(0.045, fy, -0.82), Vec3::NEG_Y, &[[0.016, 0.0], [0.016, 0.004], [0.012, 0.008], [0.0, 0.009]], 18, true);
    m.lathe(&CHROME, Vec3::new(0.045, fy - 0.009, -0.82), Vec3::NEG_Y, &[[0.006, 0.0], [0.006, 0.004], [0.0, 0.005]], 10, true);
    for (k, x) in [-0.055f32, -0.03].iter().enumerate() {
        let p = Vec3::new(*x, fy, -0.82);
        m.lathe(&CHROME, p, Vec3::NEG_Y, &[[0.006, 0.0], [0.006, 0.002], [0.004, 0.003], [0.0, 0.003]], 12, false);
        let lean = if k == 0 { 0.35 } else { -0.35 };
        let d = Quat::from_rotation_x(lean) * Vec3::NEG_Y;
        m.tube(&CHROME, &[p, p + d * 0.014], &|t| 0.0022 - 0.0007 * t, 8, true);
    }
    m.rbox(&PAINT_WHITE.pat(pat::PLACARD), Vec3::new(-0.0425, fy - 0.0002, -0.795), Vec3::new(0.022, 0.0003, 0.006), 0.0002, Quat::IDENTITY);

    // A headset left on the right seat: cups face down, the band lying flat
    // between them, the mic boom forward and the cord trailing off the cushion.
    let seat_y = -0.36 + 0.045 + 0.004;
    let cup = mt(mat::PLASTIC, 0.06, 0.062, 0.068);
    let seal = mt(mat::FABRIC, 0.02, 0.02, 0.022);
    let (ca, cb) = (Vec3::new(0.19, seat_y, -0.70), Vec3::new(0.35, seat_y, -0.64));
    for c in [ca, cb] {
        m.lathe(&seal, c, Vec3::Y, &[[0.0, 0.0], [0.042, 0.0], [0.047, 0.008], [0.044, 0.016]], 22, false);
        m.lathe(&cup, c + Vec3::Y * 0.014, Vec3::Y, &[[0.043, 0.0], [0.045, 0.012], [0.04, 0.03], [0.026, 0.04], [0.0, 0.042]], 22, false);
        m.disc(&mt(mat::PAINT, 0.5, 0.5, 0.5).pat(pat::PLACARD), c + Vec3::Y * 0.0565, Vec3::Y, Vec3::Z, 0.012, 14);
    }
    let mid = (ca + cb) * 0.5;
    let side = (cb - ca).normalize();
    let bow = Vec3::Y.cross(side).normalize() * -1.0;
    let band: Vec<Vec3> = (0..=16).map(|k| {
        let t = k as f32 / 16.0;
        let a = (t - 0.5) * PI;
        mid + side * (a.sin() * 0.085) + bow * (a.cos() * 0.10) + Vec3::Y * 0.03
    }).collect();
    m.tube(&cup, &band, &|_| 0.0055, 8, true);
    m.tube(&seal, &band[4..13], &|_| 0.011, 10, true);
    let boom: Vec<Vec3> = [(0.0, 0.03, 0.0), (-0.03, 0.028, -0.04), (-0.04, 0.02, -0.10), (-0.025, 0.012, -0.14)].iter().map(|&(x, y, z)| ca + Vec3::new(x, y, z)).collect();
    m.tube(&BLACK, &boom, &|_| 0.0022, 6, true);
    m.dome(&seal, *boom.last().unwrap(), Vec3::new(0.3, 0.2, -1.0).normalize(), 0.011, 0.016, 12, 0.0);
    let cord: Vec<Vec3> = [(0.0, 0.01, 0.03), (0.03, -0.003, 0.09), (-0.05, -0.004, 0.13), (-0.10, -0.004, 0.06), (-0.16, -0.01, -0.01), (-0.17, -0.07, -0.05), (-0.165, -0.16, -0.08)]
        .iter().map(|&(x, y, z)| ca + Vec3::new(x, y, z)).collect();
    m.tube(&BLACK, &cord, &|_| 0.0028, 6, true);

    // A laminated checklist beside it.
    let card = Quat::from_rotation_y(0.35);
    let cc = Vec3::new(0.33, seat_y + 0.0015, -0.80);
    m.rbox(&mt(mat::PLASTIC, 0.75, 0.74, 0.70).pat(pat::PLACARD), cc, Vec3::new(0.055, 0.0012, 0.08), 0.001, card);
    m.rbox(&mt(mat::PLASTIC, 0.04, 0.12, 0.35), cc + card * Vec3::new(0.0, 0.0006, -0.068), Vec3::new(0.0545, 0.0008, 0.011), 0.0006, card);

    // Parking brake: a T handle under the left of the panel.
    let pb = Vec3::new(-0.40, -0.265, PANEL_Z + 0.01);
    m.tube(&CHROME, &[pb - Vec3::Z * 0.03, pb + Vec3::Z * 0.02], &|_| 0.0035, 8, false);
    m.rbox(&mt(mat::PLASTIC, 0.35, 0.03, 0.02), pb + Vec3::Z * 0.026, Vec3::new(0.028, 0.007, 0.007), 0.005, Quat::IDENTITY);
    m.rbox(&PAINT_WHITE.pat(pat::PLACARD), pb + Vec3::new(0.0, 0.016, -0.009), Vec3::new(0.02, 0.0045, 0.0004), 0.0002, Quat::IDENTITY);
    // Cowl flaps: a lever in a gated slot low on the pedestal.
    let rot = Quat::from_rotation_x(0.12);
    let cf = Vec3::new(0.0, -0.53, PANEL_Z + 0.01);
    m.rbox(&BLACK, cf, Vec3::new(0.008, 0.035, 0.003), 0.002, rot);
    m.tube(&CHROME, &[cf, cf + Vec3::new(0.0, 0.01, 0.04)], &|_| 0.003, 8, false);
    m.rbox(&mt(mat::PLASTIC, 0.03, 0.03, 0.03), cf + Vec3::new(0.0, 0.012, 0.045), Vec3::new(0.018, 0.006, 0.01), 0.004, rot);
    m.rbox(&PAINT_WHITE.pat(pat::PLACARD), cf + Vec3::new(0.022, 0.0, 0.004), Vec3::new(0.01, 0.03, 0.0004), 0.0002, rot);
}

// ---------------------------------------------------------------- controls

fn controls(b: &mut Builder) {
    // Yokes on their shafts.
    for xs in [-0.27f32, 0.27] {
        let mut y = Mesh::default();
        let c = Vec3::new(xs, -0.06, -1.06);
        y.tube(&CHROME, &[Vec3::new(xs, -0.06, PANEL_Z - 0.14), c - Vec3::Z * 0.02], &|_| 0.0115, 14, false);
        let path: Vec<Vec3> = [(-0.132, 0.085), (-0.142, 0.04), (-0.135, 0.0), (-0.105, -0.03), (-0.05, -0.042), (0.05, -0.042), (0.105, -0.03), (0.135, 0.0), (0.142, 0.04), (0.132, 0.085)]
            .iter()
            .map(|&(dx, dy)| c + Vec3::new(dx, dy, 0.012))
            .collect();
        y.tube(&KNOB, &path, &|t| 0.0155 + 0.004 * (1.0 - (t * PI).sin()), 14, true);
        y.rbox(&KNOB, c + Vec3::new(0.0, -0.03, 0.006), Vec3::new(0.055, 0.032, 0.022), 0.014, Quat::IDENTITY);
        y.disc(&mt(mat::ALU, 0.6, 0.6, 0.62), c + Vec3::new(0.0, -0.03, 0.0285), Vec3::Z, Vec3::Y, 0.014, 20);
        y.rbox(&PAINT_WHITE.pat(pat::PLACARD), c + Vec3::new(0.0, -0.005, 0.0285), Vec3::new(0.02, 0.005, 0.0004), 0.0002, Quat::IDENTITY);
        // Push-to-talk and trim switch on the left horn.
        y.dome(&mt(mat::PLASTIC, 0.35, 0.03, 0.03), c + Vec3::new(-0.13, 0.095, 0.012), Vec3::new(0.0, 1.0, 0.2), 0.006, 0.004, 10, 0.0);
        y.rbox(&mt(mat::PLASTIC, 0.08, 0.08, 0.08), c + Vec3::new(-0.115, 0.06, 0.028), Vec3::new(0.004, 0.008, 0.004), 0.002, Quat::IDENTITY);
        y.screw(&SCREW_BLACK, c + Vec3::new(0.04, -0.055, 0.0285), Vec3::Z, 0.0022, xs);
        y.screw(&SCREW_BLACK, c + Vec3::new(-0.04, -0.055, 0.0285), Vec3::Z, 0.0022, xs + 1.0);
        b.add("yoke", G_FUSE, y, Anim::turn_slide(Drive::new(Input::Roll, -0.6), Drive::new(Input::Pitch, 0.06), Vec3::Z), c, Vec3::Z, None);
        // The shaft collar where it comes through the panel.
        let mut col = Mesh::default();
        col.lathe(&BLACK, Vec3::new(xs, -0.06, PANEL_Z), Vec3::Z, &[[0.03, 0.0], [0.03, 0.004], [0.016, 0.008], [0.013, 0.008]], 24, false);
        b.fixed("yoke collar", G_FUSE, col);

        // Rudder pedals with toe brakes, hanging from a pivot under the panel.
        for dx in [-0.085f32, 0.085] {
            let mut p = Mesh::default();
            let pv = Vec3::new(xs + dx, -0.27, -1.66);
            let pc = Vec3::new(xs + dx, -0.47, -1.585);
            p.tube(&GREY, &[pv, pc], &|_| 0.009, 8, true);
            let tilt = Quat::from_rotation_x(-0.45);
            p.rbox(&ALU, pc + Vec3::new(0.0, 0.0, 0.008), Vec3::new(0.034, 0.06, 0.005), 0.004, tilt);
            p.rbox(&RUBBER.pat(pat::KNURL), pc + Vec3::new(0.0, -0.005, 0.0145), Vec3::new(0.03, 0.04, 0.0025), 0.002, tilt);
            p.tube(&CHROME, &[pc + Vec3::new(0.0, 0.05, 0.0), pc + Vec3::new(0.0, 0.10, -0.08)], &|_| 0.005, 8, true);
            b.add("pedal", G_FUSE, p, Anim::hinge(Drive::new(Input::Yaw, dx.signum() * 0.2)), pv, Vec3::X, None);
        }
    }
    // Throttle.
    let mut t = Mesh::default();
    t.tube(&CHROME, &[Vec3::new(0.0, -0.195, PANEL_Z - 0.09), Vec3::new(0.0, -0.195, PANEL_Z + 0.02)], &|_| 0.005, 12, false);
    t.lathe(&KNOB, Vec3::new(0.0, -0.195, PANEL_Z + 0.02), Vec3::Z, &[[0.005, 0.0], [0.017, 0.006], [0.019, 0.02], [0.017, 0.027], [0.0, 0.028]], 22, true);
    b.add("throttle", G_FUSE, t, Anim::slide(Drive::new(Input::Throttle, -0.06).offset(0.06), Vec3::Z), Vec3::ZERO, Vec3::Z, None);
    // Gear handle: a lever with a wheel-shaped knob.
    let mut g = Mesh::default();
    let gp = Vec3::new(-0.09, -0.185, PANEL_Z);
    g.tube(&CHROME, &[gp, gp + Vec3::Z * 0.05], &|_| 0.004, 8, false);
    g.lathe(&mt(mat::PLASTIC, 0.85, 0.85, 0.83), gp + Vec3::new(0.0, -0.007, 0.05), Vec3::Y, &[[0.0, 0.0], [0.013, 0.0], [0.015, 0.003], [0.015, 0.011], [0.013, 0.014], [0.0, 0.014]], 20, false);
    b.add("gear handle", G_FUSE, g, Anim::hinge(Drive::new(Input::GearHandle, 0.45)), gp, Vec3::X, None);
    // Flap lever with its airfoil-shaped knob.
    let mut fl = Mesh::default();
    let fp = Vec3::new(0.15, -0.195, PANEL_Z - 0.01);
    fl.tube(&CHROME, &[fp, fp + Vec3::Z * 0.035], &|_| 0.0035, 8, false);
    let foil: Vec<Vec2> = (0..20).map(|k| { let a = k as f32 / 20.0 * TAU; Vec2::new(0.016 * a.cos(), 0.006 * a.sin() * (1.0 - 0.5 * a.cos())) }).collect();
    fl.slab(&PAINT_WHITE, &foil, &[], fp + Vec3::new(0.0, 0.0, 0.045), Vec3::X, Vec3::Y, 0.01);
    b.add("flap lever", G_FUSE, fl, Anim::hinge(Drive::new(Input::FlapLever, 0.7).offset(-0.35)), fp, Vec3::X, None);
    // Elevator trim wheel.
    let mut w = Mesh::default();
    let wc = Vec3::new(0.0, -0.40, PANEL_Z + 0.0);
    w.lathe(&BLACK.pat(pat::KNURL), wc - Vec3::X * 0.008, Vec3::X, &[[0.0, 0.0], [0.058, 0.0], [0.062, 0.004], [0.062, 0.012], [0.058, 0.016], [0.0, 0.016]], 40, false);
    b.add("trim wheel", G_FUSE, w, Anim::hinge(Drive::new(Input::Trim, 9.0)), wc, Vec3::X, None);
}

// ---------------------------------------------------------------- glass

fn glass(b: &mut Builder) {
    let (z0, z1) = (-1.80f32, 1.56f32);
    let nz = 140;
    let nt = 70;
    let rows: Vec<Vec<_>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            (0..=nt).map(|k| { let p = fuse_pt(z, k as f32 / nt as f32 * (FRAC_PI_2 + 0.4)); (p, [z, p.y], 0.0) }).collect()
        })
        .collect();
    let mut g = Mesh::default();
    g.grid(&mt(mat::GLASS, 0.0, 0.0, 0.0), &rows, false, false);
    // Keep only what lies over an opening.
    let keep: Vec<bool> = g.v.iter().map(|v| window(Vec3::from(v.pos)) < 0.02).collect();
    let idx: Vec<u32> = g.i.chunks(3).filter(|t| t.iter().any(|&i| keep[i as usize])).flatten().copied().collect();
    g.i = idx;
    let l = g.mirror_x();
    g.append(&l);
    let k = b.fixed("windows", G_FUSE, g);
    b.pieces[k].glass = true;
    b.pieces[k].occluder = false;
}

// ---------------------------------------------------------------- what the eye sees

/// The lining's half width on a grid of stations and heights, for quick tests.
struct Shell {
    z0: f32,
    y0: f32,
    nz: usize,
    ny: usize,
    half: Vec<f32>,
}
const SHELL_STEP: f32 = 0.02;
fn shell() -> &'static Shell {
    static S: std::sync::OnceLock<Shell> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let (z0, y0) = (-2.06f32, FLOOR_Y);
        let nz = ((1.60 - z0) / SHELL_STEP) as usize + 1;
        let ny = ((0.75 - y0) / SHELL_STEP) as usize + 1;
        let mut half = vec![0.0; nz * ny];
        for i in 0..nz {
            let z = z0 + i as f32 * SHELL_STEP;
            let top = inner(z).top;
            for j in 0..ny {
                let y = y0 + j as f32 * SHELL_STEP;
                half[i * ny + j] = if y >= top { -1.0 } else { inner_x(z, y) };
            }
        }
        Shell { z0, y0, nz, ny, half }
    })
}

/// Whether a look from `eye` along `d` (body frame) ends on something in the
/// cabin (true) or goes out through the glass (false).
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let s = shell();
    let mut t = 0.02;
    while t < 3.5 {
        let p = eye + d * t;
        if p.y < FLOOR_Y + 0.01 {
            return true;
        }
        // The panel, and the glareshield over it.
        if p.z < PANEL_Z + 0.01 {
            let g = ((PANEL_Z + 0.025 - p.z) / 0.46).clamp(0.0, 1.0);
            if p.y < 0.185 + 0.12 * g * g * g {
                return true;
            }
        }
        let i = ((p.z - s.z0) / SHELL_STEP).round();
        let j = ((p.y - s.y0) / SHELL_STEP).round();
        if i < 0.0 || i >= s.nz as f32 || j >= s.ny as f32 {
            return window(p) >= 0.0;
        }
        let half = s.half[i as usize * s.ny + j as usize];
        if p.x.abs() > half || p.z > 1.55 || p.z < -2.03 {
            return window(p) >= 0.0;
        }
        t += 0.015 + t * 0.01;
    }
    true
}
