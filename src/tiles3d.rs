//! Google Photorealistic 3D Tiles near the aircraft: photogrammetry meshes of
//! real cities in place of our own ground, buildings, trees and cars, within a
//! zone of about ten kilometres. Off by default (it needs the player's key, see
//! gmaps.rs). At night the mesh is lit by our street lamps and given lit
//! windows from our map's buildings (photonight.rs, photo.wgsl).
//!
//! The data is an OGC 3D Tiles tree (JSON) whose nodes carry a bounding box, a
//! geometric error in metres and either a glTF tile (one mesh; one to three JPEGs, one per primitive) or a
//! link to a further JSON subtree. Refinement is REPLACE: a node is swapped for
//! all its children once its error spans too many pixels on screen, and only
//! when every child is ready, so nothing goes missing while they load. Nodes
//! coarser than `E_DRAW` are never drawn: there our own world is better.
//!
//! Positions are Earth-centred (ECEF) on the WGS84 ellipsoid; each vertex is
//! converted to latitude, longitude and ellipsoid height and placed on our
//! sphere (geo.rs) at that height. Ellipsoid heights differ from the sea-level
//! heights of our terrain by the geoid (about -32 m in California), so the
//! difference is measured, mesh against terrain, and the mesh is lifted by it
//! (`datum`) when it is drawn and when it is collided with.
//!
//! Every frame the drawn tiles are also rendered top-down into a coverage
//! mask (render.rs); our own terrain, buildings and so on give way wherever the
//! mask is set (common.wgsl, `photo_hides`), and across a band at the edge of
//! the zone the two are dithered into each other.
//!
//! Google's terms allow caching only as the HTTP headers say (the root is
//! max-age=0), so nothing is kept on disk: tiles live in memory, least
//! recently used first out.
use crate::geo;
use crate::gmaps::{Detail, Google};
use crate::render::{PhotoGpu, Renderer};
use crate::world::Frustum;
use anyhow::{anyhow, bail, Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{DMat4, DVec3, Mat4, Vec3};
use parking_lot::{Condvar, Mutex};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

/// Coarsest node drawn (m of geometric error): coarser ones only lead on to
/// their children. Tiles this coarse are drawn some 7-14 km out.
const E_DRAW: f64 = 140.0;
/// Largest radius of the zone around the aircraft (m; see `Params`).
pub const ZONE_R: f64 = 9000.0;
/// Half the width of the coverage mask (it is re-centred when the aircraft
/// has moved this margin past where it was laid).
pub const MASK_HALF: f64 = ZONE_R + 2500.0;
/// Fetch threads; `Params::fetches` of them work at a time. Fewer on a
/// phone: each holds a decoded mesh in RAM while it lands.
#[cfg(target_os = "android")]
const WORKERS: usize = 4;
#[cfg(not(target_os = "android"))]
const WORKERS: usize = 12;
/// Requests waiting at most (the closest first); the rest wait a frame.
#[cfg(target_os = "android")]
const MAX_QUEUE: usize = 24;
#[cfg(not(target_os = "android"))]
const MAX_QUEUE: usize = 64;
/// Bytes copied to the GPU per frame at most.
const UPLOAD_BUDGET: usize = 12 << 20;
/// A root request opens a session good for about three hours.
const SESSION_S: f64 = 2.75 * 3600.0;
/// Seconds over which the physics moves onto a newly arrived tile (so
/// nothing standing on the ground is kicked by a step in height).
const RAMP_S: f64 = 2.5;
/// The photographer's sun is taken this high (degrees): only its azimuth can
/// be read off the walls, and aerial photography is flown about midday.
const BAKED_ELEV: f64 = 50.0;
/// Mesh tiles the physics may stand on: finer than this and this close.
const COL_ERROR: f64 = 70.0;
const COL_DIST: f64 = 3000.0;
/// Nodes kept at most before subtrees not used for a minute are dropped.
const MAX_NODES: usize = 60_000;
const UNUSED_FRAMES: u64 = 3600;
/// Seconds over which a tile dissolves into its children (or its parent)
/// when the level of detail changes; `FABLE_LOD_FADE=S` overrides it, 0 swaps
/// at once as before.
const FADE_S: f64 = 0.5;

/// What a detail level streams.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// a node is split once its geometric error covers more pixels than this
    pub sse_px: f64,
    /// radius of the zone around the aircraft (m)
    pub zone_r: f64,
    /// GPU memory for tiles; least recently drawn ones go first beyond it
    pub gpu_budget: usize,
    /// requests in flight at most
    pub fetches: usize,
}

impl Params {
    pub fn of(d: Detail) -> Self {
        let (sse_px, zone_r, mb, fetches) = match d {
            // fetches is capped by WORKERS at runtime; on a phone fewer
            // threads exist than the busiest detail level asks for, and
            // the rest of the queue simply waits a turn. The GPU budget
            // (the photo tiles' share of memory) is halved there too.
            Detail::Low => (32.0, 4000.0, 256, 3),
            Detail::Medium => (24.0, 6000.0, 400, 5),
            Detail::High => (16.0, ZONE_R, 640, 8),
            Detail::Ultra => (11.0, ZONE_R, 1024, 12),
        };
        let mb = if cfg!(target_os = "android") { mb / 2 } else { mb };
        Self { sse_px, zone_r, gpu_budget: mb << 20, fetches: fetches.min(WORKERS) }
    }

    /// The band at the zone's edge over which the mesh dissolves into our
    /// own world (m): 1.6 km at the full 9 km.
    pub fn band(&self) -> f64 {
        self.zone_r * (1600.0 / ZONE_R)
    }
}

// ------------------------------------------------------------ WGS84

const WA: f64 = 6_378_137.0;
const WF: f64 = 1.0 / 298.257_223_563;

/// Earth-centred position of a latitude, longitude (radians) and ellipsoid height.
pub fn lla_to_ecef(lat: f64, lon: f64, h: f64) -> DVec3 {
    let e2 = WF * (2.0 - WF);
    let (sl, cl) = lat.sin_cos();
    let (so, co) = lon.sin_cos();
    let n = WA / (1.0 - e2 * sl * sl).sqrt();
    DVec3::new((n + h) * cl * co, (n + h) * cl * so, (n * (1.0 - e2) + h) * sl)
}

/// Latitude, longitude (radians) and ellipsoid height (m) of an Earth-centred
/// position (Bowring's method: well under a millimetre near the surface).
pub fn ecef_to_lla(p: DVec3) -> (f64, f64, f64) {
    let e2 = WF * (2.0 - WF);
    let b = WA * (1.0 - WF);
    let ep2 = (WA * WA - b * b) / (b * b);
    let r = (p.x * p.x + p.y * p.y).sqrt();
    let th = (p.z * WA).atan2(r * b);
    let (st, ct) = th.sin_cos();
    let lat = (p.z + ep2 * b * st * st * st).atan2(r - e2 * WA * ct * ct * ct);
    let (sl, cl) = lat.sin_cos();
    let h = r * cl + p.z * sl - WA * (1.0 - e2 * sl * sl).sqrt();
    (lat, p.y.atan2(p.x), h)
}

/// Our planet frame for an Earth-centred position: the same latitude,
/// longitude and height, on our sphere.
fn ecef_to_ours(p: DVec3) -> DVec3 {
    let (lat, lon, h) = ecef_to_lla(p);
    geo::lla_to_pos(lat, lon, h)
}

// ------------------------------------------------------------ the tree

/// Oriented box: centre and three half-axes (ECEF).
#[derive(Clone, Copy, Debug)]
struct Obb {
    c: DVec3,
    h: [DVec3; 3],
}
impl Obb {
    /// Distance from a point to the box (0 inside).
    fn dist(&self, p: DVec3) -> f64 {
        let d = p - self.c;
        let mut s = 0.0;
        for u in &self.h {
            let l = u.length();
            if l > 1e-9 {
                let t = (d.dot(*u) / l).abs() - l;
                if t > 0.0 {
                    s += t * t;
                }
            }
        }
        s.sqrt()
    }
    fn radius(&self) -> f64 {
        (self.h[0].length_squared() + self.h[1].length_squared() + self.h[2].length_squared()).sqrt()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    /// only leads on to its children
    Group,
    /// a glTF tile
    Glb(String),
    /// a JSON subtree whose root becomes this node's only child
    Json(String),
}

enum State {
    Idle,
    Queued,
    /// decoded, waiting for its turn to be copied to the GPU
    Decoded(Box<MeshData>),
    Ready(Box<Loaded>),
    /// subtree attached (Json nodes)
    Linked,
    /// failed at this frame; tried again later, a few times
    Failed(u64),
}

struct Node {
    obb: Obb,
    /// bounding sphere in our frame
    sph_c: DVec3,
    sph_r: f64,
    error: f64,
    kind: Kind,
    state: State,
    fails: u8,
    children: Vec<u32>,
    last_used: u64,
    /// the node above (ROOT for the root)
    parent: u32,
    /// Level-of-detail crossfade (see `crossfade`): how far it has faded in,
    /// or while leaving (`out`) how much of it is left; the frame it was last
    /// shown; and frame stamps for "in the chosen set", "something under it
    /// was shown", "something under it is fading in" (now and as of the
    /// last frame: `busy`).
    fade: f32,
    out: bool,
    vis: u64,
    sel: u64,
    under: u64,
    inc_below: u64,
    out_below: u64,
    busy: u64,
}

/// A tile on the GPU, plus what the physics needs of it.
struct Loaded {
    gpu: PhotoGpu,
    col: ColMesh,
    /// texture coordinates of `col`'s vertices, and a small copy of the
    /// texture: what a destroyed building's pieces take of the tile (`take_building`)
    uv: Vec<[f32; 2]>,
    tiny: Arc<Tiny>,
    bytes: usize,
    copyright: Vec<String>,
    /// seconds (Photo::t0) it arrived
    at: f64,
    /// the photographer's sun as its walls show it (relight::wall_sun)
    sun: (Vec3, f32),
    /// obstruction lights on its tall buildings (photonight.rs)
    beacons: Vec<crate::photonight::Beacon>,
}

/// A parsed subtree, before it joins the arena.
struct PNode {
    obb: Obb,
    error: f64,
    kind: Kind,
    children: Vec<PNode>,
}

/// The session parameter in a URL, if any.
fn session_of(url: &str) -> Option<&str> {
    let q = url.split_once('?')?.1;
    q.split('&').find_map(|kv| kv.strip_prefix("session="))
}

/// A content URI as a path and query on tile.googleapis.com, relative to the
/// tileset it came from, carrying the session along when it has none.
fn resolve(base: &str, uri: &str, session: Option<&str>) -> String {
    let uri = match uri.find("://") {
        Some(i) => uri[i + 3..].find('/').map(|j| &uri[i + 3 + j..]).unwrap_or("/"),
        None => uri,
    };
    let mut path = if uri.starts_with('/') {
        uri.to_string()
    } else {
        let dir = base.split('?').next().unwrap_or("");
        format!("{}{uri}", &dir[..dir.rfind('/').map(|i| i + 1).unwrap_or(0)])
    };
    if session_of(&path).is_none() {
        if let Some(s) = session {
            path.push(if path.contains('?') { '&' } else { '?' });
            path.push_str("session=");
            path.push_str(s);
        }
    }
    path
}

fn parse_node(j: &serde_json::Value, base: &str, session: Option<&str>) -> Option<PNode> {
    let b = j["boundingVolume"]["box"].as_array()?;
    if b.len() != 12 {
        return None;
    }
    let f: Vec<f64> = b.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect();
    let v = |i: usize| DVec3::new(f[i], f[i + 1], f[i + 2]);
    let obb = Obb { c: v(0), h: [v(3), v(6), v(9)] };
    let kind = match j["content"]["uri"].as_str().or(j["content"]["url"].as_str()) {
        Some(u) => {
            let p = resolve(base, u, session);
            if p.split('?').next().unwrap_or("").ends_with(".json") {
                Kind::Json(p)
            } else {
                Kind::Glb(p)
            }
        }
        None => Kind::Group,
    };
    let children = j["children"].as_array().map(|c| c.iter().filter_map(|c| parse_node(c, base, session)).collect()).unwrap_or_default();
    Some(PNode { obb, error: j["geometricError"].as_f64().unwrap_or(0.0), kind, children })
}

/// A tileset JSON fetched from `url` (path and query).
fn parse_tileset(data: &[u8], url: &str) -> Result<PNode> {
    let j: serde_json::Value = serde_json::from_slice(data).context("tileset json")?;
    parse_node(&j["root"], url, session_of(url)).ok_or_else(|| anyhow!("tileset without a root box"))
}

// ------------------------------------------------------------ decoded tiles

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PVertex {
    /// relative to the tile's origin, our planet frame
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    /// smoothed normal, snorm
    pub nrm: [i8; 4],
}

/// A tile as the workers hand it over.
pub struct MeshData {
    pub verts: Vec<PVertex>,
    pub idx: Vec<u32>,
    /// RGBA mip levels: width, height, texels
    pub mips: Vec<(u32, u32, Vec<u8>)>,
    origin: DVec3,
    col: ColMesh,
    copyright: Vec<String>,
    /// (lat, lon, ellipsoid height) of a spread of vertices, to measure the datum
    samples: Vec<[f64; 3]>,
    /// where the photographer's sun stood, as its walls show it (relight::wall_sun)
    sun: (Vec3, f32),
    beacons: Vec<crate::photonight::Beacon>,
}
impl MeshData {
    pub fn gpu_bytes(&self) -> usize {
        self.verts.len() * std::mem::size_of::<PVertex>() + self.idx.len() * 4 + self.mips.iter().map(|m| m.2.len()).sum::<usize>()
    }
}

/// Triangles of a tile on its own east / north / up axes, bucketed in a grid
/// seen from above, for vertical rays.
struct ColMesh {
    o: DVec3,
    axes: [DVec3; 3],
    v: Vec<Vec3>,
    t: Vec<[u32; 3]>,
    min: [f32; 2],
    max: [f32; 2],
    cell: f32,
    nx: usize,
    ny: usize,
    start: Vec<u32>,
    list: Vec<u32>,
}

/// The collision tiles round a group of our cars (finest first), to put
/// their lights on the mesh's street (`Photo::streets`).
pub struct Streets<'a> {
    photo: &'a Photo,
    /// Each tile's footprint box (origin, east, north, min, max) side by
    /// side, so that passing over the tiles that miss a car stays in cache.
    boxes: Vec<(DVec3, DVec3, DVec3, [f32; 4])>,
    tiles: Vec<&'a ColMesh>,
    datum: f64,
}
impl Streets<'_> {
    /// Height (m, datum applied) of the street under a car of ours at `p`
    /// whose road is at `h`: the mesh's highest surface no more than 3 m
    /// above the road, so tree crowns, awnings and bridges over it are passed
    /// by; but where that is more than 4 m down (the mesh has an elevated
    /// road higher than ours), the lowest surface above it up to 8 m over the
    /// road, and failing that the road's own height. None
    /// where the mesh does not hide our scenery (outside the zone, in a
    /// destroyed building's outline) or no collision tile holds the point.
    pub fn at(&self, p: DVec3, h: f64) -> Option<f64> {
        if !self.photo.covers(p) || self.photo.cuts.iter().any(|c| c.contains(p)) {
            return None;
        }
        let h0 = h - self.datum;
        self.boxes.iter().zip(&self.tiles).find_map(|((o, e, n, b), t)| {
            let q = p - *o;
            let (x, y) = (q.dot(*e) as f32, q.dot(*n) as f32);
            if x < b[0] || y < b[1] || x > b[2] || y > b[3] {
                return None;
            }
            match t.surfaces(p, h0 + 3.0, h0 + 8.0) {
                (None, None) => None,
                (Some(b), _) if b > h0 - 4.0 => Some(b),
                (_, Some(a)) => Some(a),
                // Only ground far below (the mesh's deck is missing or much
                // higher): our own road's height.
                (Some(_), None) => Some(h0),
            }
        }).map(|t| t + self.datum)
    }
}

