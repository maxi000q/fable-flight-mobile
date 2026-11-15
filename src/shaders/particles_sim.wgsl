// GPU particle simulation for explosions, smoke, sparks, dust and spray.
@group(1) @binding(0) var<uniform> S: PSim;
@group(1) @binding(1) var<storage, read_write> parts: array<Particle>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= arrayLength(&parts)) { return; }
  var p = parts[i];
  if (p.age >= p.life) { return; }
  let dt = S.upv.w;
  let up = S.upv.xyz;
  p.pos += S.shift.xyz;
  p.age += dt;
  let k = p.age / p.life;
  let kind = i32(p.kind + 0.5);
  var acc = -up * 9.81;
  if (kind == 0 || kind == 2 || kind == 5 || kind == 7 || kind == 8) {
    // Hot gas: buoyant, gently turbulent (so a fireball stays one rolling
    // mass), slowly taken by the wind.
    acc = up * (p.buoy * (0.15 + p.temp));
    let n = textureSampleLevel(t_noise, s_repeat, p.pos * 0.011 + vec3f(p.seed * 3.0, S.cam_r.w * 0.04, p.seed), 0.0).gba - 0.5;
    acc += n * (7.0 + 13.0 * p.temp);
    p.vel += (S.wind.xyz - p.vel) * min(dt * 0.35 * k, 1.0);
    p.temp *= exp(-dt * 1.25);
  } else if (kind == 4) {
    acc = vec3f(0.0);
  } else if (kind == 6) {
    // Leaves and splinters: light, fluttering.
    let n = textureSampleLevel(t_noise, s_repeat, p.pos * 0.3 + vec3f(p.seed * 9.0, S.cam_r.w * 0.5, 0.0), 0.0).gba - 0.5;
    acc = -up * 4.0 + n * 14.0 + (S.wind.xyz - p.vel) * 0.4;
  }
  p.vel += acc * dt;
  p.vel *= exp(-p.drag * dt);
  p.pos += p.vel * dt;
  let hgt = dot(p.pos, up) - S.anchor.w;
  if (hgt < 0.0 && kind != 4) {
    p.pos -= up * hgt;
    let vn = dot(p.vel, up);
    if (vn < 0.0) {
      if (kind == 1) { p.vel = (p.vel - up * vn * 1.4) * 0.55; }
      else if (kind == 9) { p.vel = select(vec3f(0.0), (p.vel - up * vn * 1.3) * 0.3, vn < -2.0); }
      else if (kind == 3 || kind == 10) { p.age = p.life; }
      else if (kind == 6) { p.vel = vec3f(0.0); }
      else { p.vel -= up * vn; }
    }
  }
  parts[i] = p;
}
