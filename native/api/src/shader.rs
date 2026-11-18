//! The gap's shader, embedded: pipef0x's Yukari Gap Demo `fog.vsh` and `fog.fsh` (GLSL 450),
//! translated line for line into WGSL. MIT licensed, Copyright (c) 2026 pipef0x:
//! <https://github.com/pipef0x/Yukari-Gap-Demo>.
//!
//! Kept here, as Rust source, so the native mod carries it: for drawing the gap live instead of
//! from baked frames once Horizon lets mods bring shaders.

/// The gap shader's WGSL source: a vertex and a fragment entry point, its uniforms at bindings
/// 0 to 2 of group 0.
pub const GAP_WGSL: &str = r#"// Yukari Gap shader in WGSL: pipef0x's Yukari Gap Demo `fog.vsh` and `fog.fsh` (GLSL 450),
// translated line for line. MIT licensed, Copyright (c) 2026 pipef0x:
// https://github.com/pipef0x/Yukari-Gap-Demo
//
// The gap is the space between two cubic Bezier curves that breathe apart and back every
// 8 / ANIM_SPEED days of game time, with a black rim, eight eyes bouncing about inside it and
// stepped simplex fog behind them, all snapped to a 128 pixel grid.
//
// Bindings follow the demo's uniform blocks (std140 and WGSL's uniform layout agree on them):
//   @group(0) @binding(0) DynamicTransforms   @binding(1) Projection   @binding(2) Globals
// Vertex inputs: location 0 the position, location 1 the texture coordinate (UV0), 0..1 over
// the quad.
//
// Translation notes:
// - GLSL `mod` floors and WGSL `%` truncates, so `glsl_mod` stands in for `mod`.
// - `inout` / `out` parameters became return values (`CubicRoots`, `EyeHit`).
// - Minecraft's `GameTime` (from globals.glsl) is the day fraction, 0..1 over 24000 ticks.

struct DynamicTransforms {
    model_view_mat: mat4x4<f32>,
    color_modulator: vec4<f32>,
    model_offset: vec3<f32>,
    texture_mat: mat4x4<f32>,
    line_width: f32,
}

struct Projection {
    proj_mat: mat4x4<f32>,
}

struct Globals {
    game_time: f32,
}

@group(0) @binding(0) var<uniform> transforms: DynamicTransforms;
@group(0) @binding(1) var<uniform> projection: Projection;
@group(0) @binding(2) var<uniform> globals: Globals;

// ---- Vertex (fog.vsh) ----

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) uv0: vec2<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coord0: vec2<f32>,
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = projection.proj_mat * transforms.model_view_mat * vec4<f32>(in.position, 1.0);
    out.tex_coord0 = in.uv0;
    return out;
}

// ---- Fragment (fog.fsh) ----

const ANIM_SPEED: f32 = 1200.0;

const PI: f32 = 3.14159265358979323846;
const TWO_PI: f32 = 6.28318530717958647692;