impl ColMesh {
    fn build(o: DVec3, pos: &[Vec3], idx: &[u32]) -> Self {
        let (e, n, u) = geo::enu(o);
        let (e3, n3, u3) = (e.as_vec3(), n.as_vec3(), u.as_vec3());
        let v: Vec<Vec3> = pos.iter().map(|p| Vec3::new(p.dot(e3), p.dot(n3), p.dot(u3))).collect();
        let t: Vec<[u32; 3]> = idx.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).filter(|c| c.iter().all(|&i| (i as usize) < v.len())).collect();
        let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in &v {
            min = [min[0].min(p.x), min[1].min(p.y)];
            max = [max[0].max(p.x), max[1].max(p.y)];
        }
        if v.is_empty() {
            (min, max) = ([0.0; 2], [0.0; 2]);
        }
        let side = ((t.len() as f32 / 3.0).sqrt().ceil() as usize).clamp(1, 64);
        let cell = ((max[0] - min[0]).max(max[1] - min[1]) / side as f32).max(0.5);
        let nx = (((max[0] - min[0]) / cell) as usize + 1).min(256);
        let ny = (((max[1] - min[1]) / cell) as usize + 1).min(256);
        let range = |a: f32, b: f32, c: f32, lo: f32, n: usize| -> (usize, usize) {
            let i0 = ((a.min(b).min(c) - lo) / cell).floor().max(0.0) as usize;
            let i1 = ((a.max(b).max(c) - lo) / cell).floor().max(0.0) as usize;
            (i0.min(n - 1), i1.min(n - 1))
        };
        let mut count = vec![0u32; nx * ny + 1];
        let cells = |tri: &[u32; 3], f: &mut dyn FnMut(usize)| {
            let (a, b, c) = (v[tri[0] as usize], v[tri[1] as usize], v[tri[2] as usize]);
            let (x0, x1) = range(a.x, b.x, c.x, min[0], nx);
            let (y0, y1) = range(a.y, b.y, c.y, min[1], ny);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    f(y * nx + x);
                }
            }
        };
        for tri in &t {
            cells(tri, &mut |k| count[k + 1] += 1);
        }
        for k in 0..nx * ny {
            count[k + 1] += count[k];
        }
        let start = count.clone();
        let mut list = vec![0u32; count[nx * ny] as usize];
        for (i, tri) in t.iter().enumerate() {
            cells(tri, &mut |k| {
                list[count[k] as usize] = i as u32;
                count[k] += 1;
            });
        }
        Self { o, axes: [e, n, u], v, t, min, max, cell, nx, ny, start, list }
    }

    /// Height above our sphere of the highest surface on the vertical through
    /// `p` (our frame), before the datum.
    fn top(&self, p: DVec3) -> Option<f64> {
        self.surfaces(p, f64::MAX, f64::MAX).0
    }

    /// The surfaces on the vertical through `p` (heights above our sphere,
    /// before the datum): the highest at or below `ceil`, and the lowest above
    /// it up to `hi`.
    fn surfaces(&self, p: DVec3, ceil: f64, hi: f64) -> (Option<f64>, Option<f64>) {
        let q = p - self.o;
        let (x, y) = (q.dot(self.axes[0]) as f32, q.dot(self.axes[1]) as f32);
        if x < self.min[0] || y < self.min[1] || x > self.max[0] || y > self.max[1] {
            return (None, None);
        }
        let foot = self.o + self.axes[0] * x as f64 + self.axes[1] * y as f64;
        // Limits in the tile's own z (its up axis is radial to within a
        // fraction of a degree, so a height is z plus that of z = 0).
        let base = if ceil < f64::MAX { foot.length() - geo::R } else { 0.0 };
        let (zc, zh) = ((ceil - base).min(f32::MAX as f64) as f32, (hi - base).min(f32::MAX as f64) as f32);
        let cx = (((x - self.min[0]) / self.cell) as usize).min(self.nx - 1);
        let cy = (((y - self.min[1]) / self.cell) as usize).min(self.ny - 1);
        let k = cy * self.nx + cx;
        let (mut below, mut above): (Option<f32>, Option<f32>) = (None, None);
        for &ti in &self.list[self.start[k] as usize..self.start[k + 1] as usize] {
            let [a, b, c] = self.t[ti as usize].map(|i| self.v[i as usize]);
            let d = (b.y - c.y) * (a.x - c.x) + (c.x - b.x) * (a.y - c.y);
            if d.abs() < 1e-6 {
                continue;
            }
            let l0 = ((b.y - c.y) * (x - c.x) + (c.x - b.x) * (y - c.y)) / d;
            let l1 = ((c.y - a.y) * (x - c.x) + (a.x - c.x) * (y - c.y)) / d;
            let l2 = 1.0 - l0 - l1;
            const EPS: f32 = -1e-4;
            if l0 >= EPS && l1 >= EPS && l2 >= EPS {
                let z = l0 * a.z + l1 * b.z + l2 * c.z;
                if z <= zc {
                    below = Some(below.map_or(z, |m| m.max(z)));
                } else if z <= zh {
                    above = Some(above.map_or(z, |m| m.min(z)));
                }
            }
        }
        let h = |z: f32| (foot + self.axes[2] * z as f64).length() - geo::R;
        (below.map(h), above.map(h))
    }

    /// Whether the tile's footprint (its own east/north box) comes within `r`
    /// of `p`.
    fn near(&self, p: DVec3, r: f64) -> bool {
        let q = p - self.o;
        let (x, y) = (q.dot(self.axes[0]), q.dot(self.axes[1]));
        x > self.min[0] as f64 - r && y > self.min[1] as f64 - r && x < self.max[0] as f64 + r && y < self.max[1] as f64 + r
    }
}

/// Texels of edge colour round each photograph packed into a tile's atlas,
/// so filtering and the first mip levels don't bleed in its neighbours.
const ATLAS_PAD: u32 = 4;

/// Packs photographs of the given sizes (each grown by `ATLAS_PAD` all round)
/// into one texture: the largest first, each next into the best free corner
/// left, else onto the right or the bottom, whichever keeps the area smaller.
/// Returns the atlas size and each one's corner (of its padded box).
fn pack(sizes: &[(u32, u32)]) -> (u32, u32, Vec<(u32, u32)>) {
    let pad = |s: (u32, u32)| (s.0 + 2 * ATLAS_PAD, s.1 + 2 * ATLAS_PAD);
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse((pad(sizes[i]).0 * pad(sizes[i]).1, pad(sizes[i]).1)));
    let mut at = vec![(0, 0); sizes.len()];
    let (mut w, mut h) = pad(sizes[order[0]]);
    // free rectangles: x, y, w, h
    let mut free: Vec<(u32, u32, u32, u32)> = Vec::new();
    for &i in &order[1..] {
        let (pw, ph) = pad(sizes[i]);
        let best = free.iter().enumerate().filter(|(_, f)| f.2 >= pw && f.3 >= ph).min_by_key(|(_, f)| f.2 * f.3).map(|(k, _)| k);
        if let Some(k) = best {
            let f = free.swap_remove(k);
            at[i] = (f.0, f.1);
            // Guillotine cut along the longer leftover.
            let (rw, rh) = (f.2 - pw, f.3 - ph);
            if rw > rh {
                free.push((f.0 + pw, f.1, rw, f.3));
                free.push((f.0, f.1 + ph, pw, rh));
            } else {
                free.push((f.0, f.1 + ph, f.2, rh));
                free.push((f.0 + pw, f.1, rw, ph));
            }
            free.retain(|f| f.2 > 0 && f.3 > 0);
            continue;
        }
        let right = ((w + pw) as u64) * (h.max(ph) as u64);
        let below = (w.max(pw) as u64) * ((h + ph) as u64);
        if right < below {
            at[i] = (w, 0);
            free.push((w, ph, pw, h.saturating_sub(ph)));
            free.push((0, h, w, ph.saturating_sub(h)));
            w += pw;
            h = h.max(ph);
        } else {
            at[i] = (0, h);
            free.push((pw, h, w.saturating_sub(pw), ph));
            free.push((w, 0, pw.saturating_sub(w), h));
            h += ph;
            w = w.max(pw);
        }
        free.retain(|f| f.2 > 0 && f.3 > 0);
    }
    (w, h, at)
}

/// Copies an RGBA photograph into an atlas `aw` wide at (ox, oy), its edge
/// texels repeated across the `ATLAS_PAD` margin round it.
fn blit_padded(dst: &mut [u8], aw: u32, ox: u32, oy: u32, w: u32, h: u32, src: &[u8]) {
    let p = ATLAS_PAD as i64;
    for y in 0..(h as i64 + 2 * p) {
        let sy = (y - p).clamp(0, h as i64 - 1) as usize;
        for x in 0..(w as i64 + 2 * p) {
            let sx = (x - p).clamp(0, w as i64 - 1) as usize;
            let s = (sy * w as usize + sx) * 4;
            let d = (((oy as i64 + y) * aw as i64 + ox as i64 + x) * 4) as usize;
            dst[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
}

/// Mip levels of an RGBA image of any size (box filter), finest first.
fn mips(w: u32, h: u32, data: Vec<u8>) -> Vec<(u32, u32, Vec<u8>)> {
    let mut out = vec![(w, h, data)];
    loop {
        let (w, h, d) = out.last().unwrap();
        let (w, h) = (*w as usize, *h as usize);
        if w == 1 && h == 1 {
            break;
        }
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let mut n = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                        s += d[(sy * w + sx) * 4 + c] as u32;
                    }
                    n[(y * nw + x) * 4 + c] = ((s + 2) / 4) as u8;
                }
            }
        }
        out.push((nw as u32, nh as u32, n));
    }
    out
}

