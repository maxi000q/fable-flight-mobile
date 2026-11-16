// Collapses the multisampled depth buffer to one sample per pixel (nearest
// surface) for the passes that read depth as a texture. SAMPLES (1 on a
// phone) is injected before this file by the renderer.
@group(0) @binding(0) var t_d: texture_depth_multisampled_2d;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4f) -> @builtin(frag_depth) f32 {
  let c = vec2i(p.xy);
  var d = textureLoad(t_d, c, 0);
  for (var i = 1u; i < SAMPLES; i++) {
    d = max(d, textureLoad(t_d, c, i));
  }
  return d;
}
