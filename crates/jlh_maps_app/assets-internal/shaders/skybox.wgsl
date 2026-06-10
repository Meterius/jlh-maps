#import bevy_pbr::{
    view_transformations::{
        frag_coord_to_ndc,
        position_ndc_to_world,
    },
    forward_io::Vertex,
}

struct SkyboxShaderParams {
    sun_elevation_degrees: f32,
    moon_elevation_degrees: f32,
    haze: f32,
    exposure: f32,
};

struct SkyboxShaderUniform {
    sun_direction: vec4<f32>,
    moon_direction: vec4<f32>,

    sun_color: vec4<f32>,
    moon_color: vec4<f32>,
    ambient_color: vec4<f32>,

    params: SkyboxShaderParams,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> sky: SkyboxShaderUniform;

struct SkyboxVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> SkyboxVertexOutput {
    var out: SkyboxVertexOutput;

    // Fullscreen triangle carrier.
    //
    // The mesh should contain:
    //   (-1, -1)
    //   ( 3, -1)
    //   (-1,  3)
    //
    // These are already clip-space x/y coordinates.
    //
    // For Bevy's reversed-Z convention:
    //   z = 0.0 is far
    //   z = 1.0 is near
    out.clip_position = vec4<f32>(
        vertex.position.xy,
        0.0,
        1.0,
    );