/// Reads an accessor as floats (`comps` per element), normalising integer ones.
fn accessor_f32(j: &serde_json::Value, bin: &[u8], i: usize, comps: usize) -> Result<Vec<f32>> {
    let a = &j["accessors"][i];
    let bv = &j["bufferViews"][a["bufferView"].as_u64().ok_or_else(|| anyhow!("sparse accessor"))? as usize];
    let count = a["count"].as_u64().unwrap_or(0) as usize;
    let ct = a["componentType"].as_u64().unwrap_or(0);
    let norm = a["normalized"].as_bool().unwrap_or(false);
    let size = match ct {
        5126 => 4,
        5123 | 5122 => 2,
        5121 | 5120 => 1,
        _ => bail!("component type {ct}"),
    };
    let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(size * comps);
    if count > 0 && off + stride * (count - 1) + size * comps > bin.len() {
        bail!("accessor out of range");
    }
    let mut out = Vec::with_capacity(count * comps);
    for e in 0..count {
        for c in 0..comps {
            let p = off + e * stride + c * size;
            let b = &bin[p..p + size];
            out.push(match (ct, norm) {
                (5126, _) => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                (5123, true) => u16::from_le_bytes([b[0], b[1]]) as f32 / 65535.0,
                (5123, false) => u16::from_le_bytes([b[0], b[1]]) as f32,
                (5122, true) => (i16::from_le_bytes([b[0], b[1]]) as f32 / 32767.0).max(-1.0),
                (5122, false) => i16::from_le_bytes([b[0], b[1]]) as f32,
                (5121, true) => b[0] as f32 / 255.0,
                (5121, false) => b[0] as f32,
                (5120, true) => (b[0] as i8 as f32 / 127.0).max(-1.0),
                _ => b[0] as i8 as f32,
            });
        }
    }
    Ok(out)
}

fn accessor_idx(j: &serde_json::Value, bin: &[u8], i: usize) -> Result<Vec<u32>> {
    let a = &j["accessors"][i];
    let bv = &j["bufferViews"][a["bufferView"].as_u64().ok_or_else(|| anyhow!("sparse indices"))? as usize];
    let count = a["count"].as_u64().unwrap_or(0) as usize;
    let off = bv["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let size = match a["componentType"].as_u64() {
        Some(5121) => 1,
        Some(5123) => 2,
        Some(5125) => 4,
        c => bail!("index type {c:?}"),
    };
    let d = bin.get(off..off + count * size).ok_or_else(|| anyhow!("indices out of range"))?;
    Ok(match size {
        1 => d.iter().map(|&v| v as u32).collect(),
        2 => d.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as u32).collect(),
        _ => d.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
    })
}

fn node_matrix(n: &serde_json::Value) -> DMat4 {
    if let Some(m) = n["matrix"].as_array().filter(|m| m.len() == 16) {
        let f: Vec<f64> = m.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect();
        return DMat4::from_cols_slice(&f);
    }
    let v = |k: &str, d: &[f64]| n[k].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()).filter(|a| a.len() == d.len()).unwrap_or(d.to_vec());
    let (t, r, s) = (v("translation", &[0.0; 3]), v("rotation", &[0.0, 0.0, 0.0, 1.0]), v("scale", &[1.0; 3]));
    DMat4::from_scale_rotation_translation(DVec3::new(s[0], s[1], s[2]), glam::DQuat::from_xyzw(r[0], r[1], r[2], r[3]), DVec3::new(t[0], t[1], t[2]))
}

/// A glTF binary tile: its triangles placed in our frame, its texture with mips.
pub fn decode(glb: &[u8]) -> Result<MeshData> {
    decode_with(glb, None)
}

/// As `decode`, with the pavement smoothed (pave.rs) when `src` is given.
pub fn decode_with(glb: &[u8], src: Option<&crate::sources::Sources>) -> Result<MeshData> {
    let u32_at = |o: usize| glb.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| anyhow!("short glb"));
    if u32_at(0)? != 0x4654_6C67 {
        bail!("not a glb");
    }
    let (mut json, mut bin): (Option<serde_json::Value>, &[u8]) = (None, &[]);
    let mut o = 12;
    while o + 8 <= glb.len() {
        let (len, ty) = (u32_at(o)? as usize, u32_at(o + 4)?);
        let chunk = glb.get(o + 8..o + 8 + len).ok_or_else(|| anyhow!("short chunk"))?;
        match ty {
            0x4E4F_534A => json = Some(serde_json::from_slice(chunk)?),
            0x004E_4942 => bin = chunk,
            _ => {}
        }
        o += 8 + len;
    }
    let j = json.ok_or_else(|| anyhow!("glb without json"))?;
    // glTF is y-up; 3D Tiles are z-up (ECEF).
    let yup = DMat4::from_cols(DVec3::X.extend(0.0), DVec3::Z.extend(0.0), (-DVec3::Y).extend(0.0), glam::DVec4::W);
    let mut meshes: Vec<(DMat4, usize)> = Vec::new();
    let mut stack: Vec<(usize, DMat4)> = j["scenes"][j["scene"].as_u64().unwrap_or(0) as usize]["nodes"].as_array().map(|a| a.iter().filter_map(|n| n.as_u64()).map(|n| (n as usize, yup)).collect()).unwrap_or_default();
    while let Some((n, parent)) = stack.pop() {
        let node = &j["nodes"][n];
        let m = parent * node_matrix(node);
        if let Some(mi) = node["mesh"].as_u64() {
            meshes.push((m, mi as usize));
        }
        for c in node["children"].as_array().into_iter().flatten().filter_map(|c| c.as_u64()) {
            stack.push((c as usize, m));
        }
    }
    let mut ecef: Vec<DVec3> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    // Each primitive's texture (glTF image) and its run of vertices: a tile
    // holds up to three photographs, packed into one atlas below.
    let mut parts: Vec<(Option<usize>, std::ops::Range<usize>)> = Vec::new();
    for (m, mi) in meshes {
        for p in j["meshes"][mi]["primitives"].as_array().into_iter().flatten() {
            if p["mode"].as_u64().unwrap_or(4) != 4 {
                continue;
            }
            if !p["extensions"]["KHR_draco_mesh_compression"].is_null() {
                bail!("Draco-compressed tile (not supported)");
            }
            let tex = p["material"].as_u64().and_then(|mat| j["materials"][mat as usize]["pbrMetallicRoughness"]["baseColorTexture"]["index"].as_u64()).and_then(|t| j["textures"][t as usize]["source"].as_u64()).map(|s| s as usize);
            let pos = accessor_f32(&j, bin, p["attributes"]["POSITION"].as_u64().ok_or_else(|| anyhow!("no positions"))? as usize, 3)?;
            let n = pos.len() / 3;
            let uv = match p["attributes"]["TEXCOORD_0"].as_u64() {
                Some(a) => accessor_f32(&j, bin, a as usize, 2)?,
                None => vec![0.0; n * 2],
            };
            let base = ecef.len() as u32;
            match p["indices"].as_u64() {
                Some(a) => idx.extend(accessor_idx(&j, bin, a as usize)?.into_iter().filter(|&i| (i as usize) < n).map(|i| i + base)),
                None => idx.extend(base..base + n as u32),
            }
            idx.truncate(idx.len() / 3 * 3);
            for k in 0..n {
                ecef.push(m.transform_point3(DVec3::new(pos[k * 3] as f64, pos[k * 3 + 1] as f64, pos[k * 3 + 2] as f64)));
                uvs.push([uv.get(k * 2).copied().unwrap_or(0.0), uv.get(k * 2 + 1).copied().unwrap_or(0.0)]);
            }
            parts.push((tex, base as usize..ecef.len()));
        }
    }
    if ecef.is_empty() || idx.is_empty() {
        bail!("empty tile");
    }
    // Textures (JPEG, sometimes PNG). A third of Google's tiles spread their
    // triangles over two or three photographs (one primitive each); they are
    // packed side by side into one texture, so a tile stays one draw.
    let mut images: Vec<Option<usize>> = Vec::new();
    for (t, _) in &parts {
        if !images.contains(t) {
            images.push(*t);
        }
    }
    let mut pics = Vec::with_capacity(images.len());
    for &i in &images {
        let (w, h, mut px) = match i.and_then(|i| j["images"][i]["bufferView"].as_u64()) {
            Some(bv) => {
                let v = &j["bufferViews"][bv as usize];
                let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
                let len = v["byteLength"].as_u64().unwrap_or(0) as usize;
                let img = image::load_from_memory(bin.get(off..off + len).ok_or_else(|| anyhow!("image out of range"))?)?.to_rgba8();
                let (w, h) = img.dimensions();
                (w, h, img.into_raw())
            }
            None => (1, 1, vec![128, 128, 128, 255]),
        };
        // The photograph's sunlight out, how lit each texel was into alpha.
        if crate::relight::enabled() {
            crate::relight::delight(&mut px);
        }
        pics.push((w, h, px));
    }
    let mips = if pics.len() == 1 {
        let (w, h, px) = pics.pop().unwrap();
        mips(w, h, px)
    } else {
        let (aw, ah, at) = pack(&pics.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>());
        let mut px = vec![0u8; (aw * ah * 4) as usize];
        for (k, (w, h, src)) in pics.iter().enumerate() {
            let (ox, oy) = at[k];
            blit_padded(&mut px, aw, ox, oy, *w, *h, src);
        }
        for (t, run) in &parts {
            let k = images.iter().position(|i| i == t).unwrap();
            let (w, h) = (pics[k].0 as f32, pics[k].1 as f32);
            let (ox, oy) = ((at[k].0 + ATLAS_PAD) as f32, (at[k].1 + ATLAS_PAD) as f32);
            for uv in &mut uvs[run.clone()] {
                *uv = [(ox + uv[0].clamp(0.0, 1.0) * w) / aw as f32, (oy + uv[1].clamp(0.0, 1.0) * h) / ah as f32];
            }
        }
        mips(aw, ah, px)
    };
    // Into our frame, about the middle of the tile.
    let mid = ecef.iter().fold(DVec3::ZERO, |a, p| a + *p) / ecef.len() as f64;
    let origin = ecef_to_ours(mid);
    let mut lla = Vec::with_capacity(ecef.len());
    let mut pos: Vec<Vec3> = ecef
        .iter()
        .map(|p| {
            let (la, lo, h) = ecef_to_lla(*p);
            lla.push([la, lo, h]);
            (geo::lla_to_pos(la, lo, h) - origin).as_vec3()
        })
        .collect();
    // Roads and runways made smooth, before the normals and the collision mesh.
    if let Some(src) = src {
        crate::pave::flatten(src, origin, &mut pos, &mut lla);
    }
    // Which building each vertex is part of, for its windows at night.
    let (tags, beacons) = src.map(|src| crate::photonight::tag(src, origin, &pos, &lla)).unwrap_or_default();
    let mut nrm = vec![Vec3::ZERO; pos.len()];
    for t in idx.chunks_exact(3) {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let f = (b - a).cross(c - a);
        for &i in t {
            nrm[i as usize] += f;
        }
    }
    let up = origin.normalize().as_vec3();
    let sun = crate::relight::wall_sun(&pos, &idx, &uvs, &mips[0].2, mips[0].0, mips[0].1, up);
    let verts = pos
        .iter()
        .zip(&uvs)
        .zip(&nrm)
        .enumerate()
        .map(|(i, ((p, uv), n))| {
            let n = n.try_normalize().unwrap_or(up);
            let q = |v: f32| (v * 127.0).round().clamp(-127.0, 127.0) as i8;
            PVertex { pos: p.to_array(), uv: *uv, nrm: [q(n.x), q(n.y), q(n.z), tags.get(i).copied().unwrap_or(0) as i8] }
        })
        .collect();
    let step = (lla.len() / 64).max(1);
    let samples = lla.iter().step_by(step).copied().collect();
    let copyright = j["asset"]["copyright"].as_str().unwrap_or("").split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let col = ColMesh::build(origin, &pos, &idx);
    Ok(MeshData { verts, idx, mips, origin, col, copyright, samples, sun, beacons })
}

