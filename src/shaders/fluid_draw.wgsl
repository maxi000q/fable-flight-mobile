// Draws the fire simulation: a ray march through its grid, blended over the
// lit scene. Soot is lit by the sun (shadowed through the smoke by the
// lighting grid), the sky and the fire itself; hot soot and burning fuel
// glow at their blackbody colour.
@group(1) @binding(0) var<uniform> U: FU;
@group(1) @binding(1) var t_fscal: texture_3d<f32>;
@group(1) @binding(2) var t_flight: texture_3d<f32>;
@group(1) @binding(3) var t_fdepth: texture_depth_2d;
@group(1) @binding(4) var t_fvel: texture_3d<f32>;
// For the upsampling pass: the half-resolution result and the scene depth each texel marched to.
@group(1) @binding(5) var t_fhalf: texture_2d<f32>;
@group(1) @binding(6) var t_fhz: texture_2d<f32>;

// The march runs at half resolution: each texel stands for one pixel of its
// 2x2 block, a different one each frame (temporal AA fills in the rest).
fn sub_px() -> vec2i {
  let f = u32(G.time.w);
  return vec2i(i32(f & 1u), i32(((f >> 1u) ^ f) & 1u));
}
struct HalfOut { @location(0) c: vec4f, @location(1) z: f32 };

fn hg(c: f32, g: f32) -> f32 { return (1.0 - g * g) / (4.0 * PI * pow(1.0 + g * g - 2.0 * g * c, 1.5)); }

// Noise that streams with the local flow: two copies carried along for a
// short while each and cross-faded, so it moves with the gas but never
// stretches out.
fn flow_noise(p: vec3f, vel: vec3f, tm: f32, f: f32) -> vec4f {
  let per = 1.3;
  let k = tm / per;
  let ph0 = fract(k);
  let ph1 = fract(k + 0.5);
  let w0 = 1.0 - abs(ph0 * 2.0 - 1.0);
  let a = textureSampleLevel(t_noise, s_repeat, (p - vel * (ph0 * per)) * f + vec3f(0.0, 0.0, floor(k) * 0.37), 0.0);
  let b = textureSampleLevel(t_noise, s_repeat, (p - vel * (ph1 * per)) * f + vec3f(0.31, 0.17, floor(k + 0.5) * 0.37 + 0.5), 0.0);
  return mix(b, a, w0);
}

