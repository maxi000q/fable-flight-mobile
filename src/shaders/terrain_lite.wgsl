// The safe-mode terrain (StartupProfile::terrain_lite). The full module — the
// photo-scan hex tiling, the parallax occlusion march, the per-class ground
// materials, the relief the tessellator walks over, the city glow — is the
// third shader an Adreno 619 reliably loses its GPU device to while compiling
// (1.0.15: attempts 3 and 4 both died at the terrain pipeline, after the
// composite had already fallen back to its own safe mode). This one keeps the
// same bindings, vertex formats and G-buffer output — every entry point the
// terrain pipelines ask for — but shades the ground from the satellite photo
// alone: the same water aux data (the composite's water needs it), a plain
// lambert over the mesh normal through shade(), and the photo-mesh cut-out so
// the 3D tiles keep standing in for the ground where they cover it. No loops,
// no marches, no texture arrays: a few dozen instructions the driver cannot
// choke on. The tessellator still snaps its edges to the coarse lattice
// (copied from the full shader), only without the relief displacement — the
// surface it makes is the same one the plain cells draw, so they still join.

// offset: xyz tile centre relative to the camera, w = grid cell size (m).
// info: x = zoom, yz = the tile's corner in building tiles, w = corner bits.
// tess: the block of cells drawn tessellated (x0, y0, w, h).
struct Tile { offset: vec4f, info: vec4f, tess: vec4f };
@group(1) @binding(0) var<uniform> T: Tile;
@group(1) @binding(1) var t_img: texture_2d<f32>;
// R water coverage, G water class * 80, B ground class | seed << 4.
@group(1) @binding(2) var t_mask: texture_2d<f32>;
// The tile's own vertex buffer (10 floats per vertex), read by the tessellator.
@group(1) @binding(3) var<storage, read> TV: array<f32>;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) N: vec3f,
  @location(2) uv: vec2f,
  @location(3) depth: f32,
};
struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };

const GRID = 64u;
const GV = 65u;

// ------------------------------------------------------------ vertices
@vertex fn vs(@location(0) pos: vec3f, @location(1) depth: f32, @location(2) nrm: vec3f, @location(3) skirt: f32, @location(4) uv: vec2f) -> VOut {
  let P = pos + T.offset.xyz;
  return VOut(G.view_proj * vec4f(P, 1.0), P, nrm, uv, depth);
}

// Tessellation without a tessellation stage, as in the full shader: every
// coarse grid cell near the camera is drawn as a TN x TN patch whose vertices
// snap to a coarser lattice with distance, so the surface stays closed — only
// the relief that the full one raises the patches by is gone.
const TN = 16u;
const TESS_K = 0.03;

struct CV { P: vec3f, depth: f32, N: vec3f, uv: vec2f };
fn cv(i: u32) -> CV {
  let b = i * 10u;
  return CV(vec3f(TV[b], TV[b + 1u], TV[b + 2u]) + T.offset.xyz, TV[b + 3u], vec3f(TV[b + 4u], TV[b + 5u], TV[b + 6u]), vec2f(TV[b + 8u], TV[b + 9u]));
}
fn lerp_cv(a: CV, b: CV, t: f32) -> CV {
  let s = 1.0 - t;
  return CV(a.P * s + b.P * t, a.depth * s + b.depth * t, a.N * s + b.N * t, a.uv * s + b.uv * t);
}
fn tess_level(mid: vec3f) -> f32 {
  let sp = max(length(mid) * TESS_K, T.offset.w / f32(TN));
  return clamp(exp2(floor(log2(max(T.offset.w / sp, 1.0)))), 1.0, f32(TN));
}
fn snapk(k: u32, lv: f32) -> f32 { return round(f32(k) * lv / f32(TN)) / lv; }

