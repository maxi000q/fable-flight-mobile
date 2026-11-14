// Deferred resolve: sky, water (waves, reflections, refraction, sea bed), haze.
@group(1) @binding(0) var t_color: texture_2d<f32>;
@group(1) @binding(1) var t_aux: texture_2d<f32>;
@group(1) @binding(2) var t_depth: texture_depth_2d;
@group(1) @binding(3) var t_ao: texture_2d<f32>;
@group(1) @binding(4) var t_amb: texture_2d<f32>;

// Wave texture lookup at the mip that matches the pixel footprint (fp, metres).
fn wtex(uv: vec2f, size: f32, fp: f32) -> vec4f {
  return textureSampleLevel(t_wave, s_repeat, uv, log2(max(fp * 256.0 / size, 1.0)));
}

fn ssr(P: vec3f, R: vec3f, jit: f32) -> vec4f {
  let dims = vec2f(textureDimensions(t_depth));
  var step_len = max(length(P) * 0.012, 0.4);
  var t = step_len * (0.5 + jit);
  var prev = 0.0;
  for (var i = 0; i < 30; i++) {
    let q = P + R * t;
    let clip = G.view_proj * vec4f(q, 1.0);
    if (clip.w <= 0.0) { break; }
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2f(0.5, -0.5) + 0.5;
    if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0))) { break; }
    let zs = textureLoad(t_depth, vec2i(uv * dims), 0);
    if (zs > ndc.z) {
      // Behind a surface: is it a thin enough crossing to be a real hit?
      let ds = G.misc.x / zs;
      let dr = G.misc.x / ndc.z;
      if (dr - ds > max(step_len * 2.5, 8.0)) { break; }
      var lo = prev;
      var hi = t;
      var huv = uv;
      for (var k = 0; k < 5; k++) {
        let m = (lo + hi) * 0.5;
        let c2 = G.view_proj * vec4f(P + R * m, 1.0);
        let n2 = c2.xyz / c2.w;
        huv = n2.xy * vec2f(0.5, -0.5) + 0.5;
        if (textureLoad(t_depth, vec2i(huv * dims), 0) > n2.z) { hi = m; } else { lo = m; }
      }
      let px = vec2i(huv * dims);
      let c = textureLoad(t_color, px, 0);
      if (textureLoad(t_aux, px, 0).a > 0.5 || c.a < 0.99 || textureLoad(t_depth, px, 0) == 0.0) { break; }
      let e = min(min(huv.x, 1.0 - huv.x), min(huv.y, 1.0 - huv.y));
      return vec4f(c.rgb, smoothstep(0.0, 0.08, e));
    }
    prev = t;
    step_len *= 1.22;
    t += step_len;
  }
  return vec4f(0.0);
}

