// The F/A-18C's cockpit (aircraft/fa18/pit.rs): what its screens show, the
// HUD symbology on the combiner, and what the canopy mirrors from inside.
// Appended to plane.wgsl after deck747.wgsl (whose text and line helpers it
// uses), with the string table in front.
//
// Jet instrument params (main.rs `instruments`):
// params[0] IAS kt, altitude ft, vertical speed fpm, heading deg
// params[1] pitch deg, bank deg, turn rate, slip
// params[2] N1 (fraction), EGT C, fuel flow kg/h an engine, fuel on board kg
// params[3] Mach, ground speed kt, radio altitude ft, TAT C
// params[4] throttle 0..1, local time s, gear 0..1, flaps deg

const FLOOR18 = 0.16;  // pit.rs FLOOR
const G18 = vec3f(0.30, 1.0, 0.42);    // the DDIs' and the HUD's green
const LCD18 = vec3f(0.28, 1.0, 0.36);  // the UFC's windows
const IFEI18 = vec3f(0.86, 0.92, 0.84);

// The labels beside a display's twenty buttons (0: none). Top and bottom read
// left to right, the sides top to bottom.
fn ddi_labels(p: vec2f, aa: f32, top: array<u32, 5>, bot: array<u32, 5>, lft: array<u32, 5>, rgt: array<u32, 5>) -> f32 {
  var a = 0.0;
  for (var k = 0; k < 5; k++) {
    let t = f32(k - 2) * 0.375;
    if (top[k] > 0u) { a = max(a, str_at(p, vec2f(t, 0.87), top[k], 0.075, aa, 0.0)); }
    if (bot[k] > 0u) { a = max(a, str_at(p, vec2f(t, -0.87), bot[k], 0.075, aa, 0.0)); }
    if (lft[k] > 0u) { a = max(a, str_at(p, vec2f(-0.93, -t), lft[k], 0.075, aa, -1.0)); }
    if (rgt[k] > 0u) { a = max(a, str_at(p, vec2f(0.93, -t), rgt[k], 0.075, aa, 1.0)); }
  }
  return a;
}

// The left DDI: the ADI page.
fn ddi_adi(p: vec2f, aa: f32) -> f32 {
  let P0 = O.params[0];
  let P1 = O.params[1];
  let pitch = P1.x;
  let bank = P1.y;
  var a = 0.0;
  let k = 0.030;
  if (abs(p.x) < 0.66 && abs(p.y) < 0.66) {
    let q = rot2(p, rad(bank));
    let rel = q.y / k + pitch;
    // Horizon, and the ladder every 5 degrees, dashed below.
    a = max(a, aa_line(q.y + pitch * k, 0.007, aa) * step(0.10, abs(q.x)) * step(abs(q.x), 0.62));
    let st = round(rel / 5.0) * 5.0;
    if (abs(st) > 0.1) {
      let dy = (rel - st) * k;
      let dash = select(1.0, step(0.45, fract(q.x * 9.0)), st < 0.0);
      a = max(a, aa_line(dy, 0.0045, aa) * step(0.12, abs(q.x)) * step(abs(q.x), 0.30) * dash);
      let n = num_at(vec2f(abs(q.x), q.y), vec2f(0.38, (st - pitch) * k), abs(st), 0u, 1u, 0.055, aa, 0.0);
      a = max(a, n);
    }
  }
  // The aircraft: a fixed W.
  let w = array<vec2f, 7>(vec2f(-0.16, 0.0), vec2f(-0.08, 0.0), vec2f(-0.04, -0.05), vec2f(0.0, 0.0), vec2f(0.04, -0.05), vec2f(0.08, 0.0), vec2f(0.16, 0.0));
  for (var i = 0; i < 6; i++) { a = max(a, seg_line(p, w[i], w[i + 1], 0.006, aa)); }
  // Bank scale along the top, its pointer.
  let r = length(p);
  let ang = atan2(p.x, p.y);
  if (p.y > 0.0 && r > 0.50 && r < 0.58) {
    for (var i = -3; i <= 3; i++) {
      let t = rad(f32(i) * select(10.0, 15.0, abs(i) == 3));
      let len = select(0.53, 0.50, i == 0 || abs(i) == 3);
      a = max(a, aa_line((ang - t) * r, 0.005, aa) * step(len, r));
    }
  }
  let pa = rad(-bank);
  let tip = vec2f(sin(pa), cos(pa)) * 0.49;
  let side = vec2f(cos(pa), -sin(pa)) * 0.025;
  a = max(a, seg_line(p, tip, tip - tip * 0.08 + side, 0.004, aa));
  a = max(a, seg_line(p, tip, tip - tip * 0.08 - side, 0.004, aa));
  // Speed and altitude in boxes, heading at the bottom.
  a = max(a, box_line(p, vec2f(-0.56, 0.0), vec2f(0.13, 0.05), 0.004, aa));
  a = max(a, num_at(p, vec2f(-0.56, 0.0), P0.x, 0u, 1u, 0.065, aa, 0.0));
  a = max(a, box_line(p, vec2f(0.56, 0.0), vec2f(0.15, 0.05), 0.004, aa));
  a = max(a, num_at(p, vec2f(0.56, 0.0), round(P0.y / 10.0) * 10.0, 0u, 1u, 0.065, aa, 0.0));
  a = max(a, box_line(p, vec2f(0.0, -0.70), vec2f(0.09, 0.045), 0.004, aa));
  a = max(a, num_at(p, vec2f(0.0, -0.70), round(P0.w), 0u, 3u, 0.06, aa, 0.0));
  a = max(a, num_at(p, vec2f(0.56, -0.12), round(P0.z / 10.0) * 10.0, 0u, 1u, 0.045, aa, 0.0));
  a = max(a, ddi_labels(p, aa, array<u32, 5>(S_ADI, 0u, S_STBY, 0u, S_SCL), array<u32, 5>(S_STORES, S_HSI, S_MENU, S_SA, S_DATA),
                        array<u32, 5>(0u, S_AUTO, 0u, 0u, 0u), array<u32, 5>(S_UFC, 0u, 0u, S_DCLTR, 0u)));
  return a;
}