@vertex fn vs_tess(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  let rw = u32(T.tess.z);
  let ci = u32(T.tess.x) + ii % rw;
  let cj = u32(T.tess.y) + ii / rw;
  let a = vi % (TN + 1u);
  let b = vi / (TN + 1u);
  let i00 = cj * GV + ci;
  let c00 = cv(i00);
  let c10 = cv(i00 + 1u);
  let c01 = cv(i00 + GV);
  let c11 = cv(i00 + GV + 1u);
  var v: CV;
  if (b == 0u) {
    v = lerp_cv(c00, c10, snapk(a, tess_level((c00.P + c10.P) * 0.5)));
  } else if (b == TN) {
    v = lerp_cv(c01, c11, snapk(a, tess_level((c01.P + c11.P) * 0.5)));
  } else if (a == 0u) {
    v = lerp_cv(c00, c01, snapk(b, tess_level((c00.P + c01.P) * 0.5)));
  } else if (a == TN) {
    v = lerp_cv(c10, c11, snapk(b, tess_level((c10.P + c11.P) * 0.5)));
  } else {
    let lv = tess_level((c00.P + c11.P) * 0.5);
    let fa = snapk(a, lv);
    let fb = snapk(b, lv);
    // Same diagonal as the coarse mesh.
    if (fa + fb <= 1.0) {
      v = CV(c00.P + (c10.P - c00.P) * fa + (c01.P - c00.P) * fb, c00.depth, c00.N + (c10.N - c00.N) * fa + (c01.N - c00.N) * fb, c00.uv + (c10.uv - c00.uv) * fa + (c01.uv - c00.uv) * fb);
    } else {
      let ga = 1.0 - fa;
      let gb = 1.0 - fb;
      v = CV(c11.P + (c01.P - c11.P) * ga + (c10.P - c11.P) * gb, c11.depth, c11.N + (c01.N - c11.N) * ga + (c10.N - c11.N) * gb, c11.uv + (c01.uv - c11.uv) * ga + (c10.uv - c11.uv) * gb);
    }
  }
  return VOut(G.view_proj * vec4f(v.P, 1.0), v.P, v.N, v.uv, v.depth);
}

// The rest of a tessellated tile, one quad per cell; the cells drawn by
// vs_tess collapse to nothing.
@vertex fn vs_cells(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  let ci = ii % GRID;
  let cj = ii / GRID;
  let t = vec4u(T.tess);
  if (ci >= t.x && ci < t.x + t.z && cj >= t.y && cj < t.y + t.w) {
    return VOut(vec4f(2.0, 2.0, 2.0, 1.0), vec3f(0.0), vec3f(0.0, 0.0, 1.0), vec2f(0.0), 0.0);
  }
  let v = cv(cj * GV + ci + (vi & 1u) + (vi >> 1u) * GV);
  return VOut(G.view_proj * vec4f(v.P, 1.0), v.P, v.N, v.uv, v.depth);
}

// ------------------------------------------------------------ surface
@fragment fn fs(in: VOut) -> GOut {
  // Derivatives first, while every pixel of the quad is still running.
  let img = textureSample(t_img, s_aniso, in.uv).rgb;
  let m = textureSample(t_mask, s_clamp, in.uv);
  if (m.r > 0.5) {
    // Water: the same aux data the full terrain writes — the composite's
    // water (full or safe mode) reads it for depth, class and shoreline.
    let wclass = round(m.g * 255.0 / 80.0);
    let shore = smoothstep(0.5, 1.0, textureSampleLevel(t_mask, s_clamp, in.uv, 3.0).r);
    let profile = 0.35 + shore * shore * select(select(9.0, 3.5, wclass > 2.5), 14.0, wclass < 1.5);
    var depth = max(mix(min(in.depth, 1.2), in.depth, shore), profile);
    return GOut(vec4f(img, 1.0), vec4f(max(depth, 0.15), wclass, shore, 1.0), 0.0);
  }
  // Inside the 3D tiles zone the mesh stands in for the land; our ground
  // gives way to it exactly as in the full shader.
  if (photo_hides(in.P, in.clip.xy)) { discard; }
  var N = in.N;
  if (dot(N, N) < 1e-8) { N = in.P - G.planet.xyz; }
  N = normalize(N);
  // The satellite photo as the albedo, lit like any other surface.
  let c = shade(in.P, N, img);
  return GOut(vec4f(c, 1.0), vec4f(N, 0.0), amb_share(c));
}

// ------------------------------------------------------------ the masks
// (Copied from the full shader: the photo mesh gives way to our water and to
// the ground's presence in the shore mask — these are what photo.wgsl and
// shore.rs read back, and they are cheap.)
@group(2) @binding(0) var<uniform> CS: Cascade;
struct TopOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) h: f32, @location(2) skirt: f32 };
@vertex fn vs_top(@location(0) pos: vec3f, @location(3) skirt: f32, @location(4) uv: vec2f) -> TopOut {
  let P = pos + T.offset.xyz;
  return TopOut(CS.vp * vec4f(P, 1.0), uv, height_of(P), skirt);
}
@fragment fn fs_top(in: TopOut) -> @location(0) vec4f {
  if (in.skirt > 0.5 || textureSampleLevel(t_mask, s_clamp, in.uv, 0.0).r < 0.5) { discard; }
  return vec4f(0.0, in.h, 0.0, 0.0);
}
// Where any ground was drawn at all, into the red channel of the shore mask.
@fragment fn fs_cover(in: TopOut) -> @location(0) vec4f {
  if (in.skirt > 0.5) { discard; }
  return vec4f(1.0, 0.0, 0.0, 0.0);
}
