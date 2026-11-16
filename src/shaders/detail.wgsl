// Ground material library, generated once at start-up. Every layer tiles.
// Albedo array: rgb = colour (the terrain shader divides by the layer's mean,
// so only the variation around the satellite colour is used), a = height 0..1. Normal array: rg = surface normal xy,
// b = cavity (1 = open, 0 = deep in a crack), a = height again (for mips).
// Layers: 0 grass, 1 soil, 2 rock, 3 sand, 4 asphalt / concrete grit,
// 5 leaf litter, 6 cereal, 7 moss; 8 and up are blank (flat, mid grey) until
// the mid-scale photo scans replace them (materials.rs).
@group(0) @binding(0) var o_alb: texture_storage_2d_array<rgba8unorm, write>;
@group(0) @binding(1) var i_alb: texture_2d_array<f32>;
@group(0) @binding(2) var o_nrm: texture_storage_2d_array<rgba8unorm, write>;

// N (the array side) is injected before this file by the renderer: 512 on
// a phone, 1024 on a desktop.
const TAU = 6.28318530718;
// Metres covered by one repeat, and metres of relief between height 0 and 1
// (keep in step with DET_PROC in render.rs).
const SIZE = array<f32, 13>(1.0, 2.0, 4.0, 2.0, 2.0, 2.0, 1.0, 2.0, 20.0, 20.0, 20.0, 20.0, 20.0);
const RANGE = array<f32, 13>(0.05, 0.06, 0.45, 0.03, 0.006, 0.035, 0.08, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0);
// Normal map strength: tiny blades and grit would otherwise tilt every normal
// steeply and read as noise (and steal sunlight on average).
const BUMP = array<f32, 13>(0.25, 0.7, 1.0, 0.8, 0.5, 0.5, 0.25, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0);

// Integer hashes of the float bits (sin-based hashes streak at high frequency).
fn pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}
fn hu(p: vec2f) -> u32 { return pcg(bitcast<u32>(p.x) ^ pcg(bitcast<u32>(p.y) + 0x9e3779b9u)); }
fn h1(p: vec2f) -> f32 { return f32(hu(p) >> 8u) / 16777216.0; }
fn h2(p: vec2f) -> vec2f {
  let a = hu(p);
  return vec2f(f32(a >> 8u) / 16777216.0, f32(pcg(a) >> 8u) / 16777216.0);
}
fn wrapc(c: vec2f, f: f32) -> vec2f { return c - f * floor(c / f); }

// Periodic value noise over the unit square at integer frequency f.
fn vnoise(p: vec2f, f: f32) -> f32 {
  let x = p * f;
  let i = floor(x);
  let u = fract(x);
  let s = u * u * (3.0 - 2.0 * u);
  let a = h1(wrapc(i, f));
  let b = h1(wrapc(i + vec2f(1.0, 0.0), f));
  let c = h1(wrapc(i + vec2f(0.0, 1.0), f));
  let d = h1(wrapc(i + vec2f(1.0, 1.0), f));
  return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}
