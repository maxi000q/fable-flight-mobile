// Tiling 3D cloud noise: R = Perlin-Worley, GBA = Worley at rising frequency.
// The slices are written into a 2D array (layer = z) and copied into the 3D
// texture afterwards (render.rs): textureStore to a texture_storage_3d is the
// one pattern that faults some Adreno drivers outright — a start-up device
// loss ('driver implementation is at fault') on every 1.0.7-1.0.9 build —
// while every other storage write in the engine is 2D and safe.
@group(0) @binding(0) var out_tex: texture_storage_2d_array<rgba8unorm, write>;
const N = 128.0;

fn h3(p: vec3f) -> vec3f {
  var q = vec3f(dot(p, vec3f(127.1, 311.7, 74.7)), dot(p, vec3f(269.5, 183.3, 246.1)), dot(p, vec3f(113.5, 271.9, 124.6)));
  return fract(sin(q) * 43758.5453);
}
fn worley(p: vec3f, freq: f32) -> f32 {
  let x = p * freq;
  let c = floor(x);
  let f = x - c;
  var md = 1.0;
  for (var k = -1; k <= 1; k++) { for (var j = -1; j <= 1; j++) { for (var i = -1; i <= 1; i++) {
    let o = vec3f(f32(i), f32(j), f32(k));
    let cell = (c + o) - freq * floor((c + o) / freq);
    let d = o + h3(cell) - f;
    md = min(md, dot(d, d));
  } } }
  return 1.0 - clamp(sqrt(md), 0.0, 1.0);
}
fn grad(p: vec3f, freq: f32) -> vec3f {
  let cell = p - freq * floor(p / freq);
  return normalize(h3(cell) * 2.0 - 1.0);
}
fn perlin(p: vec3f, freq: f32) -> f32 {
  let x = p * freq;
  let c = floor(x);
  let f = x - c;
  let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  var v = 0.0;
  for (var k = 0; k <= 1; k++) { for (var j = 0; j <= 1; j++) { for (var i = 0; i <= 1; i++) {
    let o = vec3f(f32(i), f32(j), f32(k));
    let w = mix(1.0 - u.x, u.x, o.x) * mix(1.0 - u.y, u.y, o.y) * mix(1.0 - u.z, u.z, o.z);
    v += w * dot(grad(c + o, freq), f - o);
  } } }
  return v;
}
fn wfbm(p: vec3f, f: f32) -> f32 { return worley(p, f) * 0.625 + worley(p, f * 2.0) * 0.25 + worley(p, f * 4.0) * 0.125; }

@compute @workgroup_size(4, 4, 1)
fn main(@builtin(global_invocation_id) id: vec3u) {
  // id.z is the layer, which becomes the z axis of the finished volume.
  let p = (vec3f(id) + 0.5) / N;
  var pf = 0.0;
  var a = 0.5;
  var f = 4.0;
  for (var i = 0; i < 5; i++) { pf += a * perlin(p, f); a *= 0.5; f *= 2.0; }
  let per = clamp(pf * 1.1 + 0.5, 0.0, 1.0);
  let w0 = wfbm(p, 4.0);
  textureStore(out_tex, vec2i(id.xy), i32(id.z), vec4f(mix(per, w0, 0.3), wfbm(p, 8.0), wfbm(p, 16.0), wfbm(p, 32.0)));
}
