// Fragment stage for egui meshes. Tints the sampled texel and returns gamma values, because the
// surface is a non-sRGB format that stores whatever the shader writes.

@fragment
fn fs_main(
    @location(0) v_uv: vec2<f32>,
    @location(1) v_color: vec4<f32>
) -> @location(0) vec4<f32> {
    let tex = textureSample(tex, tex_sampler, v_uv);
    let tex_gamma = gamma_from_linear_rgba(tex);
    return v_color * tex_gamma;
}