fn fbm(p: vec2f, f: f32, oct: i32) -> f32 {
  var v = 0.0;
  var a = 0.5;
  var ff = f;
  var t = 0.0;
  for (var i = 0; i < oct; i++) {
    v += a * vnoise(p + vec2f(f32(i) * 0.137, f32(i) * 0.291), ff);
    t += a;
    a *= 0.5;
    ff *= 2.0;
  }
  return v / t;
}
fn ridged(p: vec2f, f: f32, oct: i32) -> f32 {
  var v = 0.0;
  var a = 0.5;
  var ff = f;
  var t = 0.0;
  for (var i = 0; i < oct; i++) {
    v += a * (1.0 - abs(vnoise(p + vec2f(f32(i) * 0.31), ff) * 2.0 - 1.0));
    t += a;
    a *= 0.55;
    ff *= 2.0;
  }
  return v / t;
}
// Periodic Worley: x = F1, y = F2 (in cells), zw = id of the nearest cell.
fn worley(p: vec2f, f: f32, jit: f32) -> vec4f {
  let x = p * f;
  let c = floor(x);
  var d1 = 9.0;
  var d2 = 9.0;
  var id = vec2f(0.0);
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let o = vec2f(f32(i), f32(j));
      let cell = wrapc(c + o, f);
      let d = length(c + o + 0.5 + (h2(cell) - 0.5) * jit - x);
      if (d < d1) { d2 = d1; d1 = d; id = cell; } else if (d < d2) { d2 = d; }
    }
  }
  return vec4f(d1, d2, id);
}
// Distance from x to the segment a..b.
fn seg(x: vec2f, a: vec2f, b: vec2f) -> vec2f {
  let ab = b - a;
  let t = clamp(dot(x - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
  return vec2f(length(x - a - ab * t), t);
}

struct Texel { c: vec3f, h: f32 };

// Blades seen from above: strokes scattered over cells, the highest one wins.
fn blades(p: vec2f, f: f32, len: f32, wid: f32, lean: vec2f, seed: f32, fine: bool) -> vec4f {
  let x = p * f;
  let c = floor(x);
  var best = vec4f(0.0, 0.0, 0.0, -1.0); // tint seed, along, unused, height
  for (var j = -2; j <= 2; j++) {
    for (var i = -2; i <= 2; i++) {
      let o = vec2f(f32(i), f32(j));
      let cell = wrapc(c + o, f);
      for (var k = 0; k < 2; k++) {
        let r = h2(cell * 1.31 + vec2f(f32(k) * 7.7 + seed, seed * 3.1));
        let r2 = h2(cell * 2.17 + vec2f(f32(k) * 3.3 - seed, 5.1));
        let base = c + o + r;
        let ang = r2.x * TAU;
        let l = len * (0.5 + 0.7 * r2.y);
        let dir = normalize(vec2f(cos(ang), sin(ang)) + lean);
        let tip = base + dir * l;
        let s = seg(x, base, tip);
        let w = wid * (1.0 - 0.85 * s.y) * select(1.0, 0.6, fine);
        if (s.x < w) {
          // Blades rise toward the tip (seen from above, the tip is highest).
          let hgt = (0.35 + 0.65 * r.y) * (0.45 + 0.55 * s.y) * (1.0 - 0.3 * (s.x / w));
          if (hgt > best.w) { best = vec4f(h1(cell + f32(k)), s.y, 0.0, hgt); }
        }
      }
    }
  }
  return best;
}

fn grass(p: vec2f) -> Texel {
  let clump = fbm(p, 6.0, 4);
  let lean = (vec2f(vnoise(p, 4.0), vnoise(p + 0.5, 4.0)) - 0.5) * 1.6;
  let b1 = blades(p, 30.0, 1.7, 0.16, lean, 0.0, false);
  let b2 = blades(p, 48.0, 1.5, 0.15, lean * 0.6, 1.7, false);
  let b3 = blades(p, 64.0, 1.4, 0.15, lean * 0.3, 3.9, true);
  var b = b1;
  if (b2.w * 0.9 > b.w) { b = vec4f(b2.xyz, b2.w * 0.9); }
  if (b3.w * 0.75 > b.w) { b = vec4f(b3.xyz, b3.w * 0.75); }
  // Older growth and thatch in the gaps.
  let thatch = fbm(p, 64.0, 3);
  var c = mix(vec3f(0.24, 0.28, 0.14), vec3f(0.40, 0.40, 0.24), thatch);
  var h = 0.12 * thatch;
  if (b.w > 0.0) {
    let dry = step(0.86, b.x) + step(0.62, fbm(p + 0.3, 8.0, 3)) * step(0.55, b.x);
    let green = mix(mix(vec3f(0.36, 0.54, 0.20), vec3f(0.50, 0.62, 0.26), b.x), vec3f(0.30, 0.46, 0.22), step(0.8, fract(b.x * 7.0)));
    let tip = mix(green, vec3f(0.70, 0.70, 0.42), 0.3 * b.y * b.y);
    c = mix(tip, vec3f(0.76, 0.70, 0.44), clamp(dry, 0.0, 1.0) * 0.8);
    c *= 0.75 + 0.45 * b.w;
    h = 0.2 + 0.8 * b.w * (0.75 + 0.5 * clump);
  }
  c *= 0.85 + 0.3 * clump;
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn soil(p: vec2f) -> Texel {
  let base = fbm(p, 4.0, 5);
  let clod = worley(p, 26.0, 1.0);
  let cl = clamp(1.0 - clod.x * 1.2, 0.0, 1.0);
  var h = 0.35 * base + 0.4 * sqrt(cl) * (0.6 + 0.4 * h1(clod.zw));
  var c = vec3f(0.45, 0.36, 0.26) * (0.85 + 0.3 * h1(clod.zw + 3.0));
  // Fine crumbs.
  let crumb = fbm(p, 128.0, 2);
  h += 0.12 * crumb;
  // Pebbles.
  let pb = worley(p, 70.0, 0.9);
  if (h1(pb.zw + 9.0) < 0.05 && pb.x < 0.38) {
    let k = sqrt(1.0 - pb.x / 0.42);
    h = max(h, 0.55 + 0.4 * k);
    c = mix(vec3f(0.46, 0.42, 0.37), vec3f(0.58, 0.53, 0.46), h1(pb.zw));
  }
  // Dry cracks.
  let ck = worley(p, 7.0, 1.0);
  let crack = (1.0 - smoothstep(0.0, 0.03, ck.y - ck.x)) * smoothstep(0.58, 0.68, fbm(p, 3.0, 2));
  h -= 0.3 * crack;
  c *= 1.0 - 0.35 * crack;
  // Damper (darker) in the hollows.
  c *= 0.8 + 0.3 * smoothstep(0.2, 0.7, h);
  return Texel(c, clamp(h, 0.0, 1.0));
}

// Position relative to the nearest Worley feature point (in cells).
fn plate_offset(p: vec2f, f: f32, jit: f32) -> vec2f {
  let x = p * f;
  let c = floor(x);
  var d1 = 9.0;
  var off = vec2f(0.0);
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let o = vec2f(f32(i), f32(j));
      let d = x - (c + o + 0.5 + (h2(wrapc(c + o, f)) - 0.5) * jit);
      if (length(d) < d1) { d1 = length(d); off = d; }
    }
  }
  return off;
}

// Voronoi with the ids of both nearest cells, so each edge can decide on its
// own whether it is a crack. x = F1, y = F2, z = hash of the edge, w = hash of the cell.
fn edges(p: vec2f, f: f32, jit: f32) -> vec4f {
  let x = p * f;
  let c = floor(x);
  var d1 = 9.0;
  var d2 = 9.0;
  var i1 = vec2f(0.0);
  var i2 = vec2f(0.0);
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let o = vec2f(f32(i), f32(j));
      let cell = wrapc(c + o, f);
      let d = length(c + o + 0.5 + (h2(cell) - 0.5) * jit - x);
      if (d < d1) { d2 = d1; i2 = i1; d1 = d; i1 = cell; } else if (d < d2) { d2 = d; i2 = cell; }
    }
  }
  return vec4f(d1, d2, h1(i1 + i2 + abs(i1 - i2) * 0.37), h1(i1));
}