// ------------------------------------------------------------ workers

const ROOT: u32 = u32::MAX;
const ROOT_PATH: &str = "/v1/3dtiles/root.json";

struct Req {
    epoch: u32,
    node: u32,
    path: String,
    json: bool,
}
enum Payload {
    Tree(PNode),
    Mesh(Box<MeshData>),
}
struct Res {
    epoch: u32,
    node: u32,
    r: Result<Payload>,
}
/// Requests waiting (priority, lowest first), and how many workers are busy
/// out of how many may be.
struct Jobs {
    items: Vec<(f32, Req)>,
    busy: usize,
    limit: usize,
}
type Queue = Arc<(Mutex<Jobs>, Condvar)>;

fn spawn_workers(g: Arc<Google>, src: Option<Arc<crate::sources::Sources>>, queue: Queue, tx: crossbeam_channel::Sender<Res>) {
    for _ in 0..WORKERS {
        let (g, src, queue, tx) = (g.clone(), src.clone(), queue.clone(), tx.clone());
        std::thread::spawn(move || loop {
            let req = {
                let mut q = queue.0.lock();
                while q.items.is_empty() || q.busy >= q.limit {
                    queue.1.wait(&mut q);
                }
                let best = q.items.iter().enumerate().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(i, _)| i).unwrap();
                q.busy += 1;
                q.items.swap_remove(best).1
            };
            let r = g.get3d(&req.path).and_then(|d| if req.json { parse_tileset(&d, &req.path).map(Payload::Tree) } else { decode_with(&d, src.as_deref()).map(|m| Payload::Mesh(Box::new(m))) });
            queue.0.lock().busy -= 1;
            queue.1.notify_one();
            if tx.send(Res { epoch: req.epoch, node: req.node, r }).is_err() {
                break;
            }
        });
    }
}

// ------------------------------------------------------------ the zone

/// What the frame needs to draw the zone and blend it with our world.
pub struct PhotoFrame {
    /// camera-relative top-down projection of the coverage mask
    pub mask_vp: Mat4,
    /// zone radius (m, 0 = off), band width, mask valid
    pub zone: [f32; 4],
    /// zone centre relative to the camera
    pub centre: [f32; 4],
    /// the photographer's sun (unit, our frame; elevation assumed) and how
    /// sure that is (0..1, 0 = take our own shadows everywhere)
    pub sun: [f32; 4],
    /// destroyed buildings cut out of the mesh (see common.wgsl, `photo_cut`):
    /// per cut its floor's middle relative to the camera and radius, east axis,
    /// north axis and first vertex, vertex count; the outlines two vertices a row
    pub cut: [[f32; 4]; 3 * MAX_CUTS],
    pub cut_v: [[f32; 4]; MAX_CUT_V / 2],
}

/// Cuts and outline vertices the shaders take at most (common.wgsl).
pub const MAX_CUTS: usize = 16;
pub const MAX_CUT_V: usize = 256;
/// Outline vertices of one cut at most.
const CUT_RING: usize = 32;

/// A destroyed building taken out of the mesh: inside its outline (grown a
/// little past the walls) nothing above its floor is drawn or stands, and our
/// own ground shows through. Its triangles come down with the wreck instead.
pub struct Cut {
    /// the middle of its floor (our frame, before the datum: it moves with
    /// the mesh) and the local axes there
    o: DVec3,
    e: DVec3,
    n: DVec3,
    ring: Vec<glam::Vec2>,
    r: f32,
}
impl Cut {
    fn local(&self, p: DVec3) -> glam::Vec2 {
        // (horizontal only: the datum, along up, does not matter)
        let d = p - self.o;
        glam::Vec2::new(d.dot(self.e) as f32, d.dot(self.n) as f32)
    }
    /// Whether a point lies inside the outline (seen from above).
    fn contains(&self, p: DVec3) -> bool {
        let q = self.local(p);
        q.length_squared() <= self.r * self.r && inside(&self.ring, q)
    }
}