fn water(P: vec3f, V: vec3f, dist: f32, tint: vec3f, aux: vec4f, jit: f32) -> vec3f {
  // Edge pixels average in the normals other geometry writes here: keep the
  // water parameters in range or the absorption maths goes negative.
  let wd = max(aux.x, 0.0);
  let wclass = clamp(aux.y, 0.0, 3.0);
  let shore = clamp(aux.z, 0.0, 1.0);
  var l = light_at(P);
  let upn = l.upn;
  // (looked up a little above our waves inside the 3D tiles' zone: the
  // mesh's own still water, a metre or two above ours, casts too)
  l.sun *= shadow_at(P + upn * select(0.0, 4.0, photo_covered(P)), upn);
  let e = normalize(G.east.xyz - upn * dot(G.east.xyz, upn));
  let n = cross(upn, e);
  let wc = wave_coord(P);
  let t = G.time.x;
  let cosv = max(dot(-V, upn), 0.03);
  let fp = dist * G.misc.w / cosv;                // pixel footprint along the view direction, metres
  var st = sea_state(wclass, upn.z, shore);
  let wind = G.ocean[1].x;

  // Wind (from the weather): it blows in gusts, ruffling some patches of the
  // surface and leaving others glassy between (cat's paws), more so in light air.
  let wdir = G.ocean[0].zw;
  let wn = vec2f(-wdir.y, wdir.x);
  let ww = vec2f(dot(wc, wdir), dot(wc, wn));
  let g1 = textureSampleLevel(t_noise, s_repeat, vec3f(ww.x / 520.0 - t * 0.012, ww.y / 170.0, 0.37), 0.0);
  let g2 = textureSampleLevel(t_noise, s_repeat, vec3f(ww.x / 95.0 - t * 0.035, ww.y / 38.0, 0.81), 0.0);
  // The noise volume has no mips: blend each layer to its mean before its
  // cells shrink under a pixel, or the gust pattern turns to sparkle far off.
  let g1r = mix(g1.r, 0.5, smoothstep(10.0, 45.0, fp));
  let g2g = mix(g2.g, 0.5, smoothstep(2.5, 10.0, fp));
  let g2b = mix(g2.b, 0.5, smoothstep(2.5, 10.0, fp));
  let calm = 1.0 - smoothstep(1.5, 7.0, wind);
  let ruffle = smoothstep(0.28, 0.72, g1r * 0.65 + g2g * 0.45 - 0.02 + 0.2 * (1.0 - st.z) - 0.3 * calm);
  // Gusts raise the short waves most.
  st.z *= mix(0.3, 1.2, ruffle);
  st.y *= mix(0.8, 1.1, ruffle);

  // The FFT ocean (ocean.wgsl). This pixel is where the sideways shift
  // carried a surface point to: find that point first (two fixed-point steps
  // over the cascades that move it most), then read its slopes.
  let xo = ocean_coord(P);
  var lod = vec3f(0.0);
  for (var c = 0; c < 3; c++) { lod[c] = log2(max(fp * 256.0 / ocean_size(c), 1.0)); }
  var x0 = xo;
  var hgt = 0.0;
  for (var it = 0; it < 2; it++) {
    var ds = vec3f(0.0);
    for (var c = 0; c < 2; c++) {
      if (lod[c] < 8.0) { ds += textureSampleLevel(t_ocean, s_repeat, x0 / ocean_size(c), c, lod[c]).xyz * st[c]; }
    }
    x0 = xo - ds.xy;
    hgt = ds.z;
  }
  var sl = vec2f(0.0);   // dh/dx, dh/dy over the undisplaced surface
  var jd = vec3f(0.0);   // Dxx, Dyy, Dxy
  var jd01 = vec3f(0.0); // the same for the two longer cascades (whitecaps)
  // Slope variance of everything too small to resolve here: it widens
  // reflections and the sun's glitter instead. Each cascade's waves are lost
  // to the mips over a range of levels (its band in octaves).
  var sig2 = 0.0;
  let lo_a = vec3f(3.7, 3.7, 0.0);
  let lo_b = vec3f(7.0, 6.3, 6.4);
  for (var c = 0; c < 3; c++) {
    let w = st[c];
    if (w <= 0.0) { continue; }
    let L = ocean_size(c);
    if (lod[c] < 8.0) {
      let a = textureSampleLevel(t_ocean, s_repeat, x0 / L, c, lod[c]);
      let b = textureSampleLevel(t_ocean, s_repeat, x0 / L, c + 3, lod[c]);
      sl += b.xy * w;
      jd += vec3f(b.zw, a.w) * w;
      if (c < 2) { jd01 += vec3f(b.zw, a.w) * w; }
    }
    sig2 += G.ocean[2][c] * w * w * clamp((lod[c] - lo_a[c]) / (lo_b[c] - lo_a[c]), 0.0, 1.0);
  }
  // Capillary ripples (shorter than any cascade) count once a pixel outgrows them.
  sig2 += G.ocean[2].w * st.z * st.z * clamp(log2(fp / 0.004) / 3.6, 0.0, 1.0);
  // Surface normal through the sideways shift: slopes over the moved surface.
  let jxx = 1.0 + jd.x;
  let jyy = 1.0 + jd.y;
  let J = jxx * jyy - jd.z * jd.z;
  var slope = vec2f(sl.x * jyy - sl.y * jd.z, sl.y * jxx - sl.x * jd.z) / max(J, 0.25);
  let smag = length(slope);
  if (smag > 1.5) { slope *= 1.5 / smag; }
  let J01 = (1.0 + jd01.x) * (1.0 + jd01.y) - jd01.z * jd01.z;
  let rough = clamp(pow(sig2 * 0.5, 0.25), 0.03, 0.36);
  sig2 *= 0.5;
  let N = normalize(upn - e * slope.x - n * slope.y);

  // Reflection: screen-space first, sky (with cloud glints) as the fallback.
  var R = reflect(V, N);
  let ru = dot(R, upn);
  if (ru < 0.03) { R = normalize(R + upn * (0.03 - ru)); }
  let ndv = max(dot(-V, N), 0.0);
  let F = 0.02 + 0.98 * pow(1.0 - ndv, 5.0) * (1.0 - rough * 1.6);
  let sig = sqrt(sig2);
  var refl = (sky_lut(R) + sky_lut(normalize(R + upn * sig * 1.3)) + sky_lut(normalize(R + upn * sig * 2.8))) / 3.0;
  let mid = G.cloud.y + 400.0;
  if (l.h < mid) {
    // The cloud layer is far away along a grazing reflection: a wave tilt of
    // a few degrees moves the reflected point by kilometres, so neighbouring
    // pixels would see unrelated clouds (sparkling noise). Tame the tilt so
    // the reflected point moves only a few hundred metres - the clouds then
    // reflect as smooth, wave-wobbled shapes, as they do on real water.
    let hc = max(mid - l.h, 100.0);
    let kc = clamp(1500.0 * cosv * cosv / hc, 0.0, 1.0);
    let Rc0 = reflect(V, normalize(upn + (N - upn) * kc));
    // Unresolved ripples smear the reflection upward, into a vertical streak
    // (as the sky term above): sample the deck across that lobe and average.
    // The haze between the water and a reflected cloud fades it toward the
    // sky colour behind it, as it does for the clouds seen directly.
    let ext = atm_ext(atm_density(0.5 * (l.h + mid)));
    let kext = dot(ext, vec3f(0.2126, 0.7152, 0.0722));
    var cov_c = 0.0;
    var tc0 = 0.0;
    for (var k = 0; k < 3; k++) {
      let off = select(select(0.0, 1.3, k == 1), 2.8, k == 2);
      var Rc = normalize(Rc0 + upn * sig * off);
      Rc = normalize(Rc + upn * max(0.03 - dot(Rc, upn), 0.0));
      let tc = hc / max(dot(Rc, upn), 0.03);
      if (k == 0) { tc0 = tc; }
      if (tc < 90000.0) {
        let cd = cloud_density(P - G.planet.xyz + Rc * tc, mid, true);
        cov_c += (1.0 - exp(-cd * 7.0)) * (1.0 - smoothstep(40000.0, 90000.0, tc)) * exp(-tc * kext);
      }
    }
    if (cov_c > 0.0) {
      let lc = light_at(P + Rc0 * min(tc0, 90000.0));
      refl = mix(refl, (lc.sun * 0.05 + lc.amb * 1.3), cov_c / 3.0);
    }
  }
  if (G.misc.z > 0.5) {
    // Same idea for reflected scenery: per-pixel wave normals make the march
    // hit and miss on alternate pixels. Less tilt the further off (where a
    // pixel spans many waves) keeps the wobble while the reflection holds together.
    let ks = mix(0.5, 0.12, smoothstep(250.0, 2500.0, dist));
    var Rs = reflect(V, normalize(upn + (N - upn) * ks));
    Rs = normalize(Rs + upn * max(0.03 - dot(Rs, upn), 0.0));
    let hit = ssr(P, Rs, 0.5);
    refl = mix(refl, hit.rgb, hit.a);
  }

  // Sun glitter.
  let H = normalize(G.sun_dir.xyz - V);
  let a2 = pow(rough, 4.0);
  let ndh = max(dot(N, H), 0.0);
  let dd = ndh * ndh * (a2 - 1.0) + 1.0;
  let D = a2 / (PI * dd * dd);
  var spec = min(D * F / (4.0 * max(ndv, 0.08)), 60.0) * max(dot(N, G.sun_dir.xyz), 0.0) * l.sun * 0.25;
  // Moon glitter: a path of light across the waves.
  if (G.moon.w > 0.0) {
    let Hm = normalize(G.moon.xyz - V);
    let am = max(a2, 0.0004);
    let ndm = max(dot(N, Hm), 0.0);
    let dm = ndm * ndm * (am - 1.0) + 1.0;
    let Dm = am / (PI * dm * dm);
    spec += min(Dm * F / (4.0 * max(ndv, 0.08)), 60.0) * max(dot(N, G.moon.xyz), 0.0) * moon_light(upn, l.h) * 0.25;
  }

  // What kind of water is this? Latitude, wclass and the satellite colour decide.
  let alat = abs(upn.z);
  let trop = 1.0 - smoothstep(0.33, 0.58, alat);
  let polar = smoothstep(0.78, 0.93, alat);
  var turb = mix(0.8, 0.06, trop);
  var body = mix(vec3f(0.004, 0.024, 0.046), vec3f(0.003, 0.05, 0.075), trop);
  body = mix(body, vec3f(0.006, 0.022, 0.03), polar);
  var weedy = mix(1.0, 0.82, trop);
  if (wclass > 2.5) { turb = 3.2; body = vec3f(0.045, 0.048, 0.022); weedy = 0.9; }
  else if (wclass > 1.5) { turb = 1.5; body = vec3f(0.012, 0.03, 0.022); weedy = 1.1; }
  let tl = max(luma(tint), 0.015);
  body = mix(body, tint / tl * luma(body), 0.4);
  let ext = vec3f(0.36, 0.052, 0.024) + vec3f(0.22, 0.2, 0.3) * turb;

  // Refraction down to the bed.
  let Rf = refract(V, N, 0.75);
  let path = wd / max(dot(-Rf, upn), 0.25);
  let Tv = exp(-ext * path);
  let mu = max(dot(upn, G.sun_dir.xyz), 0.0);
  var under = body * (l.sun * (0.3 + 0.7 * mu) / PI + l.amb) * 3.2 * (vec3f(1.0) - Tv);
  if (luma(Tv) > 0.01) {
    let bp = wc + vec2f(dot(Rf, e), dot(Rf, n)) * path;
    let near = 1.0 - smoothstep(300.0, 2500.0, dist);
    // Organic bed detail from the 3D noise volume (no visible repeat).
    let nA = textureSampleLevel(t_noise, s_repeat, vec3f(bp / 140.0, 0.37), 0.0);
    let nB = textureSampleLevel(t_noise, s_repeat, vec3f(bp / 23.0, 0.71), 0.0);
    let n1 = nA.r;
    let n2 = nB.r;
    var bed = vec3f(0.58, 0.51, 0.37) * (0.75 + 0.5 * n2);
    bed = mix(bed, vec3f(0.14, 0.13, 0.12), smoothstep(0.55, 0.75, n1 * 0.7 + nB.g * 0.4) * 0.85 * near);
    // Seaweed and kelp beds that sway with the swell.
    let sway = vec2f(sin(t * 0.8 + bp.y * 0.35), cos(t * 0.65 + bp.x * 0.3)) * 0.35;
    let wA = textureSampleLevel(t_noise, s_repeat, vec3f((bp + sway) / 61.0, 0.13), 0.0);
    let wB = textureSampleLevel(t_noise, s_repeat, vec3f((bp + sway * 0.6) / 9.0, 0.53), 0.0);
    let weed = smoothstep(0.52, 0.66, wA.r * weedy) * smoothstep(0.35, 0.6, wB.g + wB.b * 0.4) * smoothstep(0.6, 2.5, wd) * near;
    bed = mix(bed, vec3f(0.02, 0.07, 0.022) * (0.5 + 1.0 * wB.b), weed * 0.92);
    bed = mix(bed, tint * 2.4, 0.4 * (1.0 - weed));
    // Caustics.
    let c1 = wtex(bp / 6.3 + vec2f(0.03, 0.02) * t, 6.3, fp).b;
    let c2 = wtex(bp / 4.9 - vec2f(0.025, 0.04) * t, 4.9, fp).b;
    let ca = pow(1.0 - abs(c1 - c2), 9.0) * 3.0 * exp(-wd * 0.12) * near * smoothstep(0.1, 0.8, wd);
    let Td = exp(-ext * wd / max(mu, 0.35));
    under += bed * (l.sun * mu * Td * (1.0 + ca) / PI + l.amb * Td) * Tv;
  }

  // Light through the crests: looking toward the sun, the wave between the
  // eye and it glows green where it is thin and lit from behind.
  let hs = max(G.ocean[1].y * (st.x + st.y) * 0.5, 0.2);
  let crest_h = clamp(hgt / hs + 0.45, 0.0, 1.0);
  let toward = pow(max(dot(V, G.sun_dir.xyz), 0.0), 4.0);
  let sss = toward * crest_h * (0.3 + 0.7 * (1.0 - ndv)) * max(dot(G.sun_dir.xyz, upn), 0.0);
  under += (body * 8.0 + vec3f(0.01, 0.06, 0.05)) * l.sun * sss * 0.4 / PI;

  var col = mix(under, refl, clamp(F, 0.0, 1.0)) + spec + point_light(P, upn) * body * 4.0;

  // Foam: surf line at the shore, whitecaps where the crests fold over.
  let fa = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 37.0 + vec2f(0.012, 0.007) * t, t * 0.02), 0.0);
  let fb = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 5.3 - vec2f(0.02, 0.03) * t, 0.4 + t * 0.03), 0.0);
  let gust = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 1900.0 + vec2f(0.002, 0.0) * t, 0.77), 0.0).r;
  // The noise volume has no mips: fade each layer to its mean before the
  // pixel footprint outgrows its features, or it sparkles as white speckle.
  let lace = mix(fb.g * 0.6 + fb.b * 0.5, 0.55, smoothstep(0.12, 0.7, fp));
  let far_a = smoothstep(1.5, 6.0, fp);
  let fa_r = mix(fa.r, 0.5, far_a);
  let fa_g = mix(fa.g, 0.5, far_a);
  let fnear = 1.0 - smoothstep(2500.0, 9000.0, dist);
  let surge = 0.5 + 0.5 * sin(t * 0.7 + wd * 4.0 + fa_r * 6.0);
  var foam = smoothstep(1.1, 0.0, wd / (0.5 + 0.9 * surge)) * smoothstep(0.35, 0.6, lace + fa_g * 0.25) * select(1.0, 0.25, wclass > 1.5);
  // Whitecaps: the sea breaks where the sideways shift folds the surface
  // (the Jacobian falls toward zero), from a moderate breeze up. They break
  // along the crests, so they are drawn out across the wind; from afar a cap
  // is part bubbles, part dark water, so it shows paler.
  let capw = smoothstep(3.5, 9.0, wind) * step(wclass, 1.5);
  let fw = textureSampleLevel(t_noise, s_repeat, vec3f(ww.x / 21.0 - t * 0.02, ww.y / 64.0, 0.55 + t * 0.015), 0.0).r;
  let cap_n = mix(fw, 0.5, smoothstep(1.0, 4.0, fp));
  let fold = 0.9 - J01;
  let near_cap = smoothstep(0.1, 0.55, fold + (fa_r - 0.5) * 0.3 + (cap_n - 0.5) * 0.5 + (gust - 0.6) * 0.8 + (crest_h - 0.6) * 0.3) * smoothstep(0.42, 0.62, lace);
  // Once the mips have smoothed the folds away, as much of the sea as the
  // wind whitens on average, in patches with the gusts.
  let far_cap = clamp(G.ocean[1].z * 2.5 * (0.4 + 1.2 * gust), 0.0, 1.0);
  foam += mix(near_cap, far_cap, smoothstep(2.0, 5.0, lod[1])) * capw * mix(0.8, 0.45, far_a);
  // Streaks of spent foam drawn out downwind of the whitecaps.
  let streak = smoothstep(0.62, 0.85, g2b) * ruffle * capw * 0.3;
  foam = max(foam, streak * smoothstep(0.4, 0.6, lace));
  foam = clamp(foam * fnear, 0.0, 1.0);
  // Foam is bubbles, not paint: holes and lace within it, close up.
  let bub = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 2.1, 0.2 + t * 0.04), 0.0);
  let bub2 = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 0.55, 0.6), 0.0);
  let holes = smoothstep(0.3, 0.65, bub.r * 0.6 + bub2.g * 0.5);
  foam *= mix(1.0, 0.45 + 0.7 * holes, 1.0 - smoothstep(0.04, 0.2, fp));
  col = mix(col, vec3f(0.78, 0.8, 0.8) * (0.85 + 0.3 * bub2.b) * (l.sun * (0.4 + 0.6 * mu) / PI + l.amb), foam);
  return max(col, vec3f(0.0));
}

