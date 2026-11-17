// The safe-mode composite (StartupProfile::comp_lite). The full module — the
// water with its breakers, refraction and reflections, the screen-space
// reflections, the atmosphere marches — is the heaviest shader of the set, and
// the one an Adreno 619 reliably loses its GPU device to while compiling
// (every 1.0.7-1.0.14 start died at the composite pipeline). A start-up that
// was lost there comes back on this instead: the same resolved G-buffer, the
// same HDR output, shaded with a flat sea, a simple sky and distance haze —
// a few dozen instructions the driver cannot choke on. Bindings 5-8 (the
// per-sample copies) it never declares, so its bind group never holds them.

@group(1) @binding(0) var t_color: texture_2d<f32>;
@group(1) @binding(1) var t_aux: texture_2d<f32>;
@group(1) @binding(2) var t_depth: texture_depth_2d;
@group(1) @binding(3) var t_ao: texture_2d<f32>;
@group(1) @binding(4) var t_amb: texture_2d<f32>;

// A sky good enough to fly under: a day gradient, warm scattering low around
// the sun, the sun itself, and a deep blue night. No marches, no stars.
fn lite_sky(rd: vec3f) -> vec3f {
  let sun = normalize(G.sun_dir.xyz);
  let up = normalize(G.up.xyz);
  let el = dot(rd, up);
  let day = smoothstep(-0.12, 0.10, dot(up, sun));
  let cs = dot(rd, sun);
  var col = mix(vec3f(0.60, 0.70, 0.84), vec3f(0.10, 0.24, 0.58), pow(clamp(el, 0.0, 1.0), 0.45));
  // Forward scattering low over the horizon, strongest at dawn and dusk.
  col += vec3f(0.95, 0.52, 0.22) * pow(max(cs, 0.0), 5.0) * (0.12 + day * (1.0 - day) * 1.6);
  // The sun: a disc and a tight glow, both scaled by its illuminance.
  col += vec3f(1.0, 0.96, 0.90) * G.sun_dir.w * (600.0 * smoothstep(0.999978, 0.999990, cs) + 2.0 * pow(max(cs, 0.0), 64.0));
  col = mix(vec3f(0.004, 0.007, 0.016), col, day);
  // Toward the planet's far edge the sky falls into its own haze.
  col = mix(col, vec3f(0.50, 0.58, 0.70), clamp(-el * 3.0, 0.0, 1.0) * day);
  return col;
}

// The flat sea: sun-lit deep water under a Fresnel of sky, a broad slow swell
// tilted in from the wave texture, and the sun's glitter along the mirror
// direction. No breakers, no sea bed, no refraction — those are the full
// composite's, along with its compile cost.
fn lite_water(P: vec3f, rd: vec3f) -> vec3f {
  let sun = normalize(G.sun_dir.xyz);
  let up = normalize(G.up.xyz);
  // The swell must not slide with the camera: sample the wave texture at the
  // point's planet-stable position, on the horizontal plane through it.
  let wp = P - G.planet.xyz;
  let ref_dir = select(vec3f(0.0, 0.0, 1.0), vec3f(1.0, 0.0, 0.0), abs(up.z) > 0.9);
  let ex = normalize(cross(up, ref_dir));
  let ey = cross(up, ex);
  let q = vec2f(dot(wp, ex), dot(wp, ey));
  let w1 = textureSampleLevel(t_wave, s_repeat, q * 0.00012 + vec2f(G.time.x * 0.003, 0.0), 5.0);
  let w2 = textureSampleLevel(t_wave, s_repeat, q * 0.00047 + vec2f(0.0, G.time.x * 0.004), 4.0);
  let tilt = ex * ((w1.r - 0.5) * 0.22 + (w2.r - 0.5) * 0.10) + ey * ((w1.g - 0.5) * 0.22 + (w2.g - 0.5) * 0.10);
  let n = normalize(up + tilt);
  // Sun-lit deep water (the tone sky.wgsl gives the bare planet's ocean).
  let mu = clamp(dot(up, sun), -1.0, 1.0);
  let deep = vec3f(0.012, 0.035, 0.070) * (max(mu, 0.0) * 0.9 + 0.05) * G.sun_dir.w / PI;
  let r = reflect(rd, n);
  let f = 0.04 + 0.96 * pow(1.0 - clamp(dot(-rd, n), 0.0, 1.0), 5.0);
  var col = mix(deep, lite_sky(reflect(rd, up)), clamp(f, 0.0, 1.0));
  col += vec3f(1.0, 0.93, 0.80) * G.sun_dir.w * 0.30 * pow(max(dot(r, sun), 0.0), 240.0);
  return col;
}

@fragment fn fs(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  let c0 = textureLoad(t_color, px, 0);
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  if (z == 0.0 || c0.a < 0.004) {
    return vec4f(clamp(lite_sky(rd), vec3f(0.0), vec3f(3000.0)), 1.0);
  }
  let P = ray / z;
  let dist = length(P);
  // The G-buffer colour is the fully lit surface; alpha is how much of the
  // pixel the geometry covers (the anti-aliased edge).
  var col = c0.rgb / c0.a;
  let aux = textureLoad(t_aux, px, 0) / c0.a;
  if (aux.a <= 0.004) {
    // Ambient occlusion takes away from the sky and bounce light only.
    let share = clamp(textureLoad(t_amb, px, 0).r / c0.a, 0.0, 1.0);
    col *= 1.0 - share * (1.0 - textureLoad(t_ao, px, 0).r);
  }
  // Water is marked in the aux alpha; glass keeps the sky the building shader
  // already reflected into it (no screen-space reflections here).
  if (aux.a > 0.996) {
    col = lite_water(P, rd);
  }
  // Distance haze toward the sky, and the anti-aliased edge over it.
  col = mix(col, lite_sky(rd), 1.0 - exp(-dist / 25000.0));
  if (c0.a < 0.996) { col = mix(lite_sky(rd), col, c0.a); }
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), 1.0);
}
