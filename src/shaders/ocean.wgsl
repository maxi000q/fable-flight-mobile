// Wind-driven ocean (ocean.rs). Three tiling cascades (about 250 m, 40 m and
// 6 m across), each a 256 x 256 inverse FFT of a JONSWAP wind sea with
// Donelan-Banner directional spread, plus a long swell, rebuilt every frame.
//
// `rows` evaluates the spectrum at this frame's time and transforms each row;
// `cols` transforms the columns and writes the result into one 6-layer
// texture: layer c = (Dx, Dy, height, Dxy), layer c + 3 = (dh/dx, dh/dy, Dxx, Dyy)
// for cascade c, the D terms sideways displacement (already scaled by the
// choppiness) and its derivatives. `mip` builds the mip chain.
const N = 256u;
const GRAV = 9.81;
const PI = 3.14159265;

struct OU {
  // per cascade: x = tile size (m), y = k from, z = k up to (rad/m), w = choppiness
  casc: array<vec4f, 3>,
  // wind sea: x = wind (m/s), y = peak angular frequency, z = alpha, w = gamma
  sea: vec4f,
  // swell: x = peak angular frequency, y = alpha, z = gamma, w = spread
  swell: vec4f,
  // xy direction the wind blows toward, zw the swell's (east, north)
  dir: vec4f,
  // x = time (s, wrapped at the repeat period), y = frequency quantum (2 pi /
  // period), z = how much of the short-wave tail the wind raises (1 = all of it)
  t: vec4f,
};
@group(0) @binding(0) var<uniform> U: OU;
// Complex unit Gaussian per texel and cascade (ocean.rs `noise`, shared with the CPU copy).
@group(0) @binding(1) var<storage, read> noise: array<vec2f>;
// Rows transformed: [cascade][field][y][x].
@group(0) @binding(2) var<storage, read_write> spec: array<vec2f>;
@group(0) @binding(3) var out_t: texture_storage_2d_array<rgba16float, write>;
// Mip chain: the level above, and the one being written.
@group(0) @binding(5) var mip_src: texture_2d_array<f32>;
@group(0) @binding(6) var mip_dst: texture_storage_2d_array<rgba16float, write>;

var<workgroup> buf: array<vec2f, 1024>;

fn cmul(a: vec2f, b: vec2f) -> vec2f { return vec2f(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x); }
fn ci(a: vec2f) -> vec2f { return vec2f(-a.y, a.x); }
fn cconj(a: vec2f) -> vec2f { return vec2f(a.x, -a.y); }

fn jonswap(w: f32, wp: f32, alpha: f32, gamma: f32) -> f32 {
  let sig = select(0.09, 0.07, w <= wp);
  let d = (w - wp) / (sig * wp);
  let r = exp(-0.5 * d * d);
  let x = wp / w;
  let x2 = x * x;
  return alpha * GRAV * GRAV / pow(w, 5.0) * exp(-1.25 * x2 * x2) * pow(gamma, r);
}
// Donelan-Banner sech^2 spread (integrates to 1 over the circle).
fn spread(th: f32, beta: f32) -> f32 {
  let b = max(beta, 0.05);
  let c = cosh(min(b * abs(th), 40.0));
  return b / (2.0 * tanh(b * PI) * c * c);
}
fn db_beta(wr: f32) -> f32 {
  if (wr < 0.95) { return 2.61 * pow(wr, 1.3); }
  if (wr < 1.6) { return 2.28 * pow(wr, -1.3); }
  return pow(10.0, -0.4 + 0.8393 * exp(-0.567 * log(wr * wr)));
}
fn angle(a: vec2f, b: vec2f) -> f32 { return atan2(a.x * b.y - a.y * b.x, dot(a, b)); }

// Wave energy per unit kx, ky (m^4).
fn energy(kv: vec2f, k: f32) -> f32 {
  let w = sqrt(GRAV * k);
  let kh = kv / k;
  var e = 0.0;
  if (U.sea.z > 0.0) {
    e += jonswap(w, U.sea.y, U.sea.z, U.sea.w) * spread(angle(U.dir.xy, kh), db_beta(w / U.sea.y));
  }
  if (U.swell.y > 0.0) {
    e += jonswap(w, U.swell.x, U.swell.y, U.swell.z) * spread(angle(U.dir.zw, kh), U.swell.w);
  }
  // Light air leaves the short wavelets (under a metre or so) weak.
  let tail = mix(1.0, U.t.z, smoothstep(2.0, 20.0, k));
  return e * tail * GRAV / (2.0 * w) / k;
}

struct Z4 { a: vec2f, b: vec2f, c: vec2f, d: vec2f };

