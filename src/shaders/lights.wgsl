// Airfield and aircraft lights: each a small glowing sprite, added over the
// lit scene and tested against its depth (see airfield.rs `Light`).
struct BTile { offset: vec4f, ax: vec4f, ay: vec4f };
@group(1) @binding(0) var<uniform> T: BTile;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) col: vec3f,
};

const CORNERS = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));

@vertex fn vs(@builtin(vertex_index) vi: u32, @location(0) pos: vec3f, @location(1) inten: f32, @location(2) dir: vec3f, @location(3) spread: f32, @location(4) colk: vec4f, @location(5) fx: vec4<u32>) -> VOut {
  let P = pos + T.offset.xyz;
  let d = max(length(P), 0.1);
  let V = -P / d;
  let upn = normalize(P - G.planet.xyz);
  let kind = u32(round(colk.a * 255.0));
  var col = pow(colk.rgb, vec3f(2.2));
  var I = inten;
  if (kind == 1u) {
    // PAPI: white above its angle, red below, in a beam 10 degrees either side.
    let elev = asin(clamp(dot(V, upn), -1.0, 1.0));
    let w = smoothstep(spread - 0.0012, spread + 0.0012, elev);
    col = mix(vec3f(1.0, 0.02, 0.01), vec3f(1.0, 0.92, 0.82), w);
    let hv = normalize(V - upn * dot(V, upn));
    let hd = normalize(dir - upn * dot(dir, upn));
    I *= smoothstep(0.93, 0.985, dot(hv, hd)) * step(0.0, elev + 0.01);
  } else if (dot(dir, dir) > 0.5) {
    I *= smoothstep(spread, min(spread + 0.3, 0.999), dot(dir, V));
  }
  if (fx.x > 0u) {
    let ph = fract(G.time.x / (f32(fx.x) * 0.1) + f32(fx.y) / 255.0);
    I *= select(0.0, 1.0, ph < f32(fx.z) / 255.0);
  }
  // By day only the lights that are meant to be seen in daylight show, and dimly.
  let night = night_at(upn);
  I *= mix(select(0.0, 0.3, fx.w > 0u), 1.0, night);
  // Through the haze.
  I *= exp(-d * 2.5e-5 * max(G.weather.w, 0.2));
  // A light is a point: as it gets further off its sprite stops shrinking
  // below a pixel or so, and its brightness is spread over that.
  // (Kind 3, the flown aircraft's own lights, are seen close to: a lens a
  // few centimetres across, and no brighter than from 40 m, so the glare
  // round them stays a glint rather than swallowing the wing.)
  let own = kind == 3u;
  let theta = max(select(0.2, 0.035, own) / d, 1.3 * G.misc.w);
  var E = I * 0.07 / (d * d) * pow(max(d / 300.0, 1.0), 0.9);
  if (own) { E = I * 0.07 / max(d * d, 1600.0) * pow(max(d / 300.0, 1.0), 0.9); }
  let L = E / (PI * theta * theta);
  if (I <= 1e-4 || L < 1e-4) {
    return VOut(vec4f(2.0, 2.0, 2.0, 1.0), vec2f(0.0), vec3f(0.0));
  }
  let side = normalize(cross(V, select(upn, G.east.xyz, abs(dot(V, upn)) > 0.99)));
  let up = cross(side, V);
  let c = CORNERS[vi];
  let R = 2.5 * theta * d;
  let W = P + (side * c.x + up * c.y) * R;
  var clip = G.view_proj * vec4f(W, 1.0);
  // Depth: a little in front of the light, so the ground it stands on does not hide it.
  let pd = G.view_proj * vec4f(P + V * min(R + select(0.4, 0.03, own), d * 0.5), 1.0);
  clip.z = pd.z / pd.w * clip.w;
  return VOut(clip, c * 2.5, col * L);
}

@fragment fn fs(in: VOut) -> @location(0) vec4f {
  // A tight hot core with a faint, short halo (the lens of the eye/camera
  // scatters a little) and four faint diffraction spikes, instead of a big
  // soft ball.
  let r2 = dot(in.uv, in.uv);
  let a = abs(in.uv);
  let spikes = exp(-a.x * 18.0) * exp(-a.y * 1.6) + exp(-a.y * 18.0) * exp(-a.x * 1.6);
  let p = exp(-r2 * 2.0) + 0.012 * exp(-r2 * 0.4) + 0.03 * spikes * exp(-r2 * 0.3);
  return vec4f(in.col * p, 0.0);
}