fn rock(p: vec2f) -> Texel {
  // Large slabs: smooth, gently warped, a few steps where sheets have split off.
  let warp = vec2f(fbm(p, 3.0, 3), fbm(p + 0.5, 3.0, 3)) - 0.5;
  let q = p + warp * 0.08;
  let slab = edges(q, 3.0, 0.9);
  let open = step(slab.z, 0.45);
  let crack = (1.0 - smoothstep(0.0, 0.035, slab.y - slab.x)) * open;
  let sheet = 0.12 * (slab.w - 0.5);
  // Joints: long, nearly parallel fractures that run through everything.
  let jl = q.y * 4.0 + 0.6 * fbm(q, 2.0, 3) + 0.15 * q.x;
  let jd = abs(fract(jl) - 0.5) / 4.0;
  let jvis = smoothstep(0.45, 0.6, fbm(q + 0.3, 2.0, 2));
  let joint = (1.0 - smoothstep(0.0, 0.006, jd)) * jvis;
  // Small fractures, only in places.
  let fr = edges(q, 11.0, 1.0);
  let crack2 = (1.0 - smoothstep(0.0, 0.03, fr.y - fr.x)) * step(fr.z, 0.3) * smoothstep(0.5, 0.65, fbm(q + 0.7, 4.0, 2));
  // Weathered surface: soft bumps, granular grain.
  let bumps = fbm(p, 8.0, 5);
  let grain = vnoise(p, 384.0);
  var h = 0.55 + sheet + 0.25 * (bumps - 0.5) + 0.05 * grain;
  h -= 0.4 * crack + 0.35 * joint + 0.15 * crack2;
  // Crystals: salt and pepper.
  let xtal = vnoise(p + 0.21, 256.0);
  var c = vec3f(0.60, 0.59, 0.56) * (0.92 + 0.16 * (h1(vec2f(slab.w, 3.0)) - 0.5));
  c *= 0.85 + 0.3 * smoothstep(0.2, 0.8, bumps);
  c = mix(c, vec3f(0.25, 0.25, 0.25), smoothstep(0.72, 0.85, xtal) * 0.7);
  c = mix(c, vec3f(0.78, 0.76, 0.72), smoothstep(0.3, 0.15, xtal) * 0.5);
  // Iron and water streaks, and lichen patches.
  let stain = fbm(vec2f(p.x * 3.0, p.y * 0.6), 4.0, 4);
  c = mix(c, c * vec3f(0.92, 0.85, 0.76), smoothstep(0.55, 0.8, stain));
  let lich = edges(p, 26.0, 1.0);
  let lk = smoothstep(0.6, 0.3, lich.x) * step(0.74, fbm(p + 0.4, 5.0, 3) + 0.2 * lich.w);
  c = mix(c, select(vec3f(0.60, 0.62, 0.54), vec3f(0.66, 0.62, 0.44), lich.w > 0.8), lk * 0.45);
  // Dirt and darkness in the cracks.
  c = mix(c, vec3f(0.18, 0.16, 0.13), clamp(crack + joint + crack2 * 0.7, 0.0, 1.0) * 0.85);
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn sand(p: vec2f) -> Texel {
  let warp = fbm(p, 3.0, 3);
  let ph = fract((p.y + 0.12 * warp + 0.03 * vnoise(p, 9.0)) * 12.0);
  // Ripples: gentle stoss side, steep lee side.
  let rip = select(ph / 0.75, (1.0 - ph) / 0.25, ph > 0.75);
  let grain = vnoise(p, 256.0);
  var h = 0.25 + 0.5 * rip * (0.6 + 0.4 * fbm(p, 4.0, 2)) + 0.12 * grain;
  // Heavier dark minerals collect in the troughs.
  var c = vec3f(0.62, 0.56, 0.45) * (0.9 + 0.12 * grain) * (0.92 + 0.12 * rip);
  // Odd shell fragment or pebble.
  let sh = worley(p, 40.0, 1.0);
  if (h1(sh.zw) < 0.02 && sh.x < 0.22) {
    c = mix(vec3f(0.80, 0.78, 0.72), vec3f(0.42, 0.40, 0.38), step(0.5, h1(sh.zw + 1.0)));
    h += 0.2;
  }
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn asphalt(p: vec2f) -> Texel {
  // Aggregate in binder, stains, the odd crack.
  let ag = worley(p, 160.0, 1.0);
  let stone = smoothstep(0.5, 0.3, ag.x);
  let tone = h1(ag.zw);
  var c = mix(vec3f(0.36, 0.36, 0.35), mix(vec3f(0.48, 0.47, 0.45), vec3f(0.60, 0.58, 0.55), tone), stone);
  let stain = fbm(p, 5.0, 4);
  c *= 0.8 + 0.35 * stain;
  var h = 0.4 + 0.4 * stone * (0.6 + 0.4 * tone) + 0.1 * vnoise(p, 300.0);
  let ck = worley(p, 3.0, 1.0);
  let crack = (1.0 - smoothstep(0.0, 0.012, ck.y - ck.x)) * step(0.6, fbm(p + 0.7, 2.0, 2));
  h -= 0.5 * crack;
  c *= 1.0 - 0.6 * crack;
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn litter(p: vec2f) -> Texel {
  let humus = fbm(p, 16.0, 4);
  var c = vec3f(0.24, 0.19, 0.13) * (0.8 + 0.4 * humus);
  var h = 0.15 * humus;
  // Leaves: ellipses scattered over a cell grid, highest on top.
  let f = 14.0;
  let x = p * f;
  let ci = floor(x);
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let o = vec2f(f32(i), f32(j));
      let cell = wrapc(ci + o, f);
      for (var k = 0; k < 3; k++) {
        let r = h2(cell + vec2f(f32(k) * 5.3, 1.1));
        let r2 = h2(cell * 1.7 + vec2f(f32(k) * 2.9, 7.3));
        let a = r2.x * TAU;
        let d = x - (ci + o + r);
        let q = vec2f(dot(d, vec2f(cos(a), sin(a))), dot(d, vec2f(-sin(a), cos(a))));
        let sz = 0.4 + 0.45 * r2.y;
        let e = length(q / vec2f(sz, sz * (0.55 + 0.25 * h1(cell + 9.0))));
        let lh = 0.3 + 0.7 * h1(cell + f32(k) * 3.0);
        if (e < 1.0 && lh > h) {
          h = lh * (0.85 + 0.15 * (1.0 - e));
          let t = h1(cell * 3.1 + f32(k));
          c = mix(mix(vec3f(0.40, 0.30, 0.18), vec3f(0.52, 0.40, 0.22), t), vec3f(0.30, 0.24, 0.16), step(0.6, t));
          c = mix(c, vec3f(0.36, 0.36, 0.20), step(0.9, h1(cell * 5.3 + f32(k))));
          c *= 0.85 + 0.25 * (1.0 - e) - 0.25 * (1.0 - smoothstep(0.0, 0.06, abs(q.y)));
        }
      }
    }
  }
  // Twigs.
  let tw = seg(fract(p * 3.0 + vec2f(0.2, 0.7)), vec2f(0.1, 0.3), vec2f(0.8, 0.55));
  if (tw.x < 0.012 && h1(floor(p * 3.0 + vec2f(0.2, 0.7))) < 0.5) { h = 0.95; c = vec3f(0.30, 0.24, 0.17); }
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn cereal(p: vec2f) -> Texel {
  // Dense stalks seen from above: ears, leaves and deep shade between.
  let b = blades(p, 46.0, 0.8, 0.13, vec2f(0.3, 0.9), 4.2, false);
  let ears = worley(p, 60.0, 1.0);
  let ear = smoothstep(0.45, 0.2, ears.x) * step(0.35, h1(ears.zw));
  var c = vec3f(0.30, 0.28, 0.18);
  var h = 0.1;
  if (b.w > 0.0) {
    c = mix(vec3f(0.56, 0.54, 0.32), vec3f(0.64, 0.60, 0.36), b.x) * (0.75 + 0.35 * b.w);
    h = 0.25 + 0.6 * b.w;
  }
  if (ear > 0.0 && h < 0.75 + 0.2 * ear) {
    h = 0.75 + 0.2 * ear;
    c = vec3f(0.70, 0.64, 0.40) * (0.85 + 0.2 * h1(ears.zw + 1.0));
  }
  c *= 0.9 + 0.2 * fbm(p, 5.0, 3);
  return Texel(c, clamp(h, 0.0, 1.0));
}

fn moss(p: vec2f) -> Texel {
  let cu = worley(p, 12.0, 1.0);
  let cush = sqrt(clamp(1.0 - cu.x, 0.0, 1.0));
  let fuzz = fbm(p, 128.0, 3);
  var h = 0.6 * cush + 0.3 * fuzz;
  let t = fbm(p + 0.6, 6.0, 4) * 0.8 + 0.2 * h1(cu.zw);
  var c = mix(vec3f(0.36, 0.46, 0.20), vec3f(0.48, 0.52, 0.24), t) * (0.8 + 0.35 * fuzz);
  // Dead brown patches and dark wet gaps.
  c = mix(c, vec3f(0.42, 0.34, 0.20), smoothstep(0.62, 0.75, fbm(p + 0.2, 4.0, 3)));
  c *= 0.9 + 0.1 * smoothstep(0.0, 0.3, cu.y - cu.x + 0.05);
  h -= 0.15 * (1.0 - smoothstep(0.0, 0.2, cu.y - cu.x));
  return Texel(c, clamp(h, 0.0, 1.0));
}

@compute @workgroup_size(8, 8, 1)
fn gen(@builtin(global_invocation_id) id: vec3u) {
  let p = (vec2f(id.xy) + 0.5) / f32(N);
  var t: Texel;
  switch id.z {
    case 0u: { t = grass(p); }
    case 1u: { t = soil(p); }
    case 2u: { t = rock(p); }
    case 3u: { t = sand(p); }
    case 4u: { t = asphalt(p); }
    case 5u: { t = litter(p); }
    case 6u: { t = cereal(p); }
    case 7u: { t = moss(p); }
    default: { t = Texel(vec3f(0.35), 0.5); }
  }
  textureStore(o_alb, vec2i(id.xy), i32(id.z), vec4f(clamp(t.c, vec3f(0.0), vec3f(1.0)), t.h));
}

fn hat(x: i32, y: i32, l: i32) -> f32 {
  return textureLoad(i_alb, vec2i((x + N) % N, (y + N) % N), l, 0).a;
}
@compute @workgroup_size(8, 8, 1)
fn nrm(@builtin(global_invocation_id) id: vec3u) {
  let x = i32(id.x);
  let y = i32(id.y);
  let l = i32(id.z);
  // Sobel slope in metres per metre.
  let gx = (hat(x + 1, y - 1, l) + 2.0 * hat(x + 1, y, l) + hat(x + 1, y + 1, l)) - (hat(x - 1, y - 1, l) + 2.0 * hat(x - 1, y, l) + hat(x - 1, y + 1, l));
  let gy = (hat(x - 1, y + 1, l) + 2.0 * hat(x, y + 1, l) + hat(x + 1, y + 1, l)) - (hat(x - 1, y - 1, l) + 2.0 * hat(x, y - 1, l) + hat(x + 1, y - 1, l));
  let k = RANGE[l] / (SIZE[l] / f32(N)) / 8.0 * BUMP[l];
  let n = normalize(vec3f(-gx * k, -gy * k, 1.0));
  // Cavity: how far below its surroundings a texel sits.
  var avg = 0.0;
  for (var j = -3; j <= 3; j++) {
    for (var i = -3; i <= 3; i++) {
      avg += hat(x + i * 2, y + j * 2, l);
    }
  }
  avg /= 49.0;
  let h = hat(x, y, l);
  let cav = clamp(1.0 - (avg - h) * 2.2, 0.0, 1.0);
  textureStore(o_nrm, vec2i(id.xy), l, vec4f(n.xy * 0.5 + 0.5, cav, h));
}

// Box-filter one mip level into the next (both arrays share this).
@group(0) @binding(3) var m_src: texture_2d_array<f32>;
@group(0) @binding(4) var m_dst: texture_storage_2d_array<rgba8unorm, write>;
@compute @workgroup_size(8, 8, 1)
fn down(@builtin(global_invocation_id) id: vec3u) {
  let sz = textureDimensions(m_dst);
  if (id.x >= sz.x || id.y >= sz.y) { return; }
  let s = vec2i(id.xy) * 2;
  let l = i32(id.z);
  let v = textureLoad(m_src, s, l, 0) + textureLoad(m_src, s + vec2i(1, 0), l, 0) + textureLoad(m_src, s + vec2i(0, 1), l, 0) + textureLoad(m_src, s + vec2i(1, 1), l, 0);
  textureStore(m_dst, vec2i(id.xy), l, v * 0.25);
}