@fragment fn fs(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  let c0 = textureLoad(t_color, px, 0);
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let jit = ign(in.pos.xy);
  let oc = -G.planet.xyz;
  // Alpha is how much of the pixel geometry covers (the anti-aliased edge).
  let cov = c0.a;
  if (z == 0.0 || cov < 0.004) {
    return vec4f(clamp(sky_color(rd, jit), vec3f(0.0), vec3f(3000.0)), 1.0);
  }
  let P = ray / z;
  let dist = length(P);
  var col = c0.rgb / cov;
  let aux = textureLoad(t_aux, px, 0) / cov;
  if (aux.a <= 0.004) {
    // Ambient occlusion takes away from the sky and bounce light only.
    let share = clamp(textureLoad(t_amb, px, 0).r / cov, 0.0, 1.0);
    col *= 1.0 - share * (1.0 - textureLoad(t_ao, px, 0).r);
  }
  if (aux.a > 0.996) {
    col = water(P, rd, dist, col, aux, jit);
  } else if (aux.a > 0.004) {
    // Water's edge: some samples are water, which stores its unlit tint, the
    // rest lit geometry. Take the tint and parameters from the all-water
    // pixels around, and swap the water's share of the pixel for shaded water.
    var n = 0.0;
    var wt = vec3f(0.0);
    var wa = vec4f(0.0);
    for (var k = 0; k < 8; k++) {
      let o = vec2i(vec2f(cos(f32(k) * 0.785398), sin(f32(k) * 0.785398)) * 1.5);
      let c2 = textureLoad(t_color, px + o, 0);
      let a2 = textureLoad(t_aux, px + o, 0);
      if (c2.a > 0.996 && a2.a > 0.996) { wt += c2.rgb; wa += a2; n += 1.0; }
    }
    let wf = aux.a;
    if (n > 0.0) {
      wt /= n;
      col = max(col + wf * (water(P, rd, dist, wt, wa / n, jit) - wt), vec3f(0.0));
    } else if (wf > 0.5) {
      col = water(P, rd, dist, col, aux, jit);
    }
  }
  else if (aux.a < -0.01 && G.misc.z > 0.5) {
    // Glass: swap the sky the building shader reflected for whatever stands
    // in that direction on screen - the neighbouring towers, the street.
    let Ng = normalize(aux.xyz);
    let R = reflect(rd, Ng);
    let hit = ssr(P + Ng * 0.25, R, jit);
    if (hit.a > 0.0) {
      let upn = normalize(P - G.planet.xyz);
      let env = env_reflect(P, R, upn, dist < 2500.0);
      col = max(col + (hit.rgb - env) * min(-aux.a, 1.0) * hit.a, vec3f(0.0));
    }
  }
  if (dist > 150.0) {
    let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
    let t0 = max(top.x, 0.0);
    let t1 = min(dist, top.y);
    if (t1 > t0) {
      let sc = atm_march(oc, rd, t0, t1, 12, fract(jit + G.time.w * 0.618034));
      col = col * sc.T + sc.L + SKYGLOW * 0.004 * G.misc.y * night_cam() * (1.0 - luma(sc.T));
    }
  }
  if (cov < 0.996) { col = mix(sky_color(rd, jit), col, cov); }
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), 1.0);
}