/// GLSL's `mod`: x - y * floor(x / y).
fn glsl_mod(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn glsl_mod2(x: vec2<f32>, y: f32) -> vec2<f32> {
    return x - y * floor(x / y);
}

/// The real roots of a monic cubic, and how many there are (1 or 3).
struct CubicRoots {
    count: i32,
    roots: vec3<f32>,
}

// Solves the monic cubic equation: x^3 + coeffs.z * x^2 + coeffs.y * x + coeffs.x = 0.
// `roots` comes in as the roots to keep where none is found.
fn solve_cubic(coeffs: vec3<f32>, roots_in: vec3<f32>) -> CubicRoots {
    var roots = roots_in;
    let a = coeffs.z;
    let b = coeffs.y;
    let c = coeffs.x;

    // 1. Depress the cubic: substitute x = t - a/3 to eliminate the quadratic term.
    // Resulting equation: t^3 + p*t + q = 0
    let a_over_3 = a / 3.0;
    let a2 = a * a;
    let p = b - (a2 / 3.0);
    let q = (2.0 * a2 * a - 9.0 * a * b + 27.0 * c) / 27.0;

    let p_over_3 = p / 3.0;
    let q_over_2 = q / 2.0;

    // 2. Compute the discriminant: D = (q/2)^2 + (p/3)^3
    let discriminant = q_over_2 * q_over_2 + p_over_3 * p_over_3 * p_over_3;

    if (0.0 < discriminant) {
        // Case 1: One real root (and two complex conjugate roots).
        let sqrt_d = sqrt(discriminant);
        let u = -q_over_2 + sqrt_d;
        let v = -q_over_2 - sqrt_d;

        // Safe cube root for negative numbers.
        let cbrt_u = sign(u) * pow(abs(u), 1.0 / 3.0);
        let cbrt_v = sign(v) * pow(abs(v), 1.0 / 3.0);

        roots.x = (cbrt_u + cbrt_v) - a_over_3;
        return CubicRoots(1, roots);
    } else if (discriminant < 0.0) {
        // Case 2: Three distinct real roots (casus irreducibilis), by trigonometric
        // substitution to avoid complex arithmetic.
        let r = sqrt(-p_over_3 * p_over_3 * p_over_3);
        let phi = acos(clamp(-q_over_2 / r, -1.0, 1.0));
        let two_sqrt_minus_p3 = 2.0 * sqrt(-p_over_3);

        roots.x = two_sqrt_minus_p3 * cos((phi + 0.0 * TWO_PI) / 3.0) - a_over_3;
        roots.y = two_sqrt_minus_p3 * cos((phi + 1.0 * TWO_PI) / 3.0) - a_over_3;
        roots.z = two_sqrt_minus_p3 * cos((phi + 2.0 * TWO_PI) / 3.0) - a_over_3;
        return CubicRoots(3, roots);
    } else {
        // Case 3: Discriminant == 0 (multiple real roots).
        if (abs(q_over_2) < 1e-6) {
            // Triple root at t = 0.
            roots = vec3<f32>(-a_over_3);
            return CubicRoots(3, roots);
        } else {
            // One single root and one double root.
            let u = sign(-q_over_2) * pow(abs(q_over_2), 1.0 / 3.0);

            roots.x = 2.0 * u - a_over_3;
            roots.y = -u - a_over_3;
            roots.z = roots.y;
            return CubicRoots(3, roots);
        }
    }
}

// Evaluates the point on a cubic Bezier at t in [0, 1].
fn eval_cubic_bezier(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let u = 1.0 - t;
    return (u * u * u) * p0
         + (3.0 * u * u * t) * p1
         + (3.0 * u * t * t) * p2
         + (t * t * t) * p3;
}

// Evaluates the first derivative B'(t), for Newton-Raphson refinement.
fn eval_cubic_bezier_deriv(t: f32, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> vec2<f32> {
    let u = 1.0 - t;
    return 3.0 * u * u * (p1 - p0)
         + 6.0 * u * t * (p2 - p1)
         + 3.0 * t * t * (p3 - p2);
}

// Unsigned Euclidean distance from uv to the cubic Bezier segment [0, 1].
fn cubic_bezier_unsigned_dist(uv: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    // 1. Coarse search over 12 segments to find the best initial t.
    const STEPS: i32 = 12;

    var best_t = 0.0;
    var min_dist_sq = 1e38;

    for (var idx = 0; idx <= STEPS; idx++) {
        let t = f32(idx) / f32(STEPS);
        let diff = eval_cubic_bezier(t, p0, p1, p2, p3) - uv;
        let diff_sq = dot(diff, diff);

        if (diff_sq < min_dist_sq) {
            min_dist_sq = diff_sq;
            best_t = t;
        }
    }

    // 2. Two Gauss-Newton steps for exact curve projection.
    for (var j = 0; j < 2; j++) {
        let pos = eval_cubic_bezier(best_t, p0, p1, p2, p3) - uv;
        let deriv = eval_cubic_bezier_deriv(best_t, p0, p1, p2, p3);
        let denom = dot(deriv, deriv);

        if (1e-7 < denom) {
            best_t = clamp(best_t - dot(pos, deriv) / denom, 0.0, 1.0);
        }
    }

    return length(eval_cubic_bezier(best_t, p0, p1, p2, p3) - uv);
}

// Cubic Bezier SDF: the signed Euclidean distance to the Bezier curve.
fn cubic_bezier_sdf(uv: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>) -> f32 {
    let cu = -p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y;
    let qu = 3.0 * p0.y - 6.0 * p1.y + 3.0 * p2.y;
    let li = -3.0 * p0.y + 3.0 * p1.y;
    let co = p0.y - uv.y;

    let solved = solve_cubic(vec3<f32>(co / cu, li / cu, qu / cu), vec3<f32>(1e38));
    var roots = solved.roots;
    let root_count = solved.count;

    var n_ints = 0;

    for (var i = 0; i < 3; i++) {
        if (i < root_count) {
            let root = roots[i];
            if ((0.0 <= root) && (root <= 1.0)) {
                var x_pos = -p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x;
                x_pos = x_pos * root + 3.0 * p0.x - 6.0 * p1.x + 3.0 * p2.x;
                x_pos = x_pos * root + -3.0 * p0.x + 3.0 * p1.x;
                x_pos = x_pos * root + p0.x;

                if (x_pos < uv.x) {
                    n_ints++;
                }
            }
        }
    }

    let tang1 = p0 - p1;
    let tang2 = p2 - p3;

    let nor1 = vec2<f32>(tang1.y, -tang1.x);
    let nor2 = vec2<f32>(tang2.y, -tang2.x);

    if (p0.y < p1.y) {
        if ((uv.y <= p0.y) && (dot(uv - p0, nor1) < 0.0)) {
            n_ints++;
        }
    } else {
        if (!(uv.y <= p0.y) && !(dot(uv - p0, nor1) < 0.0)) {
            n_ints++;
        }
    }

    if (p2.y < p3.y) {
        if (!(uv.y <= p3.y) && (dot(uv - p3, nor2) < 0.0)) {
            n_ints++;
        }
    } else {
        if ((uv.y <= p3.y) && !(dot(uv - p3, nor2) < 0.0)) {
            n_ints++;
        }
    }

    let sgn = select(-1.0, 1.0, n_ints % 2 == 0);
    let dist = cubic_bezier_unsigned_dist(uv, p0, p1, p2, p3);

    return sgn * dist;
}

// ---- Fog ----

fn random3(c: vec3<f32>) -> vec3<f32> {
    let j = 4096.0 * sin(dot(c, vec3<f32>(17.0, 59.4, 15.0)));

    let r = vec3<f32>(
        fract(j * 32.0),
        fract(j * 4.0),
        fract(j * 512.0));

    return r - 0.5;
}

fn simplex3d(p: vec3<f32>) -> f32 {
    const INV_3: f32 = 1.0 / 3.0;
    const INV_6: f32 = 1.0 / 6.0;

    let s = floor(p + dot(p, vec3<f32>(INV_3)));
    let x = p - s + dot(s, vec3<f32>(INV_6));

    let e = step(vec3<f32>(0.0), x - x.yzx);
    let i1 = e * (1.0 - e.zxy);
    let i2 = 1.0 - e.zxy * (1.0 - e);

    let x1 = x - i1 + INV_6;
    let x2 = x - i2 + INV_6 * 2.0;
    let x3 = x - 1.0 + INV_6 * 3.0;

    var w = vec4<f32>(dot(x, x), dot(x1, x1), dot(x2, x2), dot(x3, x3));
    w = max(0.6 - w, vec4<f32>(0.0));
    w = w * w;
    w = w * w;

    var d = vec4<f32>(
        dot(random3(s + 0.0), x),
        dot(random3(s + i1), x1),
        dot(random3(s + i2), x2),
        dot(random3(s + 1.0), x3));
    d = d * w;

    return dot(d, vec4<f32>(52.0));
}

fn fog(uv: vec2<f32>) -> vec4<f32> {
    const COLOR_A: vec4<f32> = vec4<f32>(92.0, 0.0, 154.0, 255.0) / 255.0;
    const COLOR_B: vec4<f32> = vec4<f32>(255.0, 0.0, 66.0, 255.0) / 255.0;
    const FORWARD: f32 = 0.5;
    const LEVELS: f32 = 4.0;

    let t = globals.game_time * ANIM_SPEED;
    let c = uv;
    let p3 = vec3<f32>(c / 4.0, t / 512.0);

    var value = simplex3d(p3 * 32.0);
    value = floor(value * LEVELS) / LEVELS;
    value = value + FORWARD;

    let albedo = mix(COLOR_A, COLOR_B, value).xyz;

    return vec4<f32>(albedo, 1.0);
}

// ---- Eyes ----

const EYES_COUNT: i32 = 8;
const EYE_SPEED: f32 = 0.035;      // Drift speed in UV units per second (slow and eerie)
const EYE_ARC_RADIUS: f32 = 0.095; // Radius of the two intersecting circles
const EYE_ARC_OFFSET: f32 = 0.058; // Vertical shift of the two circles (how open the eye is)
const IRIS_RADIUS: f32 = 0.034;    // Red iris radius
const PUPIL_RADIUS: f32 = IRIS_RADIUS * 0.5; // Black pupil: half the iris
const EYE_OUTLINE: f32 = 0.000;    // 0.004 for a thin dark eyelid outline round the sclera

// Half-width and half-height of the vesica piscis sclera, for exact wall collisions.
const EYE_HALF_W: f32 = sqrt(EYE_ARC_RADIUS * EYE_ARC_RADIUS - EYE_ARC_OFFSET * EYE_ARC_OFFSET);
const EYE_HALF_H: f32 = EYE_ARC_RADIUS - EYE_ARC_OFFSET;

const ALBEDO_SCLERA: vec3<f32> = vec3<f32>(1.0, 1.0, 1.0);
const ALBEDO_IRIS: vec3<f32> = vec3<f32>(0.92, 0.06, 0.18);
const ALBEDO_PUPIL: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);

// Gives each eye a unique deterministic direction and a huge time offset.
fn eye_hash2(idx: i32) -> vec2<f32> {
    let fi = f32(idx) + 1.0;
    return fract(sin(vec2<f32>(fi * 127.1, fi * 311.7)) * 43758.5453);
}

// Triangle-wave reflection on [0, 1], at 0.5 when u = 0.5.
fn triangle01(u: f32) -> f32 {
    return 1.0 - abs(glsl_mod(u, 2.0) - 1.0);
}

// The exact elastically bouncing position of eye `idx` at `time_input` seconds.
fn get_eye_position(idx: i32, time_input: f32) -> vec2<f32> {
    let h = eye_hash2(idx);

    // A launch angle of its own for each eye (never purely horizontal or vertical).
    let angle = (h.x * 6.2831853) + 0.37;
    let vel = vec2<f32>(cos(angle), sin(angle)) * EYE_SPEED;

    // A large deterministic time offset, between 10 minutes and 3 hours in seconds.
    let time_offset = 600.0 + h.y * 10200.0;
    let t = time_input + time_offset;

    // The travel spans inside [0, 1], allowing for the eye's half-width and half-height.
    let min_bounds = vec2<f32>(EYE_HALF_W, EYE_HALF_H);
    let span = max(vec2<f32>(1.0) - 2.0 * min_bounds, vec2<f32>(0.001));

    // Wrap the phase modulo 2 first so huge offsets never lose float precision.
    let phase = glsl_mod2(0.5 + (vel * t) / span, 2.0);

    return min_bounds + span * vec2<f32>(triangle01(phase.x), triangle01(phase.y));
}

/// Whether a pixel is inside an eye's sclera, and its colour there.
struct EyeHit {
    hit: bool,
    albedo: vec3<f32>,
}

// Evaluates the eye at `eye_position` at `uv`.
fn eval_eye(uv: vec2<f32>, eye_position: vec2<f32>) -> EyeHit {
    let p = uv - eye_position;

    // Quick bounding-box reject before any square roots.
    if (EYE_HALF_W < abs(p.x) || EYE_HALF_H < abs(p.y)) {
        return EyeHit(false, vec3<f32>(0.0));
    }

    // Sclera: the intersection of two vertically offset circles.
    let d_top = length(p - vec2<f32>(0.0, -EYE_ARC_OFFSET)) - EYE_ARC_RADIUS;
    let d_bottom = length(p - vec2<f32>(0.0, EYE_ARC_OFFSET)) - EYE_ARC_RADIUS;
    let sclera_sdf = max(d_top, d_bottom);

    if (0.0 < sclera_sdf) {
        return EyeHit(false, vec3<f32>(0.0));
    }

    // Thin dark rim round the eyelid edge (EYE_OUTLINE = 0 turns it off).
    if (-EYE_OUTLINE < sclera_sdf) {
        return EyeHit(true, ALBEDO_PUPIL);
    }

    // Concentric iris and pupil circles, clipped inside the sclera.
    let r = length(p);
    if (r <= PUPIL_RADIUS) {
        return EyeHit(true, ALBEDO_PUPIL);
    } else if (r <= IRIS_RADIUS) {
        return EyeHit(true, ALBEDO_IRIS);
    }
    return EyeHit(true, ALBEDO_SCLERA);
}

// ---- Main ----

@fragment
fn fs_main(@location(0) tex_coord0: vec2<f32>) -> @location(0) vec4<f32> {
    const PIXEL_RES: f32 = 128.0;
    const OUTLINE_WIDTH: f32 = 0.0075;

    // Snap the UVs to the pixel grid.
    let uv = (floor(clamp(tex_coord0, vec2<f32>(0.0), vec2<f32>(0.9999)) * PIXEL_RES) + 0.5) / PIXEL_RES;

    var bezier_t = globals.game_time * ANIM_SPEED / 8.0; // The point in time
    bezier_t = abs(fract(bezier_t) * 2.0 - 1.0);         // A triangle wave
    bezier_t = smoothstep(0.0, 1.0, bezier_t);           // Eased

    let bezier_p0 = vec2<f32>(0.125, 0.125);

    let bezier_p1 = vec2<f32>(0.875, 0.125);
    let bezier_p2 = vec2<f32>(0.125, 0.875);

    let bezier_p1_delta = mix(vec2<f32>(0.0, 0.0), vec2<f32>(0.0, 0.0), bezier_t);
    let bezier_p2_delta = mix(vec2<f32>(0.15, 0.0), vec2<f32>(0.3, 0.0), bezier_t);

    let bezier_p3 = vec2<f32>(0.875, 0.875);

    let sdf_alpha = cubic_bezier_sdf(
        uv,
        bezier_p0,
        bezier_p1 + bezier_p1_delta,
        bezier_p2 + bezier_p2_delta,
        bezier_p3);

    let sdf_beta = cubic_bezier_sdf(
        uv,
        bezier_p0,
        bezier_p1 - bezier_p1_delta,
        bezier_p2 - bezier_p2_delta,
        bezier_p3);

    let is_inside_gap = sdf_alpha * sdf_beta < 0.0;
    let edge_dist = min(abs(sdf_alpha), abs(sdf_beta));

    if (!is_inside_gap && OUTLINE_WIDTH < edge_dist) {
        discard;
    }

    // The black outer border always draws over the eyes and fog inside.
    if (edge_dist <= OUTLINE_WIDTH) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Eyes 0 .. EYES_COUNT - 1; lower indices draw over higher ones.
    let eye_t = globals.game_time * ANIM_SPEED;

    for (var idx = 0; idx < EYES_COUNT; idx++) {
        let eye = eval_eye(uv, get_eye_position(idx, eye_t));
        if (eye.hit) {
            return vec4<f32>(eye.albedo, 1.0);
        }
    }

    // No eye here: the simplex fog.
    return fog(uv);
}
"#;

#[cfg(test)]
mod tests {
    use super::GAP_WGSL;

    #[test]
    fn gap_shader_has_its_entry_points_and_bindings() {
        assert!(GAP_WGSL.contains("@vertex"));
        assert!(GAP_WGSL.contains("@fragment"));
        for binding in 0..3 {
            assert!(GAP_WGSL.contains(&format!("@group(0) @binding({binding})")));
        }
    }
}