pub fn inside(ring: &[glam::Vec2], p: glam::Vec2) -> bool {
    let mut c = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (p0, p1) = (ring[i], ring[j]);
        if (p0.y > p.y) != (p1.y > p.y) && p.x < (p1.x - p0.x) * (p.y - p0.y) / (p1.y - p0.y) + p0.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// An outline with at most `max` vertices: the ones that matter least (the
/// smallest triangle with their neighbours) dropped first.
fn simplify(ring: &[glam::Vec2], max: usize) -> Vec<glam::Vec2> {
    let mut r = ring.to_vec();
    while r.len() > max.max(3) {
        let n = r.len();
        let k = (0..n).min_by(|&a, &b| {
            let w = |i: usize| (r[i] - r[(i + n - 1) % n]).perp_dot(r[(i + 1) % n] - r[i]).abs();
            w(a).total_cmp(&w(b))
        });
        r.remove(k.unwrap());
    }
    r
}

/// A small copy of a tile's texture (RGBA, sRGB), for the colour of the
/// pieces a destroyed building breaks into.
pub struct Tiny {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}
impl Tiny {
    /// Linear colour at a texture coordinate.
    pub fn at(&self, uv: [f32; 2]) -> [f32; 3] {
        let x = ((uv[0].rem_euclid(1.0) * self.w as f32) as u32).min(self.w - 1);
        let y = ((uv[1].rem_euclid(1.0) * self.h as f32) as u32).min(self.h - 1);
        let i = ((y * self.w + x) * 4) as usize;
        let c = |k: usize| (self.px.get(i + k).copied().unwrap_or(128) as f32 / 255.0).powf(2.2);
        [c(0), c(1), c(2)]
    }
}

/// The triangles of one tile that a destroyed building was made of.
pub struct SkinTile {
    pub view: wgpu::TextureView,
    pub tiny: Arc<Tiny>,
    /// corners in our frame (datum applied) and their texture coordinates
    pub tris: Vec<([DVec3; 3], [[f32; 2]; 3])>,
}

/// A destroyed building as the mesh had it (`Photo::take_building`).
pub struct SkinSrc {
    /// the floor's middle and the local axes there
    pub o: DVec3,
    pub e: DVec3,
    pub n: DVec3,
    pub u: DVec3,
    /// the outlines that were cut out, on those axes
    pub rings: Vec<Vec<glam::Vec2>>,
    pub tiles: Vec<SkinTile>,
    /// altitude of the highest of the mesh inside each outline
    pub tops: Vec<Option<f64>>,
}
impl SkinSrc {
    /// Where a point lies: across, along and above the floor.
    pub fn local(&self, p: DVec3) -> (glam::Vec2, f32) {
        let d = p - self.o;
        (glam::Vec2::new(d.dot(self.e) as f32, d.dot(self.n) as f32), d.dot(self.u) as f32)
    }
    /// Inside the cut: within an outline and above the floor.
    pub fn inside(&self, p: DVec3) -> bool {
        let (q, z) = self.local(p);
        z > 0.0 && self.rings.iter().any(|r| inside(r, q))
    }
}

pub struct Photo {
    g: Arc<Google>,
    /// where the pavement to smooth comes from (pave.rs)
    src: Option<Arc<crate::sources::Sources>>,
    nodes: Vec<Node>,
    /// root tileset: when it was asked for (seconds since t0), and whether it is in
    root_at: Option<f64>,
    root_ok: bool,
    root_fails: u32,
    queue: Queue,
    tx: crossbeam_channel::Sender<Res>,
    rx: crossbeam_channel::Receiver<Res>,
    workers: bool,
    epoch: u32,
    frame: u64,
    t0: Instant,
    gpu_bytes: usize,
    uploads: VecDeque<u32>,
    wanted: Vec<(f32, u32)>,
    /// the tiles chosen this frame (what the physics, credits and lights go by)
    draws: Vec<u32>,
    /// what is drawn: the chosen tiles, plus those dissolving into them; how
    /// far each has faded in (or is left), and whether it is leaving
    drawn: Vec<(u32, f32, bool)>,
    /// shown last frame (chosen or leaving)
    shown: Vec<u32>,
    /// crossfade length (s, 0 = off), and when it last advanced
    fade_s: f64,
    fade_t: f64,
    /// tiles the physics may stand on, finest first
    col: Vec<u32>,
    /// terrain minus mesh height at sample points: where, the difference,
    /// and the geometric error of the tile it came from; newest last
    samples: VecDeque<(DVec3, f32, f32)>,
    /// ellipsoid to our heights (m): what the mesh is lifted by
    pub datum: Option<f64>,
    /// the datum as first measured on fine tiles (once it held a few seconds)
    datum_lock: Option<f64>,
    /// the fine estimate lately, and since when it has held (seconds)
    datum_cand: Option<(f64, f64)>,
    /// zone radius now (shrinks to nothing at dusk), its centre on the ground
    pub zone_r: f64,
    centre: DVec3,
    anchor: Option<DVec3>,
    pub credits: String,
    pub active: bool,
    /// drawn tiles, tiles in memory, requests in flight or queued
    pub stats: (usize, usize, usize),
    /// GPU memory the tiles take (bytes)
    pub gpu_bytes_shown: usize,
    /// node count at which the tree is next compacted
    compact_at: usize,
    errors_logged: u32,
    /// destroyed buildings taken out of the mesh
    cuts: Vec<Cut>,
    /// the photographer's sun over the drawn tiles (horizontal, our frame)
    /// and how clearly their walls show it (0..1), smoothed
    baked_sun: (DVec3, f64),
    /// the shadows as photographed (the textures as they come) instead of our own
    pub relight: bool,
    /// what the detail setting streams, as of this frame
    pub params: Params,
}

impl Photo {
    pub fn new(g: Arc<Google>, src: Option<Arc<crate::sources::Sources>>) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            g,
            src,
            nodes: Vec::new(),
            root_at: None,
            root_ok: false,
            root_fails: 0,
            queue: Arc::new((Mutex::new(Jobs { items: Vec::new(), busy: 0, limit: Params::of(Detail::default()).fetches }), Condvar::new())),
            tx,
            rx,
            workers: false,
            epoch: 0,
            frame: 0,
            t0: Instant::now(),
            gpu_bytes: 0,
            uploads: VecDeque::new(),
            wanted: Vec::new(),
            draws: Vec::new(),
            drawn: Vec::new(),
            shown: Vec::new(),
            fade_s: std::env::var("FABLE_LOD_FADE").ok().and_then(|v| v.parse().ok()).unwrap_or(FADE_S).max(0.0),
            fade_t: 0.0,
            col: Vec::new(),
            samples: VecDeque::new(),
            datum: None,
            datum_lock: None,
            datum_cand: None,
            zone_r: 0.0,
            centre: DVec3::ZERO,
            anchor: None,
            credits: String::new(),
            active: false,
            stats: (0, 0, 0),
            gpu_bytes_shown: 0,
            compact_at: MAX_NODES,
            errors_logged: 0,
            cuts: Vec::new(),
            baked_sun: (DVec3::ZERO, 0.0),
            relight: crate::relight::enabled(),
            params: Params::of(Detail::default()),
        }
    }

    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    /// Drops the whole tree (a new session, or switched off for good).
    fn reset(&mut self) {
        self.epoch += 1;
        self.nodes.clear();
        self.compact_at = MAX_NODES;
        self.uploads.clear();
        self.draws.clear();
        self.drawn.clear();
        self.shown.clear();
        self.col.clear();
        self.gpu_bytes = 0;
        self.root_ok = false;
        self.root_at = None;
        self.queue.0.lock().items.clear();
    }

    fn push_tree(&mut self, p: PNode, parent: u32) -> u32 {
        let i = self.nodes.len() as u32;
        let sph_c = ecef_to_ours(p.obb.c);
        let sph_r = p.obb.radius();
        let never = u64::MAX;
        self.nodes.push(Node { obb: p.obb, sph_c, sph_r, error: p.error, kind: p.kind, state: State::Idle, fails: 0, children: Vec::new(), last_used: 0, parent, fade: 0.0, out: false, vis: never, sel: never, under: never, inc_below: never, out_below: never, busy: never });
        let kids: Vec<u32> = p.children.into_iter().map(|c| self.push_tree(c, i)).collect();
        self.nodes[i as usize].children = kids;
        i
    }

    /// Takes in what the workers finished.
    fn receive(&mut self) {
        while let Ok(res) = self.rx.try_recv() {
            if res.epoch != self.epoch {
                continue;
            }
            if res.node == ROOT {
                match res.r {
                    Ok(Payload::Tree(p)) => {
                        self.nodes.clear();
                        self.shown.clear();
                        self.push_tree(p, ROOT);
                        self.root_ok = true;
                        self.root_fails = 0;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::warn!("3D tiles root: {e:#}");
                        self.root_fails += 1;
                        self.root_at = Some(self.now() + 30.0 * self.root_fails as f64 - SESSION_S);
                    }
                }
                continue;
            }
            let i = res.node as usize;
            if i >= self.nodes.len() {
                continue;
            }
            match res.r {
                Ok(Payload::Tree(p)) => {
                    let c = self.push_tree(p, i as u32);
                    let n = &mut self.nodes[i];
                    n.children = vec![c];
                    n.state = State::Linked;
                }
                Ok(Payload::Mesh(m)) => {
                    self.nodes[i].state = State::Decoded(m);
                    self.uploads.push_back(i as u32);
                }
                Err(e) => {
                    if self.errors_logged < 20 {
                        self.errors_logged += 1;
                        log::warn!("3D tile: {e:#}");
                    }
                    let n = &mut self.nodes[i];
                    n.fails = n.fails.saturating_add(1);
                    n.state = State::Failed(self.frame);
                }
            }
        }
    }

    fn culled(&self, n: &Node, c: &Ctx) -> bool {
        let df = n.obb.dist(c.centre_e);
        if df > self.zone_r {
            return true;
        }
        // What lies under the aircraft is kept for the physics, seen or not.
        df > 400.0 && !c.in_view(n)
    }

    fn sse(&self, n: &Node, c: &Ctx) -> f64 {
        n.error * c.px / n.obb.dist(c.cam_e).max(1.0)
    }

    fn want(&mut self, i: u32, c: &Ctx) {
        let n = &self.nodes[i as usize];
        // (Queued: still waiting, or already being fetched; see `schedule`.)
        let retry = match n.state {
            State::Idle | State::Queued => true,
            State::Failed(f) => n.fails < 4 && self.frame > f + 300 * n.fails as u64,
            _ => false,
        };
        if retry {
            // Closest to splitting first; what is off screen (under the aircraft) after.
            let mut prio = (1.0 / self.sse(n, c).max(1e-3)) as f32;
            if !c.in_view(n) {
                prio *= 4.0;
            }
            self.wanted.push((prio, i));
        }
    }

    /// Ready to stand in for its parent: everything under it in view has loaded.
    fn ready_deep(&self, i: u32, c: &Ctx, depth: u32) -> bool {
        let n = &self.nodes[i as usize];
        if self.culled(n, c) {
            return true;
        }
        match (&n.kind, &n.state) {
            (Kind::Glb(_), State::Ready(_)) => true,
            (Kind::Glb(_), State::Failed(_)) => n.fails >= 4,
            (Kind::Glb(_), _) => false,
            (Kind::Json(_), State::Linked) | (Kind::Group, _) => depth < 4 && n.children.iter().all(|&k| self.ready_deep(k, c, depth + 1)),
            (Kind::Json(_), _) => false,
        }
    }

    fn prefetch(&mut self, i: u32, c: &Ctx, depth: u32) {
        if self.culled(&self.nodes[i as usize], c) {
            return;
        }
        self.nodes[i as usize].last_used = self.frame;
        match (&self.nodes[i as usize].kind, &self.nodes[i as usize].state) {
            (Kind::Glb(_), _) => self.want(i, c),
            (Kind::Json(_), State::Linked) | (Kind::Group, _) => {
                if depth < 4 {
                    for k in self.nodes[i as usize].children.clone() {
                        self.prefetch(k, c, depth + 1);
                    }
                }
            }
            (Kind::Json(_), _) => self.want(i, c),
        }
    }

    fn visit(&mut self, i: u32, c: &Ctx) {
        let n = &self.nodes[i as usize];
        if self.culled(n, c) {
            return;
        }
        let coarse = n.error > E_DRAW;
        let refine = !n.children.is_empty() && (coarse || self.sse(n, c) > self.params.sse_px);
        let kind = n.kind.clone();
        let linked = matches!(n.state, State::Linked);
        self.nodes[i as usize].last_used = self.frame;
        match kind {
            Kind::Json(_) if !linked => return self.want(i, c),
            Kind::Json(_) | Kind::Group => {
                for k in self.nodes[i as usize].children.clone() {
                    self.visit(k, c);
                }
                return;
            }
            Kind::Glb(_) => {}
        }
        if coarse {
            // never drawn: our own world is better at this scale
            for k in self.nodes[i as usize].children.clone() {
                self.visit(k, c);
            }
            return;
        }
        let kids = self.nodes[i as usize].children.clone();
        let kids_ready = !kids.is_empty() && kids.iter().all(|&k| self.ready_deep(k, c, 0));
        // Crossfades go one step at a time (else three levels would overlap
        // and leave holes): a tile still fading in does not split yet, and a
        // tile does not take back the place of children still fading in.
        let n = &self.nodes[i as usize];
        let fading_in = n.vis == self.frame - 1 && !n.out && n.fade < 1.0 && matches!(n.state, State::Ready(_));
        let held = n.busy == self.frame;
        if refine {
            if kids_ready && !fading_in {
                for k in kids {
                    self.visit(k, c);
                }
                return;
            }
            for &k in &kids {
                self.prefetch(k, c, 0);
            }
        } else if held && kids_ready {
            for k in kids {
                self.visit(k, c);
            }
            return;
        }
        if matches!(self.nodes[i as usize].state, State::Ready(_)) {
            self.draws.push(i);
        } else {
            self.want(i, c);
            // Not here (yet, or any more): its children may stand in.
            if kids_ready {
                for k in kids {
                    self.visit(k, c);
                }
            }
        }
    }

    /// Takes on a detail level (from the settings, any time): the zone eases
    /// to its radius, and the fetch threads to its count.
    fn set_params(&mut self, p: Params) {
        self.params = p;
        let mut q = self.queue.0.lock();
        if q.limit != p.fetches {
            let more = p.fetches > q.limit;
            q.limit = p.fetches;
            drop(q);
            if more {
                self.queue.1.notify_all();
            }
        }
    }

    /// Streams and selects the tiles for this frame. `px` is the screen
    /// height over 2 tan(fov / 2); `day` scales the zone (0 at night);
    /// `terrain` is our own ground height (m) at a lat/lon, for the datum,
    /// and whether it comes from a fine terrain tile.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, r: &Renderer, cam: DVec3, focus: DVec3, fr: &Frustum, px: f64, day: f64, dt: f64, terrain: &dyn Fn(f64, f64) -> Option<(f64, bool)>) {
        self.frame += 1;
        let want_on = self.g.photoreal() && day > 0.0;
        if !self.g.photoreal() && (!self.nodes.is_empty() || self.root_at.is_some()) {
            self.reset();
        }
        self.receive();
        // The zone grows and shrinks smoothly (on at dawn, off at dusk or when switched off).
        self.set_params(Params::of(self.g.detail()));
        let target = if want_on && self.datum.is_some() { self.params.zone_r * day.clamp(0.0, 1.0) } else { 0.0 };
        self.zone_r += (target - self.zone_r) * (dt * 0.8).min(1.0);
        if target == 0.0 && self.zone_r < 50.0 {
            self.zone_r = 0.0;
        }
        self.draws.clear();
        self.drawn.clear();
        self.col.clear();
        self.wanted.clear();
        self.active = want_on;
        let (flat, flon, _) = geo::pos_to_lla(focus);
        let gh = terrain(flat, flon).map_or(0.0, |t| t.0);
        self.centre = geo::lla_to_pos(flat, flon, gh);
        if !want_on {
            self.shown.clear();
            self.stats = (0, self.loaded(), 0);
            return;
        }
        if !self.workers {
            self.workers = true;
            spawn_workers(self.g.clone(), self.src.clone(), self.queue.clone(), self.tx.clone());
        }
        // A session lasts about three hours: then the whole tree is fetched anew.
        let now = self.now();
        if self.root_at.is_some_and(|t| now - t > SESSION_S) {
            let fails = self.root_fails;
            self.reset();
            self.root_fails = fails;
        }
        if self.root_at.is_none() {
            self.root_at = Some(now);
            self.queue.0.lock().items.push((0.0, Req { epoch: self.epoch, node: ROOT, path: ROOT_PATH.into(), json: true }));
            self.queue.1.notify_one();
        }
        self.upload(r, terrain);
        if self.nodes.len() > self.compact_at {
            self.compact();
            // Not again until it has doubled (what is in use may be most of it).
            self.compact_at = MAX_NODES.max(self.nodes.len() * 2);
        }
        if !self.root_ok || self.nodes.is_empty() {
            self.shown.clear();
            self.stats = (0, 0, 1);
            return;
        }
        let datum = self.datum.unwrap_or(0.0);
        let (clat, clon, ch) = geo::pos_to_lla(cam);
        // While the datum is still unknown, load (to measure it) over what will be the zone.
        let reach = if self.zone_r > 0.0 { self.zone_r } else { 2500.0f64.min(self.params.zone_r) };
        let c = Ctx { cam, cam_e: lla_to_ecef(clat, clon, ch - datum), centre_e: lla_to_ecef(flat, flon, gh - datum), fr: fr.clone(), px, datum };
        let zr = self.zone_r;
        self.zone_r = reach;
        self.mark_busy();
        self.visit(0, &c);
        self.zone_r = zr;
        if self.zone_r <= 0.0 {
            self.draws.clear();
        }
        self.crossfade();
        // Collision tiles near the aircraft, finest first.
        for (i, n) in self.nodes.iter().enumerate() {
            if let State::Ready(_) = n.state {
                if n.error <= COL_ERROR && n.last_used + 2 >= self.frame && n.obb.dist(c.centre_e) < COL_DIST {
                    self.col.push(i as u32);
                }
            }
        }
        self.col.sort_by(|a, b| self.nodes[*a as usize].error.total_cmp(&self.nodes[*b as usize].error));
        self.schedule();
        self.evict();
        self.update_datum(dt);
        self.update_baked_sun(dt);
        // Credits: every source in view, most frequent first.
        let mut count: HashMap<&str, usize> = HashMap::new();
        for &i in &self.draws {
            if let State::Ready(l) = &self.nodes[i as usize].state {
                for s in &l.copyright {
                    *count.entry(s.as_str()).or_default() += 1;
                }
            }
        }
        let mut cr: Vec<(&str, usize)> = count.into_iter().collect();
        cr.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        self.credits = cr.iter().map(|c| c.0).collect::<Vec<_>>().join("; ");
        let queued = self.queue.0.lock().items.len();
        let flying = self.nodes.iter().filter(|n| matches!(n.state, State::Queued)).count();
        self.stats = (self.draws.len(), self.loaded(), flying.max(queued));
        self.gpu_bytes_shown = self.gpu_bytes;
    }

    /// Drops the subtrees that have not been needed for a while (their
    /// tilesets are fetched again if the aircraft comes back) and packs the
    /// rest. Requests still out are forgotten (a new epoch) and asked again.
    fn compact(&mut self) {
        let frame = self.frame;
        let mut old = std::mem::take(&mut self.nodes);
        // Unlink stale subtrees: JSON links whose whole subtree is unused.
        fn used(nodes: &[Node], i: usize, frame: u64) -> bool {
            let n = &nodes[i];
            n.last_used + UNUSED_FRAMES >= frame || n.children.iter().any(|&c| used(nodes, c as usize, frame))
        }
        for i in 0..old.len() {
            if matches!(old[i].kind, Kind::Json(_)) && matches!(old[i].state, State::Linked) && !used(&old, i, frame) {
                old[i].children.clear();
                old[i].state = State::Idle;
            }
        }
        // Copy what is still reachable, renumbering.
        let mut map = vec![u32::MAX; old.len()];
        let mut order = vec![0usize];
        let mut k = 0;
        while k < order.len() {
            let i = order[k];
            map[i] = k as u32;
            order.extend(old[i].children.iter().map(|&c| c as usize));
            k += 1;
        }
        let mut slots: Vec<Option<Node>> = old.into_iter().map(Some).collect();
        let mut freed = 0;
        for (i, slot) in slots.iter().enumerate() {
            if map[i] == u32::MAX {
                if let Some(State::Ready(l)) = slot.as_ref().map(|n| &n.state) {
                    freed += l.bytes;
                }
            }
        }
        self.gpu_bytes -= freed;
        self.nodes = order
            .iter()
            .map(|&i| {
                let mut n = slots[i].take().unwrap();
                n.children = n.children.iter().map(|&c| map[c as usize]).collect();
                if n.parent != ROOT {
                    n.parent = map[n.parent as usize];
                }
                if matches!(n.state, State::Queued | State::Decoded(_)) {
                    n.state = State::Idle;
                }
                n
            })
            .collect();
        self.shown = self.shown.iter().map(|&i| map[i as usize]).filter(|&i| i != u32::MAX).collect();
        self.epoch += 1;
        self.uploads.clear();
        self.queue.0.lock().items.clear();
        log::info!("3D tiles: kept {} nodes", self.nodes.len());
    }

    /// Stamps `field` with this frame on every node above `i` (stopping
    /// where it is already stamped: everything above is too).
    fn mark_up(&mut self, i: u32, field: fn(&mut Node) -> &mut u64) {
        let f = self.frame;
        let mut p = self.nodes[i as usize].parent;
        while p != ROOT {
            let n = &mut self.nodes[p as usize];
            if *field(n) == f {
                break;
            }
            *field(n) = f;
            p = n.parent;
        }
    }

    fn any_above(&self, i: u32, test: impl Fn(&Node) -> bool) -> bool {
        let mut p = self.nodes[i as usize].parent;
        while p != ROOT {
            let n = &self.nodes[p as usize];
            if test(n) {
                return true;
            }
            p = n.parent;
        }
        false
    }

    /// Before choosing: marks the tiles above any still fading in (`visit`
    /// keeps choosing those children until they have).
    fn mark_busy(&mut self) {
        for k in 0..self.shown.len() {
            let i = self.shown[k];
            let n = &self.nodes[i as usize];
            if !n.out && n.fade < 1.0 {
                self.mark_up(i, |n| &mut n.busy);
            }
        }
    }

    /// After choosing: a tile chosen in place of its parent (or its children)
    /// fades in over the same seconds as they fade out, the two dithered
    /// against the same noise (photo.wgsl, `lod_keep`) so that each pixel
    /// shows one or the other: their fades always add up to one. A tile that
    /// replaces nothing shown (the first load, the edge of the zone, turning
    /// to look) is there at once, as there is nothing to fade from.
    fn crossfade(&mut self) {
        let f = self.frame;
        let now = self.now();
        let dt = (now - self.fade_t).clamp(0.0, 0.25);
        self.fade_t = now;
        let step = if self.fade_s > 0.0 { ((dt / self.fade_s) as f32).max(1e-4) } else { 1.0 };
        let prev = std::mem::take(&mut self.shown);
        for &i in &prev {
            self.mark_up(i, |n| &mut n.under);
        }
        for &i in &self.draws {
            self.nodes[i as usize].sel = f;
        }
        for k in 0..self.draws.len() {
            let i = self.draws[k];
            let n = &self.nodes[i as usize];
            if n.vis == f - 1 {
                // (leaving, and chosen again: it turns back, at the fade it had)
                self.nodes[i as usize].out = false;
                continue;
            }
            let related = n.under == f || self.any_above(i, |a| a.vis == f - 1);
            let n = &mut self.nodes[i as usize];
            n.fade = if related { 0.0 } else { 1.0 };
            n.out = false;
        }
        for k in 0..self.draws.len() {
            let i = self.draws[k];
            if self.nodes[i as usize].fade < 1.0 {
                self.mark_up(i, |n| &mut n.inc_below);
            }
        }
        // Shown last frame but not chosen now: it stays while what took its
        // place fades in.
        let mut leaving = Vec::new();
        for &i in &prev {
            let n = &self.nodes[i as usize];
            if n.sel == f {
                continue;
            }
            let keep = matches!(n.state, State::Ready(_)) && (n.inc_below == f || self.any_above(i, |a| a.sel == f && a.fade < 1.0));
            if keep {
                leaving.push(i);
            } else {
                let n = &mut self.nodes[i as usize];
                n.fade = 0.0;
                n.out = false;
            }
        }
        // Fading in with nothing left to fade from (it went out of view, or
        // out of the zone): there at once, or it would stay see-through.
        for &i in &leaving {
            self.nodes[i as usize].out = true;
            self.mark_up(i, |n| &mut n.out_below);
        }
        for k in 0..self.draws.len() {
            let i = self.draws[k];
            let n = &self.nodes[i as usize];
            if n.fade < 1.0 && n.out_below != f && !self.any_above(i, |a| a.out && a.sel != f && a.vis == f - 1) {
                self.nodes[i as usize].fade = 1.0;
            }
        }
        for k in 0..self.draws.len() {
            let i = self.draws[k];
            let n = &mut self.nodes[i as usize];
            n.fade = (n.fade + step).min(1.0);
            n.vis = f;
            self.drawn.push((i, n.fade, false));
            self.shown.push(i);
        }
        for i in leaving {
            let n = &mut self.nodes[i as usize];
            n.out = true;
            n.fade -= step;
            if n.fade > 0.0 {
                n.vis = f;
                n.last_used = f;
                self.drawn.push((i, n.fade, true));
                self.shown.push(i);
            } else {
                n.fade = 0.0;
                n.out = false;
            }
        }
    }

    fn loaded(&self) -> usize {
        self.nodes.iter().filter(|n| matches!(n.state, State::Ready(_))).count()
    }

    /// Hands the wanted requests to the workers, closest first; requests not
    /// yet taken are withdrawn so the queue follows the view.
    fn schedule(&mut self) {
        let mut q = self.queue.0.lock();
        // Not taken yet: back to idle, and queued again below if still wanted.
        // Whatever is still Queued after this is in a worker's hands.
        for (_, r) in q.items.drain(..) {
            if let Some(n) = self.nodes.get_mut(r.node as usize) {
                if matches!(n.state, State::Queued) {
                    n.state = State::Idle;
                }
            }
        }
        self.wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.wanted.dedup_by_key(|w| w.1);
        for &(p, i) in self.wanted.iter().take(MAX_QUEUE) {
            let n = &mut self.nodes[i as usize];
            let (path, json) = match &n.kind {
                Kind::Glb(p) => (p.clone(), false),
                Kind::Json(p) => (p.clone(), true),
                Kind::Group => continue,
            };
            if matches!(n.state, State::Idle | State::Failed(_)) {
                n.state = State::Queued;
                q.items.push((p, Req { epoch: self.epoch, node: i, path, json }));
            }
        }
        drop(q);
        self.queue.1.notify_all();
    }

    /// Copies decoded tiles to the GPU, a budget per frame.
    fn upload(&mut self, r: &Renderer, terrain: &dyn Fn(f64, f64) -> Option<(f64, bool)>) {
        let mut budget = UPLOAD_BUDGET;
        let now = self.now();
        while let Some(&i) = self.uploads.front() {
            let n = &mut self.nodes[i as usize];
            let State::Decoded(m) = &n.state else {
                self.uploads.pop_front();
                continue;
            };
            let bytes = m.gpu_bytes();
            if bytes > budget && budget < UPLOAD_BUDGET {
                break;
            }
            budget = budget.saturating_sub(bytes);
            self.uploads.pop_front();
            let State::Decoded(m) = std::mem::replace(&mut n.state, State::Idle) else { unreachable!() };
            let gpu = r.upload_photo(&m);
            // How far the mesh sits from our terrain, from the finer tiles.
            if n.error <= E_DRAW {
                // Only against fine terrain (a coarse tile is off by tens of
                // metres in hills), and off our buildings' roofs.
                let v: Vec<([f64; 3], f32)> = m.samples.iter().filter_map(|s| match terrain(s[0], s[1]) {
                    Some((t, true)) => Some((*s, (t - s[2]) as f32)),
                    _ => None,
                }).collect();
                for (s, d) in v.iter().step_by((v.len() / 16).max(1)) {
                    self.samples.push_back((geo::lla_to_pos(s[0], s[1], 0.0), *d, n.error as f32));
                }
                while self.samples.len() > 3000 {
                    self.samples.pop_front();
                }
            }
            let uv = m.verts.iter().map(|v| v.uv).collect();
            let small = m.mips.iter().find(|l| l.0 <= 64 && l.1 <= 64).unwrap_or(m.mips.last().unwrap());
            let tiny = Arc::new(Tiny { w: small.0, h: small.1, px: small.2.clone() });
            let m = *m;
            n.state = State::Ready(Box::new(Loaded { gpu, col: m.col, uv, tiny, bytes, copyright: m.copyright, at: now, sun: m.sun, beacons: m.beacons }));
            self.gpu_bytes += bytes;
        }
    }

    fn evict(&mut self) {
        let budget = self.params.gpu_budget;
        if self.gpu_bytes <= budget {
            return;
        }
        let mut old: Vec<(u64, u32)> = self.nodes.iter().enumerate().filter(|(_, n)| matches!(n.state, State::Ready(_)) && n.last_used + 2 < self.frame).map(|(i, n)| (n.last_used, i as u32)).collect();
        old.sort();
        for (_, i) in old {
            if self.gpu_bytes <= budget * 9 / 10 {
                break;
            }
            if let State::Ready(l) = std::mem::replace(&mut self.nodes[i as usize].state, State::Idle) {
                self.gpu_bytes -= l.bytes;
            }
        }
    }

    /// Where the photographer's sun stood, from the walls of the tiles drawn
    /// (each alone is good to some 30 degrees, together to a few). Only the
    /// azimuth shows; flights for aerial photography keep the sun high.
    fn update_baked_sun(&mut self, dt: f64) {
        let (mut a, mut area) = (Vec3::ZERO, 0.0f32);
        for &i in &self.draws {
            if let State::Ready(l) = &self.nodes[i as usize].state {
                a += l.sun.0;
                area += l.sun.1;
            }
        }
        if area <= 0.0 {
            return;
        }
        let avg = a.as_dvec3() / area as f64;
        let up = self.centre.normalize();
        let hz = (avg - up * avg.dot(up)).normalize_or_zero();
        // (lit share facing it over facing away: 0.03 is a faint hint, 0.08 typical)
        let conf = ((avg.length() - 0.02) / 0.03).clamp(0.0, 1.0);
        let k = (dt * 0.5).min(1.0);
        let (d0, c0) = self.baked_sun;
        let d = if d0 == DVec3::ZERO { hz } else { (d0 + (hz - d0) * k).normalize_or_zero() };
        self.baked_sun = (d, c0 + (conf - c0) * k);
        if self.frame % 1800 == 0 {
            let (e, n, _) = geo::enu(self.centre);
            log::debug!("3D tiles: photographed sun from azimuth {:.0} deg (walls {:.3}, sure {:.2})", d.dot(e).atan2(d.dot(n)).to_degrees().rem_euclid(360.0), avg.length(), self.baked_sun.1);
        }
    }

    /// The datum from the samples near the zone: terrain minus mesh height.
    /// Where the mesh is bare ground (or water) that is the datum give or take
    /// the terrain's own noise, a sharp peak; roofs and treetops pull it down
    /// and spread it over tens to hundreds of metres (downtown most samples
    /// are roofs). So: the densest metre of the upper half, refined to the
    /// mean of what lies within 2 m of it. Fine tiles are trusted over coarse
    /// ones, which smear roofs and streets together.
    fn update_datum(&mut self, dt: f64) {
        let near: Vec<&(DVec3, f32, f32)> = self.samples.iter().filter(|s| s.0.distance(self.centre) < 30_000.0).collect();
        let fine: Vec<f32> = near.iter().filter(|s| s.2 <= 40.0).map(|s| s.1).collect();
        let is_fine = fine.len() >= 120;
        let mut d = if is_fine { fine } else { near.iter().map(|s| s.1).collect() };
        if d.len() < 60 {
            return;
        }
        d.sort_by(|a, b| a.total_cmp(b));
        let Some(est) = ground_peak(&d) else { return };
        // Once it has been measured on fine tiles it only drifts a little:
        // the geoid hardly changes over a zone, while the peak can wander
        // metres to tens of metres off among downtown towers (walls, roofs
        // the map lacks, our terrain lifted by the towers), which would lift
        // or sink the whole city and what has been cut out of it.
        // It is trusted once it has held for a few seconds; a trusted one
        // only gives way to another that holds for a good while.
        let now = self.now();
        if !is_fine || self.datum_cand.is_none_or(|c| (est - c.0).abs() > 3.0) {
            self.datum_cand = is_fine.then_some((est, now));
        }
        let held = self.datum_cand.map_or(0.0, |c| now - c.1);
        if let Some(lock) = self.datum_lock {
            if (est - lock).abs() > 4.0 {
                if held < 20.0 {
                    return;
                }
                self.datum_lock = Some(est);
                self.datum = Some(est);
                return;
            }
        }
        self.datum = Some(match self.datum {
            Some(o) if (est - o).abs() < 40.0 && (self.datum_lock.is_some() || !is_fine) => o + (est - o) * (dt * 0.5).min(1.0),
            _ => est,
        });
        if self.datum_lock.is_none() && is_fine && held >= 3.0 {
            self.datum_lock = Some(est);
        }
    }

    /// Height (m above our sphere, datum applied) of the highest mesh surface
    /// on the vertical at lat/lon, for the physics. A tile that has only just
    /// arrived is blended in from what was there before (a coarser tile, or
    /// our own `terrain`).
    pub fn ground(&self, lat: f64, lon: f64, terrain: Option<f64>) -> Option<f64> {
        if self.zone_r <= 0.0 || self.col.is_empty() {
            return None;
        }
        let p = geo::lla_to_pos(lat, lon, 0.0);
        // Inside the zone, short of where it starts to dissolve.
        let q = p - self.centre;
        let up = self.centre.normalize();
        if (q - up * q.dot(up)).length() > self.zone_r - self.params.band() * 0.5 {
            return None;
        }
        let datum = self.datum?;
        // A destroyed building is not there any more: our own ground is.
        if self.cuts.iter().any(|c| c.contains(p)) {
            return None;
        }
        let now = self.now();
        let hits: Vec<(f64, f64)> = self
            .col
            .iter()
            .filter_map(|&i| match &self.nodes[i as usize].state {
                State::Ready(l) => l.col.top(p).map(|h| (h + datum, ((now - l.at) / RAMP_S).clamp(0.0, 1.0))),
                _ => None,
            })
            .take(3)
            .collect();
        let (&(h, k), rest) = hits.split_first()?;
        if k >= 1.0 {
            return Some(h);
        }
        let below = rest.first().map(|r| r.0).or(terrain).unwrap_or(h);
        Some(below + (h - below) * k)
    }

    /// Where the streets under our cars are, for the cars within `r` of `c`
    /// (our frame). None while the zone is off.
    pub fn streets(&self, c: DVec3, r: f64) -> Option<Streets<'_>> {
        let datum = self.datum?;
        if self.zone_r <= 0.0 || self.draws.is_empty() {
            return None;
        }
        let tiles: Vec<&ColMesh> = self
            .col
            .iter()
            .filter_map(|&i| match &self.nodes[i as usize].state {
                State::Ready(l) if l.col.near(c, r) => Some(&l.col),
                _ => None,
            })
            .collect();
        let boxes = tiles.iter().map(|t| (t.o, t.axes[0], t.axes[1], [t.min[0], t.min[1], t.max[0], t.max[1]])).collect();
        Some(Streets { photo: self, boxes, tiles, datum })
    }

    /// Whether our own scenery at lat/lon is hidden under the mesh.
    pub fn hides(&self, lat: f64, lon: f64) -> bool {
        self.ground(lat, lon, None).is_some()
    }

    /// The tiles to draw, each with its offset from the camera (datum
    /// included) and its geometric error; then how far it has faded in (or,
    /// leaving, how much is left) and 1 if it is leaving (`crossfade`).
    pub fn draws(&self, cam: DVec3) -> Vec<(&PhotoGpu, [[f32; 4]; 2])> {
        let datum = self.datum.unwrap_or(0.0);
        self.drawn
            .iter()
            .filter_map(|&(i, fade, out)| match &self.nodes[i as usize].state {
                State::Ready(l) => {
                    let o = l.col.o + l.col.axes[2] * datum - cam;
                    Some((&l.gpu, [[o.x as f32, o.y as f32, o.z as f32, self.nodes[i as usize].error as f32], [fade, if out { 1.0 } else { 0.0 }, 0.0, 0.0]]))
                }
                _ => None,
            })
            .collect()
    }

    /// The coverage mask's projection and the zone, for the shaders. The mask
    /// is laid around an anchor that only moves when the aircraft has gone a
    /// good way from it, so its texels stay put and the edges do not crawl.
    pub fn frame(&mut self, cam: DVec3) -> PhotoFrame {
        let c = self.centre;
        let anchor = match self.anchor {
            Some(a) if a.distance(c) < MASK_HALF - ZONE_R - 200.0 => a,
            _ => c,
        };
        self.anchor = Some(anchor);
        let (e, n, u) = geo::enu(anchor);
        // Looking straight down from high above the anchor: x east, y north,
        // depth 0..1 over 60 km down from there.
        let eye = anchor + u * 30_000.0 - cam;
        let (k, depth) = (1.0 / MASK_HALF, 1.0 / 60_000.0);
        let vp = DMat4::from_cols(
            glam::DVec4::new(e.x * k, n.x * k, -u.x * depth, 0.0),
            glam::DVec4::new(e.y * k, n.y * k, -u.y * depth, 0.0),
            glam::DVec4::new(e.z * k, n.z * k, -u.z * depth, 0.0),
            glam::DVec4::new(-e.dot(eye) * k, -n.dot(eye) * k, u.dot(eye) * depth, 1.0),
        );
        let on = self.zone_r > 0.0 && !self.draws.is_empty();
        let cc = c - cam;
        // The cuts nearest the camera, as many as the shaders take.
        let (mut cut, mut cut_v, mut n_cut, mut nv) = ([[0.0; 4]; 3 * MAX_CUTS], [[0.0; 4]; MAX_CUT_V / 2], 0, 0);
        let mut order: Vec<usize> = (0..self.cuts.len()).collect();
        order.sort_by(|&a, &b| (self.cuts[a].o - cam).length_squared().total_cmp(&(self.cuts[b].o - cam).length_squared()));
        for k in order {
            let ct = &self.cuts[k];
            if !on || n_cut >= MAX_CUTS {
                break;
            }
            if nv + ct.ring.len() > MAX_CUT_V {
                continue;
            }
            let o = ct.o + ct.o.normalize() * self.datum.unwrap_or(0.0) - cam;
            cut[n_cut * 3] = [o.x as f32, o.y as f32, o.z as f32, ct.r];
            cut[n_cut * 3 + 1] = [ct.e.x as f32, ct.e.y as f32, ct.e.z as f32, 0.0];
            cut[n_cut * 3 + 2] = [ct.n.x as f32, ct.n.y as f32, ct.n.z as f32, nv as f32];
            cut[n_cut * 3 + 1][3] = ct.ring.len() as f32;
            for v in &ct.ring {
                cut_v[nv / 2][(nv % 2) * 2] = v.x;
                cut_v[nv / 2][(nv % 2) * 2 + 1] = v.y;
                nv += 1;
            }
            n_cut += 1;
        }
        PhotoFrame {
            mask_vp: vp.as_mat4(),
            zone: [if on { self.zone_r as f32 } else { 0.0 }, self.params.band() as f32, if on { 1.0 } else { 0.0 }, n_cut as f32],
            centre: [cc.x as f32, cc.y as f32, cc.z as f32, 0.0],
            sun: {
                let up = self.centre.normalize();
                let (hz, conf) = self.baked_sun;
                let (s, c) = BAKED_ELEV.to_radians().sin_cos();
                let d = (hz * c + up * s).normalize_or(up);
                [d.x as f32, d.y as f32, d.z as f32, if self.relight { conf as f32 } else { -1.0 }]
            },
            cut,
            cut_v,
        }
    }

    /// The finest tiles in memory under node `i` that cover what `wanted`
    /// takes in; false if some of it is not there (a parent stands in then).
    fn pick(&self, i: u32, wanted: &dyn Fn(&Node) -> bool, out: &mut Vec<u32>, depth: u32) -> bool {
        let n = &self.nodes[i as usize];
        if !wanted(n) {
            return true;
        }
        let mut sub = Vec::new();
        let mut all = !n.children.is_empty() && depth < 64;
        if all {
            for &k in &n.children {
                all &= self.pick(k, wanted, &mut sub, depth + 1);
            }
        }
        if !all && matches!(n.state, State::Ready(_)) && n.error <= E_DRAW {
            out.push(i);
            return true;
        }
        // (with gaps if neither this nor all of its children are here)
        out.extend(sub);
        all
    }

    /// The red obstruction lights on the drawn mesh's tall buildings,
    /// camera-relative (they show at night only; see lights.wgsl). A building
    /// split over several drawn tiles is lit from the one holding its top.
    pub fn beacons(&self, cam: DVec3, out: &mut Vec<crate::airfield::Light>) {
        let Some(datum) = self.datum else { return };
        if self.zone_r <= 0.0 {
            return;
        }
        let up = self.centre.normalize();
        let mut best: std::collections::HashMap<u64, (f64, Vec<DVec3>)> = Default::default();
        for &i in &self.draws {
            let State::Ready(l) = &self.nodes[i as usize].state else { continue };
            if l.beacons.is_empty() {
                continue;
            }
            let o = l.col.o;
            let mut per: std::collections::HashMap<u64, Vec<DVec3>> = Default::default();
            for b in &l.beacons {
                let w = o + b.pos.as_dvec3();
                let q = w - self.centre;
                if (q - up * q.dot(up)).length() > self.zone_r - self.params.band() * 0.5 {
                    continue;
                }
                // Not on a destroyed building.
                if self.cuts.iter().any(|c| {
                    let d = w - c.o;
                    (d - c.o.normalize() * d.dot(c.o.normalize())).length() < c.r as f64
                }) {
                    continue;
                }
                per.entry(b.key).or_default().push(w + w.normalize() * datum);
            }
            for (k, v) in per {
                let top = v.iter().map(|p| p.length()).fold(0.0, f64::max);
                match best.get(&k) {
                    Some((t, _)) if *t >= top => {}
                    _ => {
                        best.insert(k, (top, v));
                    }
                }
            }
        }
        for (k, (_, v)) in best {
            // Medium-intensity red (FAA L-864): 20-40 flashes a minute, a
            // building's lights together, buildings out of step.
            let phase = (k >> 56) as u8;
            for p in v {
                let r = p - cam;
                out.push(crate::airfield::Light { pos: [r.x as f32, r.y as f32, r.z as f32], inten: 60.0, dir: [0.0; 3], spread: 0.0, col: [255, 28, 20, 0], fx: [18, phase, 110, 0] });
            }
        }
    }

    /// Height (m, datum applied) of the drawn mesh's highest surface on the
    /// vertical through a point.
    fn mesh_top(&self, p: DVec3) -> Option<f64> {
        let datum = self.datum?;
        self.draws.iter().find_map(|&i| match &self.nodes[i as usize].state {
            State::Ready(l) => l.col.top(p).map(|h| h + datum),
            _ => None,
        })
    }

    /// Whether the mesh is what is drawn (and stands) at a point.
    pub fn covers(&self, p: DVec3) -> bool {
        let q = p - self.centre;
        let up = self.centre.normalize();
        self.zone_r > 0.0 && self.datum.is_some() && !self.draws.is_empty() && (q - up * q.dot(up)).length() < self.zone_r - self.params.band() * 0.5
    }

    /// Takes a destroyed building out of the mesh. `rings`: its outlines
    /// (our frame, a little outside its walls), `base`: the altitude of its
    /// foot. From now on nothing inside the outlines above the street is drawn
    /// or stands; what was there (the drawn tiles' triangles) is handed back
    /// for the wreck to carry. None outside the zone.
    pub fn take_building(&mut self, rings: &[Vec<DVec3>], base: f64) -> Option<SkinSrc> {
        let all: Vec<DVec3> = rings.iter().flatten().copied().collect();
        if all.len() < 3 {
            return None;
        }
        let mid = all.iter().fold(DVec3::ZERO, |a, p| a + *p) / all.len() as f64;
        if !self.covers(mid) {
            return None;
        }
        let datum = self.datum?;
        let (lat, lon, _) = geo::pos_to_lla(mid);
        let g0 = geo::lla_to_pos(lat, lon, base);
        let (e, n, u) = geo::enu(g0);
        let to2 = |p: DVec3| glam::Vec2::new((p - g0).dot(e) as f32, (p - g0).dot(n) as f32);
        let local: Vec<Vec<glam::Vec2>> = rings.iter().map(|r| r.iter().map(|p| to2(*p)).collect()).collect();
        // The street round it, from the mesh just outside: the floor of the cut.
        let mut street: Vec<f64> = Vec::new();
        for r in &local {
            let c = r.iter().fold(glam::Vec2::ZERO, |a, p| a + *p) / r.len() as f32;
            for (i, &p) in r.iter().enumerate() {
                let q = r[(i + 1) % r.len()];
                for s in [p, (p + q) * 0.5] {
                    let out = (s - c).normalize_or_zero();
                    for d in [2.5f32, 4.5] {
                        let w = s + out * d;
                        if local.iter().any(|r| inside(r, w)) {
                            continue;
                        }
                        if let Some(h) = self.mesh_top(g0 + e * w.x as f64 + n * w.y as f64) {
                            street.push(h);
                        }
                    }
                }
            }
        }
        street.sort_by(|a, b| a.total_cmp(b));
        let ground = street.get(street.len() / 8).copied().unwrap_or(base).clamp(base - 4.0, base + 4.0);
        let floor = ground + 0.35;
        let o = g0 + u * (floor - base);
        let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
        for p in local.iter().flatten() {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let rad = local.iter().flatten().map(|p| p.length()).fold(0.0f32, f32::max);
        // (the shaders take the same outlines: what is cut out is exactly what comes down)
        let src_rings: Vec<Vec<glam::Vec2>> = local.iter().map(|r| simplify(r, CUT_RING)).collect();
        let mut tops: Vec<Option<f64>> = vec![None; local.len()];
        let mut tiles = Vec::new();
        // The finest tiles in memory that together cover the column over the
        // outlines, however high, seen or not (the cut takes all of it).
        let column = |nd: &Node| {
            // (bounding spheres are on ellipsoid heights: lifted by the datum like the mesh)
            let d = nd.sph_c + u * datum - o;
            (d - u * d.dot(u)).length() <= nd.sph_r + rad as f64 + 10.0 && d.dot(u) >= -nd.sph_r
        };
        let mut picked = Vec::new();
        if !self.nodes.is_empty() {
            self.pick(0, &column, &mut picked, 0);
        }
        for i in picked {
            let nd = &self.nodes[i as usize];
            let State::Ready(l) = &nd.state else { continue };
            let cm = &l.col;
            let world = |v: Vec3| cm.o + cm.axes[0] * v.x as f64 + cm.axes[1] * v.y as f64 + cm.axes[2] * (v.z as f64 + datum);
            let mut tris = Vec::new();
            for t in &cm.t {
                let p = t.map(|k| world(cm.v[k as usize]));
                let q = p.map(|p| ((p - o).dot(e) as f32, (p - o).dot(n) as f32, (p - o).dot(u) as f32));
                let (x0, x1) = (q[0].0.min(q[1].0).min(q[2].0), q[0].0.max(q[1].0).max(q[2].0));
                let (y0, y1) = (q[0].1.min(q[1].1).min(q[2].1), q[0].1.max(q[1].1).max(q[2].1));
                let z1 = q[0].2.max(q[1].2).max(q[2].2);
                if z1 <= 0.0 || x1 < lo.x || x0 > hi.x || y1 < lo.y || y0 > hi.y {
                    continue;
                }
                for (k, r) in src_rings.iter().enumerate() {
                    for c in &q {
                        if c.2 > 0.0 && inside(r, glam::Vec2::new(c.0, c.1)) {
                            let a = floor + c.2 as f64;
                            tops[k] = Some(tops[k].map_or(a, |t: f64| t.max(a)));
                        }
                    }
                }
                tris.push((p, t.map(|k| l.uv.get(k as usize).copied().unwrap_or([0.0; 2]))));
            }
            if !tris.is_empty() {
                tiles.push(SkinTile { view: l.gpu.view.clone(), tiny: l.tiny.clone(), tris });
            }
        }
        for r in &src_rings {
            if r.len() < 3 {
                continue;
            }
            let ring = r.clone();
            let rr = ring.iter().map(|p| p.length()).fold(0.0f32, f32::max) + 0.5;
            self.cuts.push(Cut { o: o - u * datum, e, n, ring, r: rr });
        }
        Some(SkinSrc { o, e, n, u, rings: src_rings, tiles, tops })
    }
}