// The right DDI: the radar's attack page (range while search, a B-scan).
fn ddi_radar(p: vec2f, aa: f32) -> f32 {
  var a = 0.0;
  let c = vec2f(0.0, -0.03);
  let h = vec2f(0.66, 0.66);
  a = max(a, box_line(p, c, h, 0.004, aa));
  let q = p - c;
  // Azimuth ticks top and bottom, range ticks down the left side.
  for (var i = -2; i <= 2; i++) {
    let x = f32(i) * 0.33;
    a = max(a, seg_line(q, vec2f(x, h.y), vec2f(x, h.y - select(0.03, 0.05, i == 0)), 0.004, aa));
    a = max(a, seg_line(q, vec2f(x, -h.y), vec2f(x, -h.y + select(0.03, 0.05, i == 0)), 0.004, aa));
    let y = f32(i) * 0.33;
    a = max(a, seg_line(q, vec2f(-h.x, y), vec2f(-h.x + 0.04, y), 0.004, aa));
  }
  // The antenna sweeping: the azimuth caret along the bottom and a faint scan line.
  let sx = sin(G.time.x * 1.1) * 0.6;
  a = max(a, seg_line(q, vec2f(sx, -h.y - 0.01), vec2f(sx - 0.03, -h.y - 0.06), 0.004, aa));
  a = max(a, seg_line(q, vec2f(sx, -h.y - 0.01), vec2f(sx + 0.03, -h.y - 0.06), 0.004, aa));
  a = max(a, aa_line(q.x - sx, 0.003, aa) * step(abs(q.y), h.y) * 0.25);
  // The elevation caret on the left.
  a = max(a, seg_line(q, vec2f(-h.x - 0.06, 0.06), vec2f(-h.x - 0.02, 0.04), 0.004, aa));
  // A few contacts: bricks drifting slowly down the scope, brighter for a while after the sweep passes.
  for (var i = 0; i < 4; i++) {
    let s = f32(i) * 7.31 + 1.7;
    let bx = (hash1(s) - 0.5) * 1.1;
    let by = fract(hash1(s + 3.3) - G.time.x * 0.004 * (0.5 + hash1(s + 1.1))) * 1.2 - 0.6;
    let age = fract((G.time.x * 1.1 / 6.2831853) - (bx / 1.2 + 0.25) * 0.5);
    let lit = 0.35 + 0.65 * (1.0 - age);
    a = max(a, aa_fill(sd_box(q, vec2f(bx, by), vec2f(0.022, 0.012)), aa) * lit);
  }
  a = max(a, str_at(p, vec2f(-0.80, 0.75), S_40, 0.07, aa, -1.0));
  a = max(a, ddi_labels(p, aa, array<u32, 5>(S_RWS, 0u, 0u, S_STT, S_RSET), array<u32, 5>(0u, S_SIL, S_MENU, 0u, S_TWS),
                        array<u32, 5>(0u, S_AZ, S_EL, 0u, 0u), array<u32, 5>(S_OPR, 0u, 0u, S_SCL, 0u)));
  return a;
}

