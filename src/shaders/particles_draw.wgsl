@group(1) @binding(0) var<uniform> S: PSim;
@group(1) @binding(1) var<storage, read> parts: array<Particle>;
@group(1) @binding(2) var t_depth: texture_depth_2d;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) a: vec4f,   // k, kind, seed, alpha
  @location(2) @interpolate(flat) b: vec4f,   // temp, size, age, unused
  @location(3) W: vec3f,
  @location(4) vd: f32,
  @location(5) @interpolate(flat) C: vec3f,   // centre, relative to camera
};

@vertex fn vs(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  var out: VOut;
  let p = parts[ii];
  if (p.age >= p.life) {
    out.clip = vec4f(2.0, 2.0, 2.0, 1.0);
    return out;
  }
  let corners = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let c = corners[vi];
  let k = p.age / p.life;
  let size = mix(p.size0, p.size1, 1.0 - (1.0 - k) * (1.0 - k));
  let center = S.anchor.xyz + p.pos;
  var W: vec3f;
  if (i32(p.kind + 0.5) == 1) {
    let sp = length(p.vel);
    let axis = p.vel / max(sp, 0.001);
    let side = normalize(cross(axis, normalize(center)));
    W = center + axis * c.y * (size + sp * 0.03) + side * c.x * size;
  } else if (i32(p.kind + 0.5) == 10) {
    // Thrown water: a clump of drops drawn out along its flight.
    let sp = length(p.vel);
    let axis = p.vel / max(sp, 0.001);
    let side = normalize(cross(axis, normalize(center)));
    W = center + axis * c.y * (size * 1.6 + sp * 0.045) + side * c.x * size;
  } else if (is_puff(i32(p.kind + 0.5))) {
    // A ball of gas, ray marched in the fragment shader: the quad only has to
    // cover its silhouette, so it is put on the near side and sized to the
    // cone of view that grazes the sphere.
    let D = length(center);
    let fwd = normalize(view_ray(vec2f(0.5)));
    if (D > size * 1.08) {
      let dq = max(D - size, G.misc.x * 4.0);
      let hs = dq * size / sqrt(D * D - size * size) * 1.12;
      W = center / D * dq + (S.cam_r.xyz * c.x + S.cam_u.xyz * c.y) * hs;
    } else {
      // The camera is inside it: cover the whole view.
      W = fwd * (G.misc.x * 4.0) + (S.cam_r.xyz * c.x + S.cam_u.xyz * c.y) * (G.misc.x * 40.0);
    }
  } else {
    let ang = p.seed * 6.283 + p.age * 0.35 * (p.seed - 0.5);
    let r = S.cam_r.xyz * cos(ang) + S.cam_u.xyz * sin(ang);
    let u = -S.cam_r.xyz * sin(ang) + S.cam_u.xyz * cos(ang);
    W = center + (r * c.x + u * c.y) * size;
  }
  out.clip = G.view_proj * vec4f(W, 1.0);
  out.uv = c;
  out.a = vec4f(k, p.kind, p.seed, p.alpha);
  out.b = vec4f(p.temp, size, p.age, 0.0);
  out.W = W;
  out.vd = out.clip.w;
  out.C = center;
  return out;
}

fn is_puff(kind: i32) -> bool { return kind == 0 || kind == 2 || kind == 3 || kind == 5 || kind == 7 || kind == 8; }
fn hgp(c: f32, g: f32) -> f32 { return (1.0 - g * g) / (4.0 * PI * pow(1.0 + g * g - 2.0 * g * c, 1.5)); }
// Smoke scatters strongly forward (bright rims against the sun), a little back.
fn smoke_phase(c: f32, k: f32) -> f32 { return mix(hgp(c, 0.7 * k), hgp(c, -0.3 * k), 0.4); }
// Glowing gas: colour and brightness by temperature (kelvin), a fit to the
// blackbody curve as the eye sees it, deep red through orange to yellow-white.
fn blackbody(tk: f32) -> vec3f {
  let x = clamp((tk - 700.0) / 2800.0, 0.0, 1.0);
  var c = mix(vec3f(1.0, 0.1, 0.01), vec3f(1.0, 0.38, 0.06), smoothstep(0.0, 0.25, x));
  c = mix(c, vec3f(1.0, 0.68, 0.3), smoothstep(0.25, 0.55, x));
  c = mix(c, vec3f(1.0, 0.92, 0.78), smoothstep(0.55, 1.0, x));
  return c * min(pow(tk / 1500.0, 4.5), 7.0);
}
fn puff_tau_d(kind: i32) -> f32 {
  if (kind == 0 || kind == 8) { return 3.2; }
  if (kind == 5) { return 1.4; }
  if (kind == 3) { return 1.2; }
  return 2.2;
}
fn rotate(v: vec3f, axis: vec3f, a: f32) -> vec3f {
  let c = cos(a);
  return v * c + cross(axis, v) * sin(a) + axis * dot(axis, v) * (1.0 - c);
}