/// The bare-ground peak of sorted terrain-minus-mesh differences (see
/// `Photo::update_datum`).
fn ground_peak(d: &[f32]) -> Option<f64> {
    const LO: f32 = -200.0;
    const N: usize = 400;
    let mid = *d.get(d.len() / 2)?;
    let mut hist = [0u32; N];
    for &v in d.iter().filter(|v| **v >= mid) {
        let b = (v - LO).floor();
        if b >= 0.0 && (b as usize) < N {
            hist[b as usize] += 1;
        }
    }
    let smooth = |i: usize| -> u32 { [(-2i32, 1u32), (-1, 2), (0, 3), (1, 2), (2, 1)].iter().map(|&(o, w)| hist.get((i as i32 + o) as usize).copied().unwrap_or(0) * w).sum() };
    let best = (0..N).max_by_key(|&i| smooth(i))?;
    let c = LO + best as f32 + 0.5;
    let near: Vec<f32> = d.iter().copied().filter(|v| (v - c).abs() <= 2.0).collect();
    (!near.is_empty()).then(|| near.iter().map(|&v| v as f64).sum::<f64>() / near.len() as f64)
}

struct Ctx {
    cam: DVec3,
    cam_e: DVec3,
    centre_e: DVec3,
    fr: Frustum,
    px: f64,
    /// the datum (m): tiles are drawn this much above their ellipsoid
    /// heights, and so are their bounding spheres for the frustum
    datum: f64,
}
impl Ctx {
    fn in_view(&self, n: &Node) -> bool {
        self.fr.sphere(n.sph_c + n.sph_c.normalize() * self.datum - self.cam, n.sph_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_levels_grow() {
        let p: Vec<Params> = Detail::ALL.into_iter().map(Params::of).collect();
        for w in p.windows(2) {
            assert!(w[1].sse_px < w[0].sse_px && w[1].zone_r >= w[0].zone_r && w[1].gpu_budget > w[0].gpu_budget && w[1].fetches > w[0].fetches);
        }
        assert!(p.iter().all(|p| p.zone_r <= ZONE_R && p.fetches <= WORKERS && p.zone_r - p.band() > COL_DIST * 0.5));
        assert_eq!(Params::of(Detail::High).band(), 1600.0);
    }

    #[test]
    fn wgs84_round_trip() {
        for &(lat, lon, h) in &[(37.6188f64, -122.375f64, -30.0), (0.0, 0.0, 0.0), (-45.0, 170.0, 2500.0), (89.9, 10.0, 100.0)] {
            let p = lla_to_ecef(lat.to_radians(), lon.to_radians(), h);
            let (la, lo, hh) = ecef_to_lla(p);
            assert!((la.to_degrees() - lat).abs() < 1e-9, "{la} {lat}");
            assert!((lo.to_degrees() - lon).abs() < 1e-9);
            assert!((hh - h).abs() < 1e-3, "{hh} {h}");
        }
    }

    #[test]
    fn urls() {
        let base = "/v1/3dtiles/datasets/A/files/B.json?session=S1";
        assert_eq!(resolve(base, "/v1/3dtiles/datasets/A/files/C.glb", session_of(base)), "/v1/3dtiles/datasets/A/files/C.glb?session=S1");
        assert_eq!(resolve(base, "D.glb?session=S2", session_of(base)), "/v1/3dtiles/datasets/A/files/D.glb?session=S2");
        assert_eq!(resolve("/v1/3dtiles/root.json", "https://tile.googleapis.com/v1/x.json?session=Z", None), "/v1/x.json?session=Z");
        assert_eq!(session_of("/a?key=1&session=Q"), Some("Q"));
    }

    #[test]
    fn datum_peak() {
        // A third bare ground at +32 (noisy), the rest roofs 5..150 m up.
        let mut d: Vec<f32> = (0..300).map(|i| 32.0 + ((i * 37 % 17) as f32 - 8.0) * 0.2).collect();
        d.extend((0..600).map(|i| 32.0 - 5.0 - (i * 53 % 145) as f32));
        d.sort_by(|a, b| a.total_cmp(b));
        let p = ground_peak(&d).unwrap();
        assert!((p - 32.0).abs() < 0.5, "{p}");
        // Downtown, off our buildings' roofs: streets, the roofs the map lacks, walls.
        let mut d: Vec<f32> = (0..300).map(|i| 32.0 + ((i * 37 % 17) as f32 - 8.0) * 0.2).collect();
        d.extend((0..150).map(|i| -10.0 + ((i * 29 % 13) as f32 - 6.0) * 0.15));
        d.extend((0..400).map(|i| 32.0 - 3.0 - (i * 53 % 200) as f32));
        d.sort_by(|a, b| a.total_cmp(b));
        let p = ground_peak(&d).unwrap();
        assert!((p - 32.0).abs() < 0.5, "{p}");
    }

    #[test]
    fn obb_distance() {
        let b = Obb { c: DVec3::ZERO, h: [DVec3::X * 2.0, DVec3::Y, DVec3::Z] };
        assert_eq!(b.dist(DVec3::new(1.0, 0.5, 0.0)), 0.0);
        assert!((b.dist(DVec3::new(5.0, 0.0, 0.0)) - 3.0).abs() < 1e-9);
        assert!((b.dist(DVec3::new(2.0, 4.0, 5.0)) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn column_heights() {
        // Two triangles making a 10 m square, 3 m above an origin on the ground.
        let o = geo::lla_to_pos(0.6, -2.1, 0.0);
        let (e, n, u) = geo::enu(o);
        let pts = [(-5.0, -5.0), (5.0, -5.0), (5.0, 5.0), (-5.0, 5.0)];
        let pos: Vec<Vec3> = pts.iter().map(|&(x, y)| (e * x + n * y + u * 3.0).as_vec3()).collect();
        let m = ColMesh::build(o, &pos, &[0, 1, 2, 0, 2, 3]);
        let h = m.top(o + e * 1.0 + n * 2.0).unwrap();
        assert!((h - 3.0).abs() < 0.01, "{h}");
        assert!(m.top(o + e * 7.0).is_none());
    }

    /// Decodes a tile saved by hand (TILE3D=path.glb); never part of a normal run.
    #[test]
    fn atlas_packing() {
        let cases: [&[(u32, u32)]; 6] = [&[(256, 512), (256, 256)], &[(512, 512), (256, 256)], &[(512, 512), (256, 512)], &[(512, 512), (256, 512), (256, 256)], &[(512, 1024), (256, 256)], &[(1, 1), (256, 256), (64, 128)]];
        for sizes in cases {
            let (w, h, at) = pack(sizes);
            let boxes: Vec<(u32, u32, u32, u32)> = sizes.iter().zip(&at).map(|(s, a)| (a.0, a.1, s.0 + 2 * ATLAS_PAD, s.1 + 2 * ATLAS_PAD)).collect();
            let used: u32 = boxes.iter().map(|b| b.2 * b.3).sum();
            for (k, a) in boxes.iter().enumerate() {
                assert!(a.0 + a.2 <= w && a.1 + a.3 <= h, "{sizes:?}: out of the atlas");
                for b in &boxes[k + 1..] {
                    assert!(a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1, "{sizes:?}: overlap");
                }
            }
            // No more than a quarter wasted on the size mixes Google sends.
            assert!(used as f64 / (w * h) as f64 > 0.75, "{sizes:?}: {w}x{h} for {used}");
        }
        // The padding repeats the edge texels.
        let src: Vec<u8> = (0..4u8).flat_map(|i| [i * 60, 0, 0, 255]).collect();
        let (aw, ah) = (2 + 2 * ATLAS_PAD, 2 + 2 * ATLAS_PAD);
        let mut dst = vec![0u8; (aw * ah * 4) as usize];
        blit_padded(&mut dst, aw, 0, 0, 2, 2, &src);
        assert_eq!(dst[0], 0);
        assert_eq!(dst[((aw * ah - 1) * 4) as usize], 180);
    }

    #[test]
    #[ignore]
    fn decode_sample() {
        let p = std::env::var("TILE3D").expect("TILE3D=path to a .glb");
        let m = decode(&std::fs::read(p).unwrap()).unwrap();
        let (lat, lon, h) = geo::pos_to_lla(m.origin);
        println!("{} verts {} tris tex {}x{} at {:.5},{:.5} h {:.1} credits {:?}", m.verts.len(), m.idx.len() / 3, m.mips[0].0, m.mips[0].1, lat.to_degrees(), lon.to_degrees(), h, m.copyright);
        assert!(!m.verts.is_empty());
    }
}
