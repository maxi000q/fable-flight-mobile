// Distance to the shore by jump flooding (shore.rs). Seeds are land texels,
// packed x | y << 16; NONE where no land has been found yet.
struct U { step: u32, n: u32, texel: f32, maxd: f32 };
@group(0) @binding(0) var<uniform> P: U;
// Our water from above: r = ground drawn here, g = water height (-1e4 where none).
@group(0) @binding(1) var t_mask: texture_2d<f32>;
@group(0) @binding(2) var<storage, read> src: array<u32>;
@group(0) @binding(3) var<storage, read_write> dst: array<u32>;
@group(0) @binding(4) var out: texture_storage_2d<rgba16float, write>;
// The field's next finer level, for its mips.
@group(0) @binding(5) var t_fine: texture_2d<f32>;
// The sea (written when a rebuild starts): direction the wind blows toward
// (east, north), wind (m/s), open-ocean breaker height (m); then metres per
// wide texel, texels across the exposure grid, the wide mask's size, the
// longest fetch looked for (wide texels).
struct Sea { wdir: vec2f, wind: f32, hopen: f32, wtexel: f32, en: u32, wn: u32, reach: f32 };
@group(0) @binding(6) var<uniform> S: Sea;
// The same as `t_mask` over a much wider square (shore.rs `WIDE_N`), coarser.
@group(0) @binding(7) var t_wide: texture_2d<f32>;
// Exposure of the water around the field's square, at the wide texels: how
// open it is to the sea (share of directions with no land within reach), and
// the fetch upwind (m).
@group(0) @binding(8) var expose_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(9) var t_expose: texture_2d<f32>;

const NONE = 0xffffffffu;

// Land is ground that is not water; where nothing was drawn (outside every
// tile we have) is neither, and seeds nothing.
@compute @workgroup_size(8, 8) fn seed(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= P.n || id.y >= P.n) { return; }
  let m = textureLoad(t_mask, vec2i(id.xy), 0);
  let land = m.r > 0.5 && m.g < -5000.0;
  dst[id.y * P.n + id.x] = select(NONE, id.x | (id.y << 16u), land);
}

fn seed_pos(s: u32) -> vec2i { return vec2i(i32(s & 0xffffu), i32(s >> 16u)); }

@compute @workgroup_size(8, 8) fn jump(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= P.n || id.y >= P.n) { return; }
  let p = vec2i(id.xy);
  let n = i32(P.n);
  let k = i32(P.step);
  var best = NONE;
  var bd = 0x7fffffff;
  for (var dy = -1; dy <= 1; dy++) {
    for (var dx = -1; dx <= 1; dx++) {
      let q = p + vec2i(dx, dy) * k;
      if (any(q < vec2i(0)) || any(q >= vec2i(n))) { continue; }
      let s = src[u32(q.y * n + q.x)];
      if (s == NONE) { continue; }
      let v = seed_pos(s) - p;
      let d = v.x * v.x + v.y * v.y;
      if (d < bd) { bd = d; best = s; }
    }
  }
  dst[id.y * P.n + id.x] = best;
}

// ------------------------------------------------------------ exposure
// Swell comes in from the open sea only, so what reaches a shore depends on
// how much of the horizon is open water; the wind's own waves grow with the
// stretch of water upwind (the fetch). Rays from every texel of the exposure
// grid (the middle of the wide mask) find both. One band of rows a frame:
// P.step is the band.
const RAYS = 32;
const BANDS = 8u;   // shore.rs JUMPS.len()

fn wide_land(q: vec2f) -> i32 {
  // 1 land, 0 water (or nothing drawn), -1 off the mask
  let n = i32(S.wn);
  let p = vec2i(floor(q));
  if (any(p < vec2i(0)) || any(p >= vec2i(n))) { return -1; }
  let m = textureLoad(t_wide, p, 0);
  return select(0, 1, m.r > 0.5 && m.g < -5000.0);
}