// The AMPCD: a heading-up colour moving map under the HSI's compass rose.
fn ampcd(p: vec2f, aa: f32) -> vec3f {
  let P0 = O.params[0];
  let hdg = P0.w;
  let c = vec2f(0.0, -0.22);
  let q = p - c;
  // The chart: water, low ground, hills, a road or two.
  let m = rot2(q, -rad(hdg)) * 0.5 + vec2f(0.37, 0.61) + vec2f(sin(G.time.x * 0.01), cos(G.time.x * 0.013)) * 0.05;
  let land = nz(vec3f(m * 0.6, 0.37)).g + 0.15 * nz(vec3f(m * 2.4, 0.11)).r;
  var col = mix(vec3f(0.05, 0.12, 0.35), vec3f(0.25, 0.33, 0.16), smoothstep(0.47, 0.5, land));
  col = mix(col, vec3f(0.42, 0.33, 0.20), smoothstep(0.62, 0.75, land));
  col = mix(col, vec3f(0.85, 0.75, 0.55), smoothstep(0.80, 0.86, land));
  let road = aa_line(nz(vec3f(m * 1.3, 0.73)).b - 0.5, 0.006, aa * 2.0) * smoothstep(0.5, 0.52, land);
  col = put(col, vec3f(0.85, 0.55, 0.25), road);
  col *= 0.75;
  // The compass rose: ticks every 5 degrees, longer every 10, cardinal letters, numbers every 30.
  let r = length(q);
  let ang = atan2(q.x, q.y) + rad(hdg);
  let deg = ang * 57.29578;
  let d5 = abs(deg / 5.0 - round(deg / 5.0)) * 5.0;
  let len = select(0.03, 0.055, abs(deg / 10.0 - round(deg / 10.0)) < 0.01 || d5 < 0.01 && abs(fract(deg / 10.0 + 0.5) - 0.5) < 0.01);
  var ink = aa_line(d5 * 0.0174533 * r, 0.004, aa) * step(0.60 - len, r) * step(r, 0.60);
  ink = max(ink, aa_line(r - 0.60, 0.003, aa));
  for (var i = 0; i < 12; i++) {
    let a0 = rad(f32(i) * 30.0 - hdg);
    let pc = c + vec2f(sin(a0), cos(a0)) * 0.48;
    var s = 0u;
    switch i { case 0: { s = S_N; } case 3: { s = S_E; } case 6: { s = S_S; } case 9: { s = S_W; } default: {} }
    if (s > 0u) { ink = max(ink, str_at(p, pc, s, 0.075, aa, 0.0)); }
    else { ink = max(ink, num_at(p, pc, f32(i) * 3.0, 0u, 1u, 0.06, aa, 0.0)); }
  }
  col = put(col, vec3f(0.95), ink);
  // The course to the next waypoint (magenta), the waypoint, the aircraft.
  let brg = rad(40.0 - hdg);
  let dir = vec2f(sin(brg), cos(brg));
  let wp = c + dir * 0.42;
  col = put(col, vec3f(1.0, 0.3, 1.0), seg_line(p, c, wp, 0.006, aa));
  col = put(col, vec3f(1.0, 0.3, 1.0), aa_line(length(p - wp) - 0.025, 0.005, aa));
  let ac = array<vec2f, 4>(vec2f(0.0, 0.06), vec2f(0.0, -0.06), vec2f(-0.05, 0.0), vec2f(0.05, 0.0));
  col = put(col, vec3f(1.0), max(seg_line(q, ac[0], ac[1], 0.008, aa), seg_line(q, ac[2], ac[3], 0.008, aa)));
  col = put(col, vec3f(1.0), seg_line(q, vec2f(-0.025, -0.05), vec2f(0.025, -0.05), 0.006, aa));
  // Heading box, the labels.
  col = put(col, vec3f(0.0), aa_fill(sd_box(p, vec2f(0.0, 0.62), vec2f(0.09, 0.05)), aa));
  col = put(col, vec3f(0.95), box_line(p, vec2f(0.0, 0.62), vec2f(0.09, 0.05), 0.004, aa));
  col = put(col, vec3f(0.95), num_at(p, vec2f(0.0, 0.62), round(hdg), 0u, 3u, 0.06, aa, 0.0));
  let lab = ddi_labels(p, aa, array<u32, 5>(S_HSI, S_MAP, 0u, S_TCN, S_WYPT), array<u32, 5>(S_DATA, S_SCL, S_MENU, S_SEQ1, S_TAC),
                       array<u32, 5>(0u, S_AUTO, 0u, 0u, S_DCLTR), array<u32, 5>(0u, S_GS, S_RNG, 0u, 0u));
  col = put(col, G18, lab);
  return col * step(abs(p.x), 1.0) * step(abs(p.y), 1.0);
}