    return out;
}

fn saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn sky_view_direction_from_frag_coord(frag_coord: vec4<f32>) -> vec3<f32> {
    let ndc = frag_coord_to_ndc(frag_coord);

    // Bevy/WGPU reversed-depth convention:
    // z = 1.0 is near, z = 0.0 is far for perspective projection.
    let near_world = position_ndc_to_world(vec3<f32>(ndc.xy, 1.0));
    let far_world = position_ndc_to_world(vec3<f32>(ndc.xy, 0.0));

    return normalize(far_world - near_world);
}

fn elevation_blend(edge0: f32, edge1: f32, elevation_degrees: f32) -> f32 {
    return smoothstep(edge0, edge1, elevation_degrees);
}

fn base_day_sky(dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let up = saturate(dir.z);
    let horizon_amount = pow(1.0 - up, 2.0);

    let clear_zenith = vec3<f32>(0.10, 0.28, 0.62);
    let clear_horizon = vec3<f32>(0.58, 0.72, 0.95);

    let hazy_zenith = vec3<f32>(0.32, 0.48, 0.72);
    let hazy_horizon = vec3<f32>(0.78, 0.82, 0.86);

    let haze = saturate(sky.params.haze);

    let zenith = mix(clear_zenith, hazy_zenith, haze);
    let horizon = mix(clear_horizon, hazy_horizon, haze);

    var color = mix(horizon, zenith, pow(up, 0.65));

    // Slight brightening toward the sun side of the sky.
    let sun_side = pow(saturate(dot(dir, sun_dir) * 0.5 + 0.5), 2.0);
    color += sky.sun_color.rgb * sky.sun_color.a * sun_side * 0.08;

    // Haze brightens the horizon.
    color = mix(color, horizon, horizon_amount * haze * 0.35);

    return color;
}

fn base_sunrise_sky(dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let up = saturate(dir.z);
    let horizon_amount = pow(1.0 - up, 1.7);

    let twilight_zenith = vec3<f32>(0.13, 0.12, 0.28);
    let sunrise_horizon = vec3<f32>(1.00, 0.42, 0.18);
    let upper_blue = vec3<f32>(0.28, 0.38, 0.70);

    var color = mix(sunrise_horizon, upper_blue, pow(up, 0.75));

    // Push the horizon warm near the sun direction.
    let sun_side = pow(saturate(dot(dir, sun_dir) * 0.5 + 0.5), 3.0);
    color = mix(color, sunrise_horizon * 1.15, sun_side * horizon_amount * 0.65);

    // Add twilight purple in the upper sky.
    color = mix(color, twilight_zenith, pow(up, 1.8) * 0.25);

    return color;
}

fn base_twilight_sky(dir: vec3<f32>, sun_dir: vec3<f32>) -> vec3<f32> {
    let up = saturate(dir.z);
    let horizon_amount = pow(1.0 - up, 2.2);

    let zenith = vec3<f32>(0.025, 0.030, 0.090);
    let horizon = vec3<f32>(0.30, 0.18, 0.34);
    let sun_horizon = vec3<f32>(0.90, 0.30, 0.18);

    var color = mix(horizon, zenith, pow(up, 0.8));

    let sun_side = pow(saturate(dot(dir, sun_dir) * 0.5 + 0.5), 4.0);
    color = mix(color, sun_horizon, sun_side * horizon_amount * 0.5);

    return color;
}

fn base_night_sky(dir: vec3<f32>, moon_dir: vec3<f32>) -> vec3<f32> {
    let up = saturate(dir.z);

    let zenith = vec3<f32>(0.004, 0.006, 0.020);
    let horizon = sky.ambient_color.rgb * (0.08 + 0.35 * sky.ambient_color.a);

    var color = mix(horizon, zenith, pow(up, 0.7));

    // Moon-side lift.
    let moon_side = pow(saturate(dot(dir, moon_dir) * 0.5 + 0.5), 3.0);
    color += sky.moon_color.rgb * sky.moon_color.a * moon_side * 0.05;

    return color;
}

fn ground_sky(dir: vec3<f32>) -> vec3<f32> {
    let below = saturate(-dir.z);

    let horizon_ground = sky.ambient_color.rgb * (0.18 + 0.40 * sky.ambient_color.a);
    let nadir_ground = sky.ambient_color.rgb * (0.025 + 0.12 * sky.ambient_color.a);

    return mix(horizon_ground, nadir_ground, pow(below, 0.55));
}

fn add_sun(dir: vec3<f32>, sun_dir: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let sun_amount = max(dot(dir, sun_dir), 0.0);

    let broad_glow = pow(sun_amount, 24.0) * 0.035;
    let tight_glow = pow(sun_amount, 256.0) * 0.25;
    let disc = smoothstep(0.9995, 1.0, sun_amount);

    var result = color;

    result += sky.sun_color.rgb * sky.sun_color.a * broad_glow;
    result += sky.sun_color.rgb * sky.sun_color.a * tight_glow;

    result = mix(
        result,
        sky.sun_color.rgb * 1.25,
        disc * sky.sun_color.a,
    );

    return result;
}

fn add_moon(dir: vec3<f32>, moon_dir: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let moon_amount = max(dot(dir, moon_dir), 0.0);

    let moon_visible = select(0.0, 1.0, sky.params.moon_elevation_degrees > 0.0);
    let upper_sky = select(0.0, 1.0, dir.z >= 0.0);

    let glow = pow(moon_amount, 62.0) * 0.08;
    let disc = smoothstep(0.9997, 1.0, moon_amount);

    var result = color;

    result += sky.moon_color.rgb
        * sky.moon_color.a
        * glow
        * moon_visible
        * upper_sky;

    result = mix(
        result,
        sky.moon_color.rgb * 1.15,
        disc * sky.moon_color.a * moon_visible * upper_sky,
    );

    return result;
}

@fragment
fn fragment(in: SkyboxVertexOutput) -> @location(0) vec4<f32> {
    let dir = sky_view_direction_from_frag_coord(in.clip_position);

    let sun_dir = normalize(sky.sun_direction.xyz);
    let moon_dir = normalize(sky.moon_direction.xyz);

    let sun_elevation = sky.params.sun_elevation_degrees;

    // Elevation phase weights:
    //
    // night:     below roughly -12 degrees
    // twilight:  -12 to -2
    // sunrise:   -2 to +8
    // day:       +8 and above, fully day around +25
    let twilight_factor = elevation_blend(-12.0, -2.0, sun_elevation);
    let sunrise_factor = elevation_blend(-2.0, 8.0, sun_elevation);
    let day_factor = elevation_blend(6.0, 25.0, sun_elevation);

    let night = base_night_sky(dir, moon_dir);
    let twilight = base_twilight_sky(dir, sun_dir);
    let sunrise = base_sunrise_sky(dir, sun_dir);
    let day = base_day_sky(dir, sun_dir);

    var upper_color = night;

    upper_color = mix(upper_color, twilight, twilight_factor);
    upper_color = mix(upper_color, sunrise, sunrise_factor);
    upper_color = mix(upper_color, day, day_factor);

    // Subtle pull toward the scene ambient color so sky and lighting stay aligned.
    upper_color = mix(
        upper_color,
        sky.ambient_color.rgb,
        0.12 * sky.ambient_color.a,
    );

    upper_color = add_sun(dir, sun_dir, upper_color);
    upper_color = add_moon(dir, moon_dir, upper_color);

    let lower_color = ground_sky(dir);
    var final_color = select(lower_color, upper_color, dir.z >= 0.0);

    // Soft horizon seam.
    let horizon_line = 1.0 - smoothstep(0.0, 0.020, abs(dir.z));
    let horizon_color = mix(
        sky.ambient_color.rgb,
        vec3<f32>(0.80, 0.86, 0.96),
        saturate(day_factor),
    );

    final_color = mix(final_color, horizon_color, horizon_line * 0.20);

    final_color *= sky.params.exposure;

    return vec4<f32>(max(final_color, vec3<f32>(0.0)), 1.0);
}