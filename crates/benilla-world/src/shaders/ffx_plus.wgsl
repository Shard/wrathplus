// Plus world-only postprocess. Stock FFX shader remains untouched.
#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

@group(0) @binding(0) var in_tex: texture_2d<f32>;
@group(0) @binding(1) var in_samp: sampler;
// Combine-pass bindings, unbound in the downsample and blur passes.
@group(0) @binding(2) var blur_tex: texture_2d<f32>;
struct FfxCombine {
    // x = zone glow weight `w` (both combines), y = FFXDeath gate (1 while a ghost, no ramp),
    // z = haze mix, w = deband dither on.
    lane: vec4<f32>,
    // xy = the GlowWave phases `(t mod 3174)/3174` and `(t mod 2805)/2805`, t in ms; zw unused.
    wave: vec4<f32>,
}
@group(0) @binding(3) var<uniform> ffx: FfxCombine;
// The 128×128 wave LUT and its repeat sampler: bound on every combine, sampled only underwater.
@group(0) @binding(4) var wave_tex: texture_2d<f32>;
@group(0) @binding(5) var wave_samp: sampler;



// Plus filters operate in linear light, never on UI or booth targets.
fn decode_sample(uv: vec2<f32>) -> vec4<f32> {
    let c = textureSample(in_tex, in_samp, uv);
    return vec4<f32>(srgb_to_linear(max(c.rgb, vec3<f32>(0.0))), c.a);
}

@fragment
fn fs_downsample(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(in_tex));
    return (decode_sample(in.uv + vec2<f32>(-1.5, -1.5) * texel)
        + decode_sample(in.uv + vec2<f32>(0.5, -1.5) * texel)
        + decode_sample(in.uv + vec2<f32>(0.5, 0.5) * texel)
        + decode_sample(in.uv + vec2<f32>(-1.5, 0.5) * texel)) * 0.25;
}

// Gauss4: taps at ±0.5 and ±2.5 source texels (0x6cad10), weights from FFXGauss4.bls.
fn gauss4(uv: vec2<f32>, axis: vec2<f32>) -> vec4<f32> {
    let texel = axis / vec2<f32>(textureDimensions(in_tex));
    return textureSample(in_tex, in_samp, uv - 2.5 * texel) * 0.125
        + textureSample(in_tex, in_samp, uv - 0.5 * texel) * 0.375
        + textureSample(in_tex, in_samp, uv + 0.5 * texel) * 0.375
        + textureSample(in_tex, in_samp, uv + 2.5 * texel) * 0.125;
}

@fragment
fn fs_gauss_h(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return gauss4(in.uv, vec2<f32>(1.0, 0.0));
}

@fragment
fn fs_gauss_v(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return gauss4(in.uv, vec2<f32>(0.0, 1.0));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Deviation, opt-in (`WOW_DITHER=1`): a dither against banding, which the reference's 8-bit
// framebuffer lacks. The hash is Bevy's `screen_space_dither`, copied here because Bevy applies
// it only in the tonemapping pass that `Tonemapping::None` skips.
fn screen_space_dither(frag_coord: vec2<f32>) -> vec3<f32> {
    var dither = vec3<f32>(dot(vec2<f32>(171.0, 231.0), frag_coord)).xxx;
    dither = fract(dither.rgb / vec3<f32>(103.0, 71.0, 97.0));
    return (dither - vec3<f32>(0.5)) / 255.0;
}

// The combine's exit: dither in gamma space, where the present-encode rounds, then the frame's
// one decode, which the sRGB present-encode reverses byte-exactly; none under `GAMMA_OUT`.
fn combine_out(outg: vec3<f32>, alpha: f32, frag_coord: vec2<f32>) -> vec4<f32> {
    let dithered = clamp(
        outg + screen_space_dither(frag_coord) * step(0.5, ffx.lane.w),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
#ifdef GAMMA_OUT
    // The UI camera's ground pass stores premultiplied bytes like `ui_quad.wgsl`; the lane decodes.
    return vec4<f32>(dithered * alpha, alpha);
#else
    return vec4<f32>(srgb_to_linear(dithered), alpha);
#endif
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055,
        c * 12.92, c <= vec3<f32>(0.0031308));
}

// Fitted filmic curve, linear input/output. Exposure is intentional: the Plus look is darker.
fn filmic(c: vec3<f32>) -> vec3<f32> {
    let x = c * 0.85;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14),
        vec3<f32>(0.0), vec3<f32>(1.0));
}