// A ball of gas, rendered like the cloud layer: density carved by 3D noise,
// lit by marching toward the sun through the smoke volume, multiple
// scattering as dimmer softer octaves, and the hot core glowing by temperature.
fn puff(in: VOut, kind: i32, z: f32) -> vec4f {
  let k = in.a.x;
  let seed = in.a.z;
  let size = in.b.y;
  let temp = in.b.x;
  let age = in.b.z;
  let C = in.C;
  let rd = normalize(in.W);
  let hit = ray_sphere(-C, rd, size);
  if (hit.y <= 0.0) { discard; }
  let t0 = max(hit.x, 0.0);
  var t1 = hit.y;
  if (z > 0.0) {
    let fwd = normalize(view_ray(vec2f(0.5)));
    t1 = min(t1, G.misc.x / z / max(dot(rd, fwd), 0.05));
  }
  if (t1 <= t0 + 0.01) { discard; }
  let fade = smoothstep(0.0, 0.06, k) * (1.0 - smoothstep(0.55, 1.0, k));
  let tau = in.a.w * fade * puff_tau_d(kind);
  if (tau < 0.004) { discard; }
  // Peak extinction, for a parabolic profile with optical depth tau through the middle.
  let sig0 = 0.75 * tau / size;
  let hot = select(0.0, clamp(temp * 0.6, 0.0, 1.0), kind == 0);
  // Hot gas is smooth and swollen; it breaks up into ragged smoke as it cools.
  let erode = mix(0.95, 0.7, hot);
  // Each puff rolls slowly about its own axis, so the billows churn.
  let axis = normalize(vec3f(seed - 0.5, 0.31, fract(seed * 7.3) - 0.5));
  let spin = age * (0.18 + 0.35 * fract(seed * 3.7)) * select(1.0, -1.0, fract(seed * 11.0) > 0.5);
  let noff = vec3f(seed * 5.1, seed * 2.3, seed * 3.7) + vec3f(0.0, 0.0, age * 0.015);

  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let mid = rd * (t0 + t1) * 0.5;
  let vl = vol_light(mid);
  let lgt = light_at_nv(C);
  let mu = dot(lgt.upn, sun);
  let to_l = G.light_pos.xyz - C;
  // The crash's fire light only reaches the smoke hanging low and close to it;
  // the burning gas itself lights the plume through the volume's glow term.
  let fire_pt = point_light(C, normalize(to_l)) * 0.05 * exp(-length(to_l) / 40.0);
  // Sky from above (less of it deep in the plume) and the lit ground from below.
  // (Inside a plume much of the "sky" is more smoke, so the ambient is greyed.)
  let amb = mix(lgt.amb, vec3f(luma(lgt.amb)), 0.4) * 1.15 * (0.4 + 0.6 * exp(-vl.z * 0.7)) + lgt.sun * max(mu, 0.0) * exp(-vl.x) * 0.035;
  let glow = vl.y * vec3f(1.0, 0.42, 0.1) * 0.6 * G.sun_dir.w;

  var albedo = vec3f(0.85);
  if (kind == 0) {
    // Fresh soot is nearly black; as it thins out and cools it turns grey.
    albedo = mix(vec3f(0.17, 0.165, 0.16), vec3f(0.4, 0.4, 0.4), smoothstep(0.2, 1.0, k) * (1.0 - hot));
  } else if (kind == 8) {
    // Smoke carried on from the fire simulation: as dark as its soot there.
    albedo = mix(vec3f(0.2, 0.185, 0.17), vec3f(0.3, 0.29, 0.28), smoothstep(0.3, 1.0, k));
  } else if (kind == 2) {
    albedo = vec3f(0.5, 0.42, 0.32);
  } else if (kind == 7) {
    // Masonry dust: a grey-beige, darker than it looks in a thin haze.
    albedo = vec3f(0.54, 0.51, 0.47);
  } else {
    albedo = vec3f(0.92, 0.94, 0.96);
  }
  // Light-weighted ambient terms that do not change along the ray.
  let flat_l = amb + glow + fire_pt;

  let px = size / max(in.vd, 0.1) / G.misc.w;  // radius in pixels
  let steps = select(select(2, 4, px > 40.0), 6, px > 220.0);
  let dt = (t1 - t0) / f32(steps);
  let jit = 0.5 + (ign(in.clip.xy + seed * 113.0) - 0.5) * 0.6;
  var T = 1.0;
  var L = vec3f(0.0);
  for (var i = 0; i < steps; i++) {
    let t = t0 + dt * (f32(i) + jit);
    let s = rd * t - C;
    let r2 = dot(s, s) / (size * size);
    if (r2 >= 1.0) { continue; }
    let q = rotate(s / size, axis, spin);
    // Base shape from the Perlin-Worley channel, as the clouds do; then the
    // edges are eaten away by finer Worley noise into cauliflower billows.
    let n = textureSampleLevel(t_noise, s_repeat, q * 0.5 + noff, 0.0);
    let fbm = n.g * 0.625 + n.b * 0.25 + n.a * 0.125;
    let base = clamp(remap(n.r, fbm - 1.0, 1.0, 0.0, 1.0), 0.0, 1.0);
    // Noise only eats inward, so the outline breaks up instead of being
    // clipped by the bounding sphere.
    let rr = sqrt(r2) + (1.0 - base) * 0.75 * erode;
    var d = clamp(1.0 - rr * rr, 0.0, 1.0) * (0.6 + 0.6 * base);
    if (d <= 0.002) { continue; }
    if (steps > 2) {
      let e = textureSampleLevel(t_noise, s_repeat, q * 1.35 + noff * 1.7 + vec3f(0.0, age * 0.03, 0.0), 0.0);
      let ef = e.g * 0.625 + e.b * 0.25 + e.a * 0.125;
      d = clamp(remap(d, ef * 0.5 * erode, 1.0, 0.0, 1.0), 0.0, 1.0);
    } else {
      d = clamp(remap(d, fbm * 0.3 * erode, 1.0, 0.0, 1.0), 0.0, 1.0);
    }
    // Real smoke edges are abrupt; spray and mist thin out softly.
    d = select(smoothstep(0.0, 0.25, d), d * d * (3.0 - 2.0 * d), kind == 5);
    if (d <= 0.002) { continue; }
    var sig = sig0 * d * 2.4;
    // Toward the sun: the rest of the smoke, plus this puff's own body.
    let b = dot(s, sun);
    let lout = -b + sqrt(max(b * b - dot(s, s) + size * size, 0.0));
    let od = vl.x + lout * sig0 * 0.5;
    var ms = 0.0;
    var aa = 1.0;
    var bb = 1.0;
    var cc = 1.0;
    for (var o = 0; o < 3; o++) {
      ms += aa * exp(-od * bb) * smoke_phase(cs, cc);
      aa *= 0.5;
      bb *= 0.35;
      cc *= 0.5;
    }
    // Beer-powder: the sunward skin of a dense puff looks darker than its body.
    let powder = mix(1.0, 1.0 - exp(-sig * size * 1.2), 0.5 * (1.0 - max(cs, 0.0)));
    var Ls = albedo * (0.75 + 0.5 * n.b) * (lgt.sun * ms * 3.4 * powder + flat_l);
    if (kind == 0 && temp > 0.15) {
      // Burning gas: hottest in the dense core, flickering with the turbulence.
      let heat = temp * (1.1 - 0.65 * sqrt(r2)) * (0.35 + 0.95 * n.g) * d;
      // A fuel fireball burns at 1300-2000 K: yellow-orange at its hottest, never white.
      Ls += blackbody(500.0 + 1700.0 * (1.0 - exp(-heat * 0.45))) * 0.16 * G.sun_dir.w;
      // Luminous flame is thinner than the soot it turns into.
      sig *= mix(1.0, 0.55, smoothstep(1.0, 2.0, heat));
    }
    let st = exp(-sig * dt);
    L += T * Ls * (1.0 - st);
    T *= st;
    if (T < 0.01) { break; }
  }
  let a = 1.0 - T;
  if (a < 0.002) { discard; }
  return vec4f(L, a);
}