// The RWR scope: range rings, the aircraft in the middle, the threats round it.
fn rwr(p: vec2f, aa: f32) -> f32 {
  let r = length(p);
  var a = max(aa_line(r - 0.33, 0.006, aa), aa_line(r - 0.66, 0.006, aa));
  a = max(a, aa_line(r - 0.96, 0.008, aa));
  a = max(a, max(aa_line(p.x, 0.005, aa), aa_line(p.y, 0.005, aa)) * step(r, 0.12));
  let hdg = rad(O.params[0].w);
  let threats = array<vec3f, 3>(vec3f(rad(35.0), 0.55, 2.0), vec3f(rad(210.0), 0.78, 6.0), vec3f(rad(300.0), 0.42, 4.0));
  for (var i = 0; i < 3; i++) {
    let t = threats[i];
    let pc = vec2f(sin(t.x - hdg), cos(t.x - hdg)) * t.y;
    a = max(a, num_at(p, pc, t.z, 0u, 1u, 0.16, aa, 0.0));
    if (i == 0) { a = max(a, aa_line(sd_box(rot2(p - pc, 0.785), vec2f(0.0), vec2f(0.1)), 0.008, aa)); }
  }
  return a;
}

fn display18(id: u32, uv: vec2f, px: f32) -> vec3f {
  let aa = max(px * 1.1, 0.004);
  let P2 = O.params[2];
  let P4 = O.params[4];
  let bg = vec3f(0.0, 0.010, 0.004);
  switch id {
    case 40u: { return mix(bg, G18, ddi_adi(uv, aa)); }
    case 41u: { return mix(bg, G18, ddi_radar(uv, aa)); }
    case 42u: { return ampcd(uv, aa); }
    case 43u: {
      // The scratchpad: the TACAN channel being entered.
      let q = uv * vec2f(4.0, 1.0);
      return mix(vec3f(0.0, 0.02, 0.006), LCD18, max(str_at(q, vec2f(3.3, 0.0), S_71X, 1.1, aa * 4.0, 1.0), str_at(q, vec2f(-3.3, 0.0), S_TCN, 1.1, aa * 4.0, -1.0)));
    }
    case 44u, 45u, 46u, 47u, 48u: {
      let k = id - 44u;
      let words = array<u32, 5>(S_T_R, S_RCV, S_A_A, S_X, S_Y);
      let q = uv * vec2f(2.7, 1.0);
      var a = str_at(q, vec2f(0.4, 0.0), words[k], 1.0, aa * 3.0, 0.0);
      // The cue: a colon by the selected option.
      if (k == 0u) { a = max(a, max(aa_fill(length(q - vec2f(-1.6, 0.3)) - 0.13, aa * 3.0), aa_fill(length(q - vec2f(-1.6, -0.3)) - 0.13, aa * 3.0))); }
      return mix(vec3f(0.0, 0.02, 0.006), LCD18, a);
    }
    case 49u: { return mix(vec3f(0.0, 0.02, 0.006), LCD18, num_at(uv, vec2f(0.0), 2.0, 0u, 1u, 1.3, aa * 2.0, 0.0)); }
    case 50u: { return mix(vec3f(0.0, 0.02, 0.006), LCD18, str_at(uv, vec2f(0.0), S_M, 1.3, aa * 2.0, 0.0)); }
    case 51u: {
      // IFEI, engines: RPM, exhaust temperature, fuel flow (pph x 100), nozzle position, oil pressure.
      let q = uv * vec2f(0.84, 1.0);
      let n1 = P2.x;
      let x = sat((n1 - 0.62) / 0.38);
      let ab = burner();
      let vals = array<f32, 5>(round(70.0 + 30.0 * x + ab * 2.0), round(P2.y + ab * 60.0), round(P2.z * 2.2046 / 100.0 * (1.0 + ab * 2.5)), round(mix(78.0, 4.0, x) + ab * 70.0), round(42.0 + 48.0 * x));
      let names = array<u32, 5>(S_RPM, S_TEMP, S_FF, S_NOZ, S_OIL);
      var a = 0.0;
      for (var i = 0; i < 5; i++) {
        let y = 0.76 - f32(i) * 0.38;
        a = max(a, str_at(q, vec2f(0.0, y), names[i], 0.10, aa, 0.0));
        a = max(a, num_at(q, vec2f(-0.18, y), vals[i], 0u, 1u, 0.17, aa, 1.0));
        a = max(a, num_at(q, vec2f(0.62, y), vals[i], 0u, 1u, 0.17, aa, 1.0));
      }
      return mix(vec3f(0.006), IFEI18, a);
    }
    case 52u: {
      // IFEI, fuel: total and internal (lb), the bingo setting, the time of day.
      let q = uv * vec2f(0.84, 1.0);
      let lb = round(P2.w * 2.2046 / 10.0) * 10.0;
      var a = num_at(q, vec2f(0.50, 0.70), lb, 0u, 1u, 0.20, aa, 1.0);
      a = max(a, str_at(q, vec2f(0.68, 0.70), S_T, 0.13, aa, 0.0));
      a = max(a, num_at(q, vec2f(0.50, 0.36), lb, 0u, 1u, 0.20, aa, 1.0));
      a = max(a, str_at(q, vec2f(0.68, 0.36), S_I, 0.13, aa, 0.0));
      a = max(a, str_at(q, vec2f(0.0, 0.06), S_BINGO, 0.10, aa, 0.0));
      a = max(a, num_at(q, vec2f(0.50, -0.20), 3000.0, 0u, 1u, 0.17, aa, 1.0));
      let t = P4.y;
      let hh = floor(t / 3600.0) % 24.0;
      let mm = floor(t / 60.0) % 60.0;
      let ss = floor(t) % 60.0;
      a = max(a, num_at(q, vec2f(-0.48, -0.70), hh, 0u, 2u, 0.15, aa, 0.0));
      a = max(a, num_at(q, vec2f(-0.08, -0.70), mm, 0u, 2u, 0.15, aa, 0.0));
      a = max(a, num_at(q, vec2f(0.32, -0.70), ss, 0u, 2u, 0.15, aa, 0.0));
      for (var i = 0; i < 2; i++) {
        let x = -0.28 + f32(i) * 0.40;
        a = max(a, max(aa_fill(length(q - vec2f(x, -0.66)) - 0.014, aa), aa_fill(length(q - vec2f(x, -0.74)) - 0.014, aa)));
      }
      return mix(vec3f(0.006), IFEI18, a);
    }
    case 53u: { return mix(vec3f(0.0, 0.012, 0.004), G18, rwr(uv, aa)); }
    default: {}
  }
  return bg;
}

