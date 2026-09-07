// Vertex stage, bindings and color helpers shared by every pipeline the painter builds.
// `Shader::source` prepends this file to the fragment shader that gives a pipeline its name, so
// both draw the same egui vertex buffer and read the same bind groups.

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_uv: vec2<f32>,
    @location(1) v_color: vec4<f32>, // gamma 0-1
};

struct Output {
    screen_size: vec2<f32>,
    // Uniform buffers need to be at least 16 bytes in WebGL.
    // See https://github.com/gfx-rs/wgpu/issues/2072
    _padding: vec2<u32>,
};
@group(0) @binding(0) var<uniform> out: Output;

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var tex_sampler: sampler;

// 0-1 linear  from  0-1 sRGB gamma
fn linear_from_gamma_rgb(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

// 0-1 sRGB gamma  from  0-1 linear
fn gamma_from_linear_rgb(rgb: vec3<f32>) -> vec3<f32> {
    let cutoff = rgb < vec3<f32>(0.0031308);
    let lower = rgb * vec3<f32>(12.92);
    let higher = vec3<f32>(1.055) * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(higher, lower, cutoff);
}

// 0-1 sRGBA gamma  from  0-1 linear
fn gamma_from_linear_rgba(linear_rgba: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(gamma_from_linear_rgb(linear_rgba.rgb), linear_rgba.a);
}

// [u8; 4] SRGB as u32 -> [r, g, b, a] in 0.-1
fn unpack_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 255u),
        f32((color >> 8u) & 255u),
        f32((color >> 16u) & 255u),
        f32((color >> 24u) & 255u),
    ) / 255.0;
}

fn position_from_screen(screen_pos: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(
        2.0 * screen_pos.x / out.screen_size.x - 1.0,
        1.0 - 2.0 * screen_pos.y / out.screen_size.y,
        0.0,
        1.0,
    );
}

// Size of the drawn quad in physical pixels, recovered from how fast the interpolated texture
// coordinate moves across one fragment. A quad egui emits for an image spans the full 0-1 UV
// range, so the reciprocal of that rate is the quad's on-screen size.
fn quad_size_in_pixels(v_uv: vec2<f32>) -> vec2<f32> {
    return 1.0 / max(abs(vec2<f32>(dpdx(v_uv.x), dpdy(v_uv.y))), vec2<f32>(1e-6));
}

@vertex
fn vs_main(
    @location(0) v_pos: vec2<f32>,
    @location(1) v_uv: vec2<f32>,
    @location(2) v_color: u32,
) -> VertexOutput {
    var out: VertexOutput;
    out.v_uv = v_uv;
    out.v_color = unpack_color(v_color);
    out.position = position_from_screen(v_pos);
    return out;
}