@fragment fn fs(in: FsIn) -> HalfOut {
  let px = vec2i(in.pos.xy) * 2 + sub_px();
  let full = vec2i(textureDimensions(t_fdepth));
  let pxc = min(px, full - 1);
  let z = textureLoad(t_fdepth, pxc, 0);
  var out = HalfOut(vec4f(0.0), z);
  if (U.org.w < 0.5) { return out; }
  let h = U.a.y;
  let rd = normalize(view_ray((vec2f(pxc) + 0.5) / vec2f(full)));
  let e = U.ax_e.xyz;
  let n = U.ax_n.xyz;
  let u = U.ax_u.xyz;
  let ro = vec3f(dot(-U.org.xyz, e), dot(-U.org.xyz, n), dot(-U.org.xyz, u));
  let rl = vec3f(dot(rd, e), dot(rd, n), dot(rd, u));
  let size = vec3f(FN) * h;
  let inv = 1.0 / select(rl, vec3f(1e-6), abs(rl) < vec3f(1e-6));
  let ta = (vec3f(0.0) - ro) * inv;
  let tb = (size - ro) * inv;
  let t0v = min(ta, tb);
  let t1v = max(ta, tb);
  var t0 = max(max(max(t0v.x, t0v.y), t0v.z), 0.0);
  var t1 = min(min(t1v.x, t1v.y), t1v.z);
  if (z > 0.0) {
    let fwd = normalize(view_ray(vec2f(0.5)));
    t1 = min(t1, G.misc.x / z / max(dot(rd, fwd), 0.05));
  }
  if (t1 <= t0) { return out; }

  // Light arriving at the domain (one value for the whole ray).
  let mid = U.org.xyz + (e * size.x + n * size.y) * 0.5 + u * (U.ax_e.w + 10.0);
  let lgt = light_at_nv(mid);
  let cs = dot(rd, G.sun_dir.xyz);
  let phase = mix(hg(cs, 0.55), hg(cs, -0.25), 0.3);
  let sky = mix(lgt.amb, vec3f(luma(lgt.amb)), 0.3) * 1.6 * U.ax_u.w;
  let bounce = lgt.sun * max(dot(lgt.upn, G.sun_dir.xyz), 0.0) * 0.06;
  let flame = U.ext.w * G.sun_dir.w;
  let fire_col = fl_bb_color(1500.0);

  let jit = fract(ign(vec2f(px)) + G.time.w * 0.618034);
  let st_full = h * 0.9;
  var t = t0 + st_full * jit;
  // Thin gas is crossed in longer strides than dense gas.
  var stride = st_full;
  var T = 1.0;
  var L = vec3f(0.0);
  let tm = U.a.z;
  for (var i = 0; i < 260; i++) {
    if (t >= t1 || T < 0.01) { break; }
    let pl = ro + rl * t;
    let q = pl / size;
    let lg = textureSampleLevel(t_flight, s_clamp, q, 0.0);
    if (lg.w < 0.5) {
      // Nothing here or next to it: stride on.
      t += st_full * 3.0;
      continue;
    }
    let dt = min(stride, t1 - t);
    // Detail the grid is too coarse to carry: look it up through a warp
    // that folds its edges, then let finer noise eat into the thin gas at
    // the edges of the billows (cauliflower), as the clouds do.
    var s: vec4f;
    var sig: f32;
    var emitter: f32;
    // Near the top and the open sides the smoke thins out: beyond them it is particles.
    let edge = min(min(pl.x, size.x - pl.x), min(pl.y, size.y - pl.y));
    // (over more metres in a coarse grid, or a dense plume ends in a wall)
    let fade_s = max(8.0, 8.0 + (h - 1.2) * 12.0);
    let fade_t = max(12.0, 12.0 + (h - 1.2) * 16.0);
    let topfade = smoothstep(size.z, size.z - fade_t, pl.z) * smoothstep(0.0, fade_s, edge);
    if (U.ax_n.w > 0.0) {
      // Carried with the flow, but no faster than a few m/s: where the gas
      // moves fast its shear would smear the noise into streaks.
      let v0 = textureSampleLevel(t_fvel, s_clamp, q, 0.0).xyz;
      let vel = v0 * min(1.0, 3.0 / max(length(v0), 1e-3));
      // The largest octave changes slowly enough to just drift.
      let n0 = textureSampleLevel(t_noise, s_repeat, pl * 0.016 + vec3f(0.0, 0.0, -tm * 0.03), 0.0);
      let n1 = flow_noise(pl, vel, tm, 0.04);
      let warp = ((n0.gba - vec3f(0.45)) * 2.0 + (n1.gba - vec3f(0.45))) * (2.6 * h * U.ax_n.w);
      let pw = pl + warp;
      s = max(textureSampleLevel(t_fscal, s_clamp, pw / size, 0.0), vec4f(0.0));
      sig = fl_sigma(s, U.ext);
      if (sig > 1e-4 || s.y > 0.02) {
        let n2 = flow_noise(pw, vel, tm, 0.11);
        let ero = (n2.g * 0.55 + n2.b * 0.3 + n2.a * 0.15) * 0.62 * U.ax_n.w;
        let dn = 1.0 - exp(-sig * 2.0 * h);
        let de = clamp((dn - ero) / max(1.0 - ero, 1e-3), 0.0, 1.0);
        let m = de / max(dn, 1e-5);
        s = vec4f(s.x * m, s.y * (0.8 + 0.4 * n2.r), s.z * m, s.w * m);
        sig *= m;
      }
    } else {
      s = max(textureSampleLevel(t_fscal, s_clamp, q, 0.0), vec4f(0.0));
      sig = fl_sigma(s, U.ext);
    }
    s = vec4f(s.x * topfade, s.y, s.z, s.w * topfade);
    sig = fl_sigma(s, U.ext);
    emitter = fl_emitter(s, U.ext, U.k2.z);
    if (sig > 1e-4) {
      let soot_s = s.x * U.ext.x;
      let dust_s = s.w * U.ext.y;
      let fuel_s = s.z * U.ext.z;
      // Fresh soot is nearly black; dust is pale earth; fuel mist is light.
      let alb = (soot_s * vec3f(0.23, 0.21, 0.19) + dust_s * vec3f(0.55, 0.47, 0.38) + fuel_s * vec3f(0.32, 0.3, 0.27)) / max(sig, 1e-6);
      let sunl = lgt.sun * (exp(-lg.x) * phase + 0.08 * exp(-lg.x * 0.3) / (4.0 * PI));
      // (the sky's light is dimmed by the smoke straight overhead; under a
      // plume hundreds of metres wide much of it still comes in from the
      // sides, so a coarse grid's tall columns shade less)
      let amb = sky * exp(-lg.z * 0.3 / max(1.0, h / 1.2)) + bounce;
      let fire = fire_col * lg.y * flame * 0.5;
      let Ls = alb * (sunl + amb + fire);
      let tk = fl_kelvin(s.y);
      let Le = fl_bb_color(tk) * (fl_radiance(tk) * flame) * emitter / max(sig, 1e-6);
      let a = exp(-sig * dt);
      L += T * (Ls + Le) * (1.0 - a);
      T *= a;
    } else if (emitter > 0.0) {
      let tk = fl_kelvin(s.y);
      L += T * fl_bb_color(tk) * fl_radiance(tk) * flame * emitter * dt;
    }
    t += dt;
    stride = st_full * mix(2.2, 1.0, clamp(sig * h * 4.0 + emitter * h * 8.0, 0.0, 1.0));
  }
  out.c = vec4f(L, 1.0 - T);
  return out;
}