// ---------------------------------------------------------------- HUD

// The symbology, in degrees from the boresight (x right, y up): conformal
// pitch ladder and velocity vector, heading along the top, airspeed and
// altitude in boxes either side of the waterline.
fn hud_ink(q: vec2f, aa: f32) -> f32 {
  let P0 = O.params[0];
  let P1 = O.params[1];
  let P3 = O.params[3];
  let pitch = P1.x;
  let bank = P1.y;
  let w = 0.028;
  var a = 0.0;
  // The velocity vector: where the aircraft is going (flight path angle from
  // the climb rate and ground speed; the angle of attack is the difference).
  let gamma = degrees(atan2(P0.z / 196.85, max(P3.y, 30.0) * 0.5144));
  let alpha = clamp((pitch - gamma) / max(cos(rad(bank)), 0.4), -6.0, 20.0);
  let fpm = vec2f(0.0, -alpha);
  let f = q - fpm;
  a = max(a, aa_line(length(f) - 0.45, w, aa));
  a = max(a, seg_line(f, vec2f(0.45, 0.0), vec2f(1.25, 0.0), w, aa));
  a = max(a, seg_line(f, vec2f(-0.45, 0.0), vec2f(-1.25, 0.0), w, aa));
  a = max(a, seg_line(f, vec2f(0.0, 0.45), vec2f(0.0, 0.95), w, aa));
  // The ladder, centred on the velocity vector, turning with the bank.
  let r = rot2(q - vec2f(0.0, fpm.y), rad(bank));
  let v = r.y + fpm.y + pitch;  // pitch angle of this point, near enough
  let st = round(v / 5.0) * 5.0;
  let dy = v - st;
  if (abs(st) < 0.1) {
    a = max(a, aa_line(dy, w, aa) * step(1.8, abs(r.x)) * step(abs(r.x), 9.0));
  } else if (abs(r.x) < 6.0) {
    let dash = select(1.0, step(0.4, fract(abs(r.x) * 1.6)), st < 0.0);
    a = max(a, aa_line(dy, w, aa) * step(1.8, abs(r.x)) * step(abs(r.x), 4.0) * dash);
    // The ends bend toward the horizon.
    let tick = select(-0.5, 0.5, st < 0.0);
    a = max(a, aa_line(abs(r.x) - 4.0, w, aa) * step(min(0.0, tick), dy) * step(dy, max(0.0, tick)));
    a = max(a, num_at(vec2f(abs(r.x), r.y), vec2f(4.7, st - pitch - fpm.y + 0.0), abs(st), 0u, 1u, 0.42, aa, 0.0));
  }
  // The waterline at the boresight.
  let wl = array<vec2f, 6>(vec2f(-1.4, 0.0), vec2f(-0.7, 0.0), vec2f(-0.35, -0.45), vec2f(0.0, 0.0), vec2f(0.35, -0.45), vec2f(0.7, 0.0));
  for (var i = 0; i < 5; i++) { a = max(a, seg_line(q, wl[i], wl[i + 1], w, aa)); }
  a = max(a, seg_line(q, vec2f(0.7, 0.0), vec2f(1.4, 0.0), w, aa));
  // Heading scale along the top: a tick every 5 degrees, numbered every 10, the caret under it.
  let hdg = P0.w;
  let hy = 1.25;
  let s = 0.45;
  let hx = q.x / s + hdg;
  let h5 = round(hx / 5.0) * 5.0;
  if (abs(q.x) < 4.5) {
    let tall = abs(h5 / 10.0 - round(h5 / 10.0)) < 0.01;
    a = max(a, aa_line((hx - h5) * s, w, aa) * step(hy, q.y) * step(q.y, hy + select(0.22, 0.4, tall)));
    if (tall) {
      let hv = (h5 + 360.0) % 360.0;
      a = max(a, num_at(q, vec2f((h5 - hdg) * s, hy + 0.85), round(hv / 10.0), 0u, 2u, 0.42, aa, 0.0));
    }
  }
  a = max(a, seg_line(q, vec2f(0.0, hy - 0.05), vec2f(-0.2, hy - 0.35), w, aa));
  a = max(a, seg_line(q, vec2f(0.0, hy - 0.05), vec2f(0.2, hy - 0.35), w, aa));
  // Airspeed and altitude in boxes, Mach and the climb rate under them.
  a = max(a, box_line(q, vec2f(-4.8, -1.0), vec2f(0.8, 0.34), w, aa));
  a = max(a, num_at(q, vec2f(-4.8, -1.0), round(P0.x), 0u, 1u, 0.45, aa, 0.0));
  a = max(a, box_line(q, vec2f(4.8, -1.0), vec2f(1.0, 0.34), w, aa));
  a = max(a, num_at(q, vec2f(4.8, -1.0), round(P0.y / 10.0) * 10.0, 0u, 1u, 0.45, aa, 0.0));
  a = max(a, str_at(q, vec2f(-5.4, -2.0), S_M, 0.38, aa, 0.0));
  a = max(a, num_at(q, vec2f(-4.6, -2.0), P3.x, 2u, 1u, 0.38, aa, 0.0));
  a = max(a, num_at(q, vec2f(4.8, -2.0), round(P0.z / 10.0) * 10.0, 0u, 1u, 0.38, aa, 0.0));
  a = max(a, str_at(q, vec2f(-5.4, -2.8), S_G, 0.38, aa, 0.0));
  a = max(a, num_at(q, vec2f(-4.6, -2.8), 1.0 / max(cos(rad(bank)), 0.15), 1u, 1u, 0.38, aa, 0.0));
  // Bank angle scale under it all: ticks at 0, 5, 10, 15, 20, 30, 45, the pointer.
  let bc = vec2f(0.0, 2.0);
  let br = q - bc;
  let bl = length(br);
  let ba = atan2(br.x, -br.y) * 57.29578;
  if (bl > 9.6 && bl < 10.3 && abs(ba) < 50.0) {
    let ticks = array<f32, 7>(0.0, 5.0, 10.0, 15.0, 20.0, 30.0, 45.0);
    for (var i = 0; i < 7; i++) {
      let len = select(0.35, 0.6, i == 0 || i == 2 || i == 5 || i == 6);
      a = max(a, aa_line((abs(ba) - ticks[i]) * 0.01745 * bl, w, aa) * step(bl, 9.6 + len));
    }
  }
  let pa = rad(clamp(-bank, -50.0, 50.0));
  let tip = bc + vec2f(sin(pa), -cos(pa)) * 9.5;
  let side = vec2f(cos(pa), sin(pa)) * 0.25;
  a = max(a, seg_line(q, tip, tip + (tip - bc) * -0.05 + side, w, aa));
  a = max(a, seg_line(q, tip, tip + (tip - bc) * -0.05 - side, w, aa));
  return a;
}

