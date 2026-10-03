struct Uniforms { surface_size: vec2<f32>, _pad: vec2<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct BandOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) shape: vec2<f32>,
    @location(4) notch: vec2<f32>,
};

@vertex
fn vs(
    @builtin(vertex_index) vi: u32,
    @location(0) offset: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
) -> BandOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vi];
    let px = offset + corner * size;
    var out: BandOut;
    out.pos = vec4<f32>(
        px.x / u.surface_size.x * 2.0 - 1.0,
        1.0 - px.y / u.surface_size.y * 2.0,
        0.0,
        1.0,
    );
    out.local = corner * size;
    out.size = size;
    out.color = color;
    out.shape = uv_min;
    out.notch = uv_max;
    return out;
}

// The signed distance from `p` to a rectangle `size` big at the origin,
// its corners rounded by `r`.
fn rounded(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let half = size * 0.5;
    let rr = min(r, min(half.x, half.y));
    let q = abs(p - half) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

fn lin_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs(in: BandOut) -> @location(0) vec4<f32> {
    var d = rounded(in.local, in.size, in.shape.x);
    if (in.notch.x > 0.0) {
        let upper = rounded(in.local, vec2<f32>(in.size.x, in.notch.y), in.shape.x);
        let lower = rounded(in.local, vec2<f32>(in.notch.x, in.size.y), in.shape.x);
        d = min(upper, lower);
    }
    var cov = clamp(0.5 - d, 0.0, 1.0);
    if (in.shape.y > 0.0) {
        cov = cov - clamp(0.5 - (d + in.shape.y), 0.0, 1.0);
    }
    let a = cov * in.color.a;
    return vec4<f32>(lin_to_srgb(in.color.rgb) * a, a);
}
