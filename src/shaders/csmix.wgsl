// Cloud shadow cross-fade (render.rs CloudShadow): each frame, the map lookups
// read (both cascades) blended from the previous map to the front one, texel by
// texel on the front map's grid. Map centres snap to whole outer texels of a
// shared tangent frame, so the previous map's texel for the same spot is an
// integer shift away.
struct Mix {
  lay: vec4i, // first layer of the previous map, of the front one; previous usable
  sh: vec4i,  // texel shift from front to previous: inner cascade xy, outer zw
  a: vec4f,   // how far faded onto the front map
};
@group(0) @binding(0) var t_maps: texture_2d_array<f32>;
@group(0) @binding(1) var<uniform> M: Mix;

struct FsIn { @builtin(position) pos: vec4f };
@vertex fn vs_full(@builtin(vertex_index) i: u32) -> FsIn {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return FsIn(vec4f(p * 2.0 - 1.0, 0.0, 1.0));
}
fn mixed(px: vec2i, k: i32, sh: vec2i) -> vec4f {
  let f = textureLoad(t_maps, px, M.lay.y + k, 0);
  let q = px + sh;
  let n = i32(textureDimensions(t_maps).x);
  if (M.lay.z == 0 || any(q < vec2i(0)) || any(q >= vec2i(n))) { return f; }
  return mix(textureLoad(t_maps, q, M.lay.x + k, 0), f, M.a.x);
}
@fragment fn fs_in(in: FsIn) -> @location(0) vec4f { return mixed(vec2i(in.pos.xy), 0, M.sh.xy); }
@fragment fn fs_out(in: FsIn) -> @location(0) vec4f { return mixed(vec2i(in.pos.xy), 1, M.sh.zw); }