fn combine_body(uv: vec2<f32>, frag_coord: vec2<f32>) -> vec4<f32> {
    let scene = textureSample(in_tex, in_samp, uv);
    let source = srgb_to_linear(max(scene.rgb, vec3<f32>(0.0)));
    let blur = max(textureSample(blur_tex, in_samp, uv).rgb, vec3<f32>(0.0));
    var colour = mix(source, blur, ffx.lane.z);
#ifdef PLUS_BLOOM
    // Soft bright-region extraction on the linear blur; bounded strength independent of zone
    // gamma glow. The stock zone weight retains its spatial character in the Plus bloom.
    colour += max(blur - vec3<f32>(0.45), vec3<f32>(0.0)) * (0.25 + 0.2 * ffx.lane.x);
#endif
#ifdef PLUS_TONE_MAPPING
    colour = filmic(colour);
#endif
#ifdef PLUS_COLOUR_GRADING
    let luma = dot(colour, vec3<f32>(0.2126, 0.7152, 0.0722));
    colour = mix(vec3<f32>(luma), colour, 0.88);
    // Cool shadows, slightly warm highlights, restrained saturation.
    colour *= mix(vec3<f32>(0.90, 0.96, 1.04), vec3<f32>(1.02, 0.99, 0.94), clamp(luma, 0.0, 1.0));
    colour *= 0.94;
#endif
    var gamma = linear_to_srgb(max(colour, vec3<f32>(0.0)));
    if (ffx.lane.y > 0.0) {
        // Preserve the player's death-state identity after the world-only Plus look.
        let luma = clamp(dot(gamma, vec3<f32>(0.299, 0.587, 0.144)), 0.0, 1.0);
        gamma = vec3<f32>(luma) + vec3<f32>(83.0, 147.0, 168.0) / 255.0 * clamp(4.0 * luma * (1.0 - luma), 0.0, 1.0);
    }
    return combine_out(gamma, scene.a, frag_coord);
}

@fragment
fn fs_combine(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return combine_body(in.uv, in.position.xy);
}

// FFXGlowWave, the warp the reference runs instead of FFXGlow while the eye is in any liquid
// (`0x6cc630`, `0x6cb310`): the same combine, both texcoords displaced by a 128×128 sine LUT. The
// reference's per-vertex wave texcoord (`0x7bca80`) is affine in the screen UV, so per fragment
// is exact: uv' = uv · R(10°) · S(W/128, 0.88·H/128) · T(p1, p2), fixed to screen pixels.
const WAVE_ROT_RAD: f32 = 0.174532925; // 10°, the reference's rotation row
const WAVE_LUT_EDGE: f32 = 128.0; // the LUT is 128×128 texels ([0xce89a0])
const WAVE_V_SCALE: f32 = 0.88; // [0xce89c0]: the v axis only; u's scale is 1.0
const WAVE_AMPLITUDE_PX: f32 = 3.0; // full-res pixels, both samples

// The warp's screen-UV offset in UV units; `screen` is the full-res scene size in pixels.
fn wave_offset(uv: vec2<f32>, screen: vec2<f32>) -> vec2<f32> {
    let c = cos(WAVE_ROT_RAD);
    let s = sin(WAVE_ROT_RAD);
    let scale = vec2<f32>(screen.x, WAVE_V_SCALE * screen.y) / WAVE_LUT_EDGE;
    let tex = vec2<f32>(c * uv.x - s * uv.y, s * uv.x + c * uv.y) * scale + ffx.wave.xy;
    // Sampled, not evaluated: the reference's sine is a filtered LUT. The sampler must repeat,
    // as the texcoord spans about 10 cycles; the unbias matches the shipped ps_2_0 shader.
    let d = (textureSample(wave_tex, wave_samp, tex).rg - vec2<f32>(0.5)) * 2.0;
    return d * WAVE_AMPLITUDE_PX / screen;
}

@fragment
fn fs_combine_wave(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let screen = vec2<f32>(textureDimensions(in_tex));
    return combine_body(in.uv + wave_offset(in.uv, screen), in.position.xy);
}
