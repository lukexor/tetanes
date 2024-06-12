//  CRT Shader by EasyMode
//  License: GPL
//
//  A flat CRT shader ideally for 1080p or higher displays.
//
//  Recommended Settings:
//
//  Video
//  - Aspect Ratio:  4:3
//  - Integer Scale: Off
//
//  Shader
//  - Filter: Nearest
//  - Scale:  Don't Care
//
//  Example RGB Mask Parameter Settings:
//
//  Aperture Grille (Default)
//  - Dot Width:  1
//  - Dot Height: 1
//  - Stagger:    0
//
//  Lottes' Shadow Mask
//  - Dot Width:  2
//  - Dot Height: 1
//  - Stagger:    3
//
//  Adapted from https://github.com/libretro/glsl-shaders/blob/master/crt/shaders/crt-easymode.glsl

var<private> vertices: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>(3.0, -1.0),
    vec2<f32>(-1.0, 3.0),
);

// Vertex shader

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) v_uv: vec2<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) v_idx: u32
) -> VertexOutput {
    var out: VertexOutput;
    let vert = vertices[v_idx];
    // Convert x from -1.0..1.0 to 0.0..1.0 and y from -1.0..1.0 to 1.0..0.0
    out.v_uv = fma(vert, vec2(0.5, -0.5), vec2(0.5, 0.5));
    out.position = vec4(vert, 0.0, 1.0);
    return out;
}

// Fragment shader

struct Output {
    size: vec2<f32>,
    padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> out: Output;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

const PI = 3.141592653589;

const SHARPNESS_H = 0.5;
const SHARPNESS_V = 1.0;
const MASK_STRENGTH = 0.3;
const MASK_DOT_WIDTH = 1.0;
const MASK_DOT_HEIGHT = 1.0;
const MASK_STAGGER = 0.0;
const MASK_SIZE = 1.0;
const SCANLINE_STRENGTH = 0.95;
const SCANLINE_BEAM_WIDTH_MIN = 2.5;
const SCANLINE_BEAM_WIDTH_MAX = 2.5;
const SCANLINE_BRIGHT_MIN = 0.3;
const SCANLINE_BRIGHT_MAX = 0.6;
const SCANLINE_CUTOFF = 400.0;
const GAMMA_INPUT = 1.0;
const GAMMA_OUTPUT = 2.2;
const BRIGHT_BOOST = 1.1;
const DILATION = 1.0;

// apply half-circle s-curve to distance for sharper (more pixelated) interpolation
fn curve_distance(x: f32, sharp: f32) -> f32 {
    let x_step = step(0.5, x);
    let curve = 0.5 - sqrt(0.25 - (x - x_step) * (x - x_step)) * sign(0.5 - x);

    return mix(x, curve, sharp);
}

fn filter_lanczos(coeffs: vec4<f32>, color_matrix: mat4x4<f32>) -> vec3<f32> {
    var col = color_matrix * coeffs;
    let sample_min = min(color_matrix[1], color_matrix[2]);
    let sample_max = max(color_matrix[1], color_matrix[2]);

    col = clamp(col, sample_min, sample_max);

    return col.rgb;
}

fn dilate(col: vec4<f32>) -> vec4<f32> {
    let x = mix(vec4<f32>(1.0), col, DILATION);

    return col * x;
}

fn tex2d(c: vec2<f32>) -> vec4<f32> {
    return dilate(textureSample(tex, tex_sampler, c));
}

fn get_color_matrix(co: vec2<f32>, dx: vec2<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(tex2d(co - dx), tex2d(co), tex2d(co + dx), tex2d(co + 2.0 * dx));
}


@fragment
fn fs_main(@location(0) v_uv: vec2<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(tex));
    let inv_dims = 1.0 / dims;

    let dx = vec2<f32>(inv_dims.x, 0.0);
    let dy = vec2<f32>(0.0, inv_dims.y);
    let pix_co = v_uv * dims - vec2<f32>(0.5, 0.5);
    let tex_co = (floor(pix_co) + vec2<f32>(0.5, 0.5)) * inv_dims;
    let dist = fract(pix_co);

    var curve_x = curve_distance(dist.x, SHARPNESS_H * SHARPNESS_H);
    var coeffs = PI * vec4<f32>(1.0 + curve_x, curve_x, 1.0 - curve_x, 2.0 - curve_x);

    coeffs = max(abs(coeffs), vec4(1e-5));
    coeffs = 2.0 * sin(coeffs) * sin(coeffs * 0.5) / (coeffs * coeffs);
    coeffs /= dot(coeffs, vec4<f32>(1.0));

    var col = filter_lanczos(coeffs, get_color_matrix(tex_co, dx));
    var col2 = filter_lanczos(coeffs, get_color_matrix(tex_co + dy, dx));

    col = mix(col, col2, curve_distance(dist.y, SHARPNESS_V));
    col = pow(col, vec3<f32>(GAMMA_INPUT / (DILATION + 1.0)));

    let luma = dot(vec3<f32>(0.2126, 0.7152, 0.0722), col);
    let bright = (max(col.r, max(col.g, col.b)) + luma) * 0.5;
    let scan_bright = clamp(bright, SCANLINE_BRIGHT_MIN, SCANLINE_BRIGHT_MAX);
    let scan_beam = clamp(bright * SCANLINE_BEAM_WIDTH_MAX, SCANLINE_BEAM_WIDTH_MIN, SCANLINE_BEAM_WIDTH_MAX);
    var scan_weight = 1.0 - pow(cos(v_uv.y * 2.0 * PI * dims.y) * 0.5 + 0.5, scan_beam) * SCANLINE_STRENGTH;

    let insize = dims;
    let mask = 1.0 - MASK_STRENGTH;
    let mod_fac = floor(v_uv * out.size * dims / (insize * vec2<f32>(MASK_SIZE, MASK_DOT_HEIGHT * MASK_SIZE)));
    let dot_no = i32(((mod_fac.x + (mod_fac.y % 2.0) * MASK_STAGGER) / MASK_DOT_WIDTH % 3.0));

    var mask_weight: vec3<f32>;
    if dot_no == 0 {
        mask_weight = vec3<f32>(1.0, mask, mask);
    } else if dot_no == 1 {
        mask_weight = vec3<f32>(mask, 1.0, mask);
    } else {
        mask_weight = vec3<f32>(mask, mask, 1.0);
    }

    if insize.y >= SCANLINE_CUTOFF {
        scan_weight = 1.0;
    }

    col2 = col.rgb;
    col *= vec3<f32>(scan_weight);
    col = mix(col, col2, scan_bright);
    col *= mask_weight;
    col = pow(col, vec3<f32>(1.0 / GAMMA_OUTPUT));

    return vec4<f32>(col * BRIGHT_BOOST, 1.0);
}