@compute @workgroup_size(8, 8) fn expose(@builtin(global_invocation_id) id: vec3u) {
  let rows = S.en / BANDS;
  if (id.x >= S.en || id.y >= rows) { return; }
  let e = vec2u(id.x, id.y + P.step * rows);
  let start = vec2f(e + (S.wn - S.en) / 2u) + 0.5;
  // (from a texel of land by the water: across the land first, a little way)
  let on_land = wide_land(start) == 1;
  var open = 0.0;
  var fsum = 0.0;
  var csum = 0.0;
  let up = -S.wdir;
  for (var k = 0; k < RAYS; k++) {
    let a = (f32(k) + 0.5) * 6.2831853 / f32(RAYS);
    let dn = vec2f(cos(a), sin(a));            // east, north
    let dir = vec2f(dn.x, -dn.y);              // texels: rows run north to south
    var wet = !on_land;
    var t = 0.0;
    var fetch = S.reach;
    loop {
      t += select(select(4.0, 2.0, t < 96.0), 1.0, t < 24.0);
      if (t >= S.reach) { break; }
      let l = wide_land(start + dir * t);
      if (l < 0) { break; }
      if (l == 1) {
        if (wet) { fetch = t; break; }
        if (t > 4.0) { fetch = 0.0; break; }
      } else {
        wet = true;
      }
    }
    open += smoothstep(0.6 * S.reach, S.reach, fetch);
    // The effective fetch over 45 degrees either side of upwind.
    let c = dot(dn, up);
    if (c > 0.7) { fsum += fetch * c * c; csum += c; }
  }
  let f = select(0.0, fsum / csum, csum > 0.0) * S.wtexel;
  textureStore(expose_out, vec2i(e), vec4f(open / f32(RAYS), f, 0.0, 1.0));
}

// The exposure grid at field texel p, bilinear.
fn exposure_at(p: vec2i) -> vec2f {
  let e = (vec2f(p) + 0.5 - f32(P.n) * 0.5) * (P.texel / S.wtexel) + f32(S.en) * 0.5 - 0.5;
  let i = vec2i(floor(e));
  let fr = e - floor(e);
  let mx = i32(S.en) - 1;
  let a = textureLoad(t_expose, clamp(i, vec2i(0), vec2i(mx)), 0).xy;
  let b = textureLoad(t_expose, clamp(i + vec2i(1, 0), vec2i(0), vec2i(mx)), 0).xy;
  let c = textureLoad(t_expose, clamp(i + vec2i(0, 1), vec2i(0), vec2i(mx)), 0).xy;
  let d = textureLoad(t_expose, clamp(i + vec2i(1, 1), vec2i(0), vec2i(mx)), 0).xy;
  return mix(mix(a, b, fr.x), mix(c, d, fr.x), fr.y);
}

// How tall the breakers come in here (m): the open sea's swell as far as the
// horizon is open to it, or the wind's own waves over the fetch upwind
// (fetch-limited growth, Hasselmann et al. 1973: Hs = 0.0016 U sqrt(F / g),
// no more than fully developed), whichever is the taller.
fn breaker_height(p: vec2i) -> f32 {
  let x = exposure_at(p);
  let swell = S.hopen * smoothstep(0.06, 0.38, x.x);
  let u = S.wind;
  let wind = min(0.0016 * u * sqrt(max(x.y, 0.0) / 9.81), 0.24 * u * u / 9.81);
  return max(swell, wind);
}

// The field: distance to the nearest land (m, from the land texel's edge),
// direction away from it (east, north; texture rows run north to south), and
// how tall the breakers come in there (m).
@compute @workgroup_size(8, 8) fn resolve(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= P.n || id.y >= P.n) { return; }
  let p = vec2i(id.xy);
  let s = src[id.y * P.n + id.x];
  var o = vec4f(P.maxd, 0.0, 0.0, breaker_height(p));
  if (s != NONE) {
    let v = vec2f(p - seed_pos(s));
    let l = length(v);
    o.x = min(max(l - 0.5, 0.0) * P.texel, P.maxd);
    if (l > 0.0) { o = vec4f(o.x, v.x / l, -v.y / l, o.w); }
  }
  textureStore(out, p, o);
}

// The next mip level: the mean of four.
@compute @workgroup_size(8, 8) fn mip(@builtin(global_invocation_id) id: vec3u) {
  let n = textureDimensions(out);
  if (id.x >= n.x || id.y >= n.y) { return; }
  let p = vec2i(id.xy) * 2;
  let o = textureLoad(t_fine, p, 0) + textureLoad(t_fine, p + vec2i(1, 0), 0) + textureLoad(t_fine, p + vec2i(0, 1), 0) + textureLoad(t_fine, p + vec2i(1, 1), 0);
  textureStore(out, vec2i(id.xy), o * 0.25);
}