fn spectrum(c: u32, x: u32, y: u32) -> Z4 {
  var o = Z4(vec2f(0.0), vec2f(0.0), vec2f(0.0), vec2f(0.0));
  // (the Nyquist row and column have no mirror image: left out)
  if (x == 0u || y == 0u) { return o; }
  let L = U.casc[c].x;
  let dk = 2.0 * PI / L;
  let kv = vec2f(f32(i32(x) - 128), f32(i32(y) - 128)) * dk;
  let k = length(kv);
  if (k <= 0.0 || k < U.casc[c].y || k >= U.casc[c].z) { return o; }
  let a = dk * sqrt(0.5);
  let base = c * N * N;
  let h0p = noise[base + y * N + x] * (sqrt(energy(kv, k)) * a);
  let h0m = noise[base + ((N - y) & 255u) * N + ((N - x) & 255u)] * (sqrt(energy(-kv, k)) * a);
  // Deep water with surface tension; frequencies quantised so the sea repeats exactly.
  let w = floor(sqrt(GRAV * k * (1.0 + k * k / 136900.0)) / U.t.y) * U.t.y;
  let ph = w * U.t.x;
  let e = vec2f(cos(ph), sin(ph));
  let h = cmul(h0p, e) + cmul(cconj(h0m), cconj(e));
  let ih = ci(h);
  let dx = ih * (kv.x / k);
  let dy = ih * (kv.y / k);
  let dxx = h * (-kv.x * kv.x / k);
  let dyy = h * (-kv.y * kv.y / k);
  let dxy = h * (-kv.x * kv.y / k);
  o.a = dx + ci(dy);
  o.b = h + ci(dxy);
  o.c = ih * kv.x + ci(ih * kv.y);
  o.d = dxx + ci(dyy);
  return o;
}

fn bitrev(x: u32) -> u32 { return reverseBits(x) >> 24u; }

// In-place inverse radix-2 FFT of the four 256-point rows in `buf`
// (loaded in bit-reversed order).
fn fft(t: u32) {
  for (var s = 0u; s < 8u; s++) {
    workgroupBarrier();
    if (t < 128u) {
      let hw = 1u << s;
      let pos = t & (hw - 1u);
      let i0 = ((t >> s) << (s + 1u)) + pos;
      let i1 = i0 + hw;
      let a = PI * f32(pos) / f32(hw);
      let w = vec2f(cos(a), sin(a));
      for (var f = 0u; f < 4u; f++) {
        let p = buf[f * N + i0];
        let q = cmul(buf[f * N + i1], w);
        buf[f * N + i0] = p + q;
        buf[f * N + i1] = p - q;
      }
    }
  }
  workgroupBarrier();
}

@compute @workgroup_size(256)
fn rows(@builtin(local_invocation_id) lid: vec3u, @builtin(workgroup_id) wid: vec3u) {
  let x = lid.x;
  let y = wid.x;
  let c = wid.y;
  let z = spectrum(c, x, y);
  let r = bitrev(x);
  buf[r] = z.a;
  buf[N + r] = z.b;
  buf[2u * N + r] = z.c;
  buf[3u * N + r] = z.d;
  fft(x);
  for (var f = 0u; f < 4u; f++) {
    spec[((c * 4u + f) * N + y) * N + x] = buf[f * N + x];
  }
}

@compute @workgroup_size(256)
fn cols(@builtin(local_invocation_id) lid: vec3u, @builtin(workgroup_id) wid: vec3u) {
  let y = lid.x;
  let x = wid.x;
  let c = wid.y;
  let r = bitrev(y);
  for (var f = 0u; f < 4u; f++) {
    buf[f * N + r] = spec[((c * 4u + f) * N + y) * N + x];
  }
  fft(y);
  // Wavenumbers run from -N/2: that shifts every other texel's sign.
  let sg = select(1.0, -1.0, ((x + y) & 1u) == 1u);
  let a = buf[y] * sg;
  let b = buf[N + y] * sg;
  let s = buf[2u * N + y] * sg;
  let d = buf[3u * N + y] * sg;
  let lam = U.casc[c].w;
  textureStore(out_t, vec2u(x, y), c, vec4f(a * lam, b.x, b.y * lam));
  textureStore(out_t, vec2u(x, y), c + 3u, vec4f(s, d * lam));
}

@compute @workgroup_size(8, 8, 1)
fn mip(@builtin(global_invocation_id) id: vec3u) {
  let s = textureDimensions(mip_dst);
  if (id.x >= s.x || id.y >= s.y) { return; }
  let p = vec2i(id.xy * 2u);
  let l = i32(id.z);
  let v = textureLoad(mip_src, p, l, 0) + textureLoad(mip_src, p + vec2i(1, 0), l, 0) + textureLoad(mip_src, p + vec2i(0, 1), l, 0) + textureLoad(mip_src, p + vec2i(1, 1), l, 0);
  textureStore(mip_dst, id.xy, l, v * 0.25);
}