@fragment fn fs(in: VOut) -> @location(0) vec4f {
  let k = in.a.x;
  let kind = i32(in.a.y + 0.5);
  let seed = in.a.z;
  let z = textureLoad(t_depth, vec2i(in.clip.xy), 0);
  if (is_puff(kind)) { return puff(in, kind, z); }
  let r = length(in.uv);
  if (r > 1.0) { discard; }
  var soft = 1.0;
  if (z > 0.0) {
    let ds = G.misc.x / z;
    if (ds < in.vd && kind != 4) { discard; }
    soft = clamp((ds - in.vd) / (in.b.y * 0.6 + 0.05), 0.0, 1.0);
  }
  let near = smoothstep(0.3, 2.5, in.vd);
  if (kind == 1) {
    // Spark: additive streak that cools from white to red.
    let heat = (1.0 - k);
    let f = (1.0 - abs(in.uv.x)) * (1.0 - in.uv.y * in.uv.y);
    return vec4f(mix(vec3f(1.0, 0.25, 0.03), vec3f(1.0, 0.85, 0.55), heat * heat) * f * heat * 60.0 * G.sun_dir.w * 0.1 * in.a.w, 0.0);
  }
  if (kind == 4) {
    let f = pow(1.0 - r, 4.0) * (1.0 - k) * (1.0 - k);
    return vec4f(vec3f(1.0, 0.78, 0.5) * f * in.a.w * G.sun_dir.w, 0.0) * soft;
  }
  if (kind == 9) {
    // Glass shard: a sliver that tumbles, catching the sun and the sky in
    // flashes, and lies glittering in the street once it has landed.
    if (abs(in.uv.x) * 1.6 + abs(in.uv.y) > 1.0) { discard; }
    let ll = light_at(in.W);
    let fadeo = 1.0 - smoothstep(0.75, 1.0, k);
    let ph = seed * 61.0 + in.b.z * (5.0 + 14.0 * fract(seed * 13.7));
    let flash = pow(max(sin(ph), 0.0), 18.0);
    let sky = sky_lut(normalize(ll.upn + S.cam_r.xyz * (fract(seed * 5.1) - 0.5)));
    let c = vec3f(0.5, 0.58, 0.6) * ll.amb * 0.6 + sky * 0.25 + ll.sun * flash * 0.6;
    return vec4f(c * fadeo * in.a.w, 0.45 * fadeo * in.a.w);
  }
  if (kind == 10) {
    // Thrown water: a broken-up streak of drops, white where the sun is on
    // it and glowing when seen toward the sun (drops scatter forward).
    let nz = textureSampleLevel(t_noise, s_repeat, vec3f(in.uv * vec2f(0.35, 0.12) + seed * 7.0, seed + in.b.z * 0.2), 0.0);
    let f = (1.0 - in.uv.x * in.uv.x) * (1.0 - r * r) * smoothstep(0.25, 0.7, nz.r * 0.7 + nz.g * 0.5);
    let ll = light_at(in.W);
    let cs = max(dot(normalize(in.W), G.sun_dir.xyz), 0.0);
    let fwd = 0.35 + 2.6 * pow(cs, 6.0);
    let c = vec3f(0.86, 0.9, 0.93) * (ll.sun * fwd / PI + ll.amb * 1.1);
    let a = clamp(f * in.a.w * (1.0 - smoothstep(0.6, 1.0, k)) * smoothstep(0.0, 0.03, k) * soft * near, 0.0, 1.0);
    if (a < 0.003) { discard; }
    return vec4f(c * a, a);
  }
  if (kind == 6) {
    // Leaf: a small opaque fleck, lit like foliage.
    if (abs(in.uv.x) + abs(in.uv.y) * 0.6 > 1.0) { discard; }
    let ll = light_at(in.W);
    let fadeo = 1.0 - smoothstep(0.8, 1.0, k);
    let lc = mix(vec3f(0.07, 0.13, 0.03), vec3f(0.16, 0.12, 0.05), step(0.7, seed)) * (0.6 + 0.8 * fract(seed * 7.3));
    return vec4f(lc * (ll.sun * 0.5 / PI + ll.amb) * fadeo, fadeo);
  }
  discard;
  return vec4f(0.0);
}