fn hud18(in: VOut, px: f32) -> vec4f {
  let V = normalize(in.P);
  let ml = transpose(mat3x3f(O.model[0].xyz, O.model[1].xyz, O.model[2].xyz));
  let d = ml * V;
  let q = vec2f(degrees(atan2(d.x, -d.z)), degrees(atan2(d.y, length(d.xz))));
  let aa = max(degrees(px / max(length(in.P), 0.05)), 0.002) * 1.3;
  let ink = hud_ink(q, aa);
  let l = light_at(in.P);
  let night = 1.0 - smoothstep(-0.08, 0.1, dot(l.upn, G.sun_dir.xyz));
  // Bright enough to read against a sunlit cloud by day, turned well down at night.
  let k = G.sun_dir.w * mix(0.16, 0.010, night);
  // The combiner's coating mirrors a little green-gold of the sky.
  let coat = vec3f(0.04, 0.05, 0.03) * l.amb;
  return vec4f(G18 * ink * k + coat, 0.06);
}

// What the canopy mirrors seen from the seat: the grey cockpit (lit by the
// sun when it reaches in) below, darkness above.
fn pit_mirror(p0: vec3f, r: vec3f, l: Light, ls: vec3f) -> vec3f {
  let down = sat(-r.y * 2.5 + 0.2);
  let n = 0.6 + 0.8 * nz(r * 1.3 + p0 * 0.7).r;
  return (l.amb * (0.012 + 0.035 * down) + l.sun * max(ls.y, 0.0) * 0.025 * down) * n;
}
