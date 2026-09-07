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

const PI = 3.141592653589;

const SHARPNESS_H = 0.5;
const SHARPNESS_V = 1.0;
const MASK_STRENGTH = 0.3;
const MASK_DOT_WIDTH = 1.0;
const MASK_DOT_HEIGHT = 1.0;
const MASK_STAGGER = 0.0;
const MASK_SIZE = 1.0;
const SCANLINE_STRENGTH = 1.0;
const SCANLINE_BEAM_WIDTH_MIN = 1.5;
const SCANLINE_BEAM_WIDTH_MAX = 1.5;
const SCANLINE_BRIGHT_MIN = 0.35;
const SCANLINE_BRIGHT_MAX = 0.65;
const SCANLINE_CUTOFF = 2000.0;
const SCANLINE_MIN_SCALE = 2.0;
const GAMMA_INPUT = 2.4;
const GAMMA_OUTPUT = 2.2;
const BRIGHT_BOOST = 1.3;
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

// The emulator writes its frames to an sRGB texture, so a sample comes back linear. The CRT math
// below expects the gamma values a CRT would have been fed.
fn tex2d(c: vec2<f32>) -> vec4<f32> {
    return dilate(gamma_from_linear_rgba(textureSample(tex, tex_sampler, c)));
}

fn get_color_matrix(co: vec2<f32>, dx: vec2<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(tex2d(co - dx), tex2d(co), tex2d(co + dx), tex2d(co + 2.0 * dx));
}

@fragment
fn fs_main(
    @location(0) v_uv: vec2<f32>,
    @location(1) v_color: vec4<f32>
) -> @location(0) vec4<f32> {
    let tex_dims = vec2<f32>(textureDimensions(tex));
    let inv_tex_dims = 1.0 / tex_dims;
    let out_dims = quad_size_in_pixels(v_uv);

    let pix_co = v_uv * tex_dims - vec2<f32>(0.5, 0.5);
    let tex_co = (floor(pix_co) + vec2<f32>(0.5, 0.5)) * inv_tex_dims;
    let dist = fract(pix_co);

    var curve_x = curve_distance(dist.x, SHARPNESS_H * SHARPNESS_H);
    var coeffs = PI * vec4<f32>(1.0 + curve_x, curve_x, 1.0 - curve_x, 2.0 - curve_x);

    coeffs = max(abs(coeffs), vec4(1e-5));
    coeffs = 2.0 * sin(coeffs) * sin(coeffs * 0.5) / (coeffs * coeffs);
    coeffs /= dot(coeffs, vec4<f32>(1.0));

    let dx = vec2<f32>(inv_tex_dims.x, 0.0);
    let dy = vec2<f32>(0.0, inv_tex_dims.y);
    var col = filter_lanczos(coeffs, get_color_matrix(tex_co, dx));
    var col2 = filter_lanczos(coeffs, get_color_matrix(tex_co + dy, dx));

    col = mix(col, col2, curve_distance(dist.y, SHARPNESS_V));
    col = pow(col, vec3<f32>(GAMMA_INPUT / (DILATION + 1.0)));

    let luma = dot(vec3<f32>(0.2126, 0.7152, 0.0722), col);
    let bright = (max(col.r, max(col.g, col.b)) + luma) * 0.5;
    let scan_bright = clamp(bright, SCANLINE_BRIGHT_MIN, SCANLINE_BRIGHT_MAX);
    let scan_beam = clamp(bright * SCANLINE_BEAM_WIDTH_MAX, SCANLINE_BEAM_WIDTH_MIN, SCANLINE_BEAM_WIDTH_MAX);

    // One scanline per source row needs two output rows to resolve, one bright and one dark.
    // Drawn any smaller the scanlines beat against the pixel grid into wide horizontal bands, so
    // fade them out between 1x and 2x instead.
    let scan_fade = smoothstep(1.0, SCANLINE_MIN_SCALE, out_dims.y * inv_tex_dims.y);
    let scan_line = pow(cos(v_uv.y * 2.0 * PI * tex_dims.y) * 0.5 + 0.5, scan_beam);
    var scan_weight = 1.0 - scan_line * SCANLINE_STRENGTH * scan_fade;

    let mask = 1.0 - MASK_STRENGTH;
    let mod_fac = floor(v_uv * out_dims / vec2<f32>(MASK_SIZE, MASK_DOT_HEIGHT * MASK_SIZE));
    let dot_no = i32(((mod_fac.x + (mod_fac.y % 2.0) * MASK_STAGGER) / MASK_DOT_WIDTH % 3.0));

    var mask_weight: vec3<f32>;
    if dot_no == 0 {
        mask_weight = vec3<f32>(1.0, mask, mask);
    } else if dot_no == 1 {
        mask_weight = vec3<f32>(mask, 1.0, mask);
    } else {
        mask_weight = vec3<f32>(mask, mask, 1.0);
    }

    if tex_dims.y >= SCANLINE_CUTOFF {
        scan_weight = 1.0;
    }

    col2 = col.rgb;
    col *= vec3<f32>(scan_weight);
    col = mix(col, col2, scan_bright);
    col *= mask_weight;
    col = pow(col, vec3<f32>(1.0 / GAMMA_OUTPUT));

    return vec4<f32>(col * BRIGHT_BOOST, 1.0) * v_color;
}