// Upsample to full resolution, blended over the scene. Each pixel takes the
// half-resolution texels around it, weighted by distance and by how close the
// scene depth each one marched to is to its own (so the fire does not bleed
// over the edges of things in front of it, nor leave halos around them).
@fragment fn fs_up(in: FsIn) -> @location(0) vec4f {
  if (U.org.w < 0.5) { discard; }
  let p = vec2i(in.pos.xy);
  let sub = sub_px();
  let hd = vec2i(textureDimensions(t_fhalf));
  let z = textureLoad(t_fdepth, p, 0);
  let dz = select(1e6, G.misc.x / z, z > 0.0);
  let hp = vec2f(p - sub) * 0.5;
  let h0 = vec2i(floor(hp));
  var acc = vec4f(0.0);
  var wsum = 0.0;
  var best = vec4f(0.0);
  var bestd = 1e30;
  for (var k = 0; k < 4; k++) {
    let hq = clamp(h0 + vec2i(k & 1, k >> 1), vec2i(0), hd - 1);
    let sp = vec2f(hq * 2 + sub);
    let ws = max(1.0 - abs(sp - vec2f(p)) * 0.5, vec2f(0.0));
    let c = textureLoad(t_fhalf, hq, 0);
    let zq = textureLoad(t_fhz, hq, 0).r;
    let dq = select(1e6, G.misc.x / zq, zq > 0.0);
    let rel = abs(dq - dz) / min(dq, dz);
    let wd = 1.0 / (1.0 + rel * rel * 400.0);
    let w = ws.x * ws.y * wd + 1e-5;
    acc += c * w;
    wsum += w;
    if (rel < bestd) { bestd = rel; best = c; }
  }
  var c = acc / wsum;
  // Nothing around matches this pixel's depth well: take the closest match.
  if (wsum < 0.05) { c = best; }
  if (c.a < 0.001 && dot(c.rgb, vec3f(1.0)) < 1e-5) { discard; }
  return c;
}
