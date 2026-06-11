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

struct SkyboxShaderHorizon {
    position: vec2<f32>,
    normal: vec2<f32>,
    sky_gradient_distance_px: f32,
    ground_gradient_distance_px: f32,
    seam_width_px: f32,
    _padding: f32,
};

struct SkyboxShaderUniform {
    sun_direction: vec4<f32>,
    moon_direction: vec4<f32>,

    sun_color: vec4<f32>,
    moon_color: vec4<f32>,
    ambient_color: vec4<f32>,

    params: SkyboxShaderParams,
    horizon: SkyboxShaderHorizon,
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

fn horizon_signed_distance_px(frag_coord: vec4<f32>) -> f32 {
    return dot(frag_coord.xy - sky.horizon.position, sky.horizon.normal);
}

fn remap_sky_up_from_horizon_z(dir_z: f32, horizon_z: f32) -> f32 {
    return saturate(
        (dir_z - horizon_z) / max(1.0 - horizon_z, 0.0001)
    );
}

fn remap_ground_down_from_horizon_z(dir_z: f32, horizon_z: f32) -> f32 {
    return saturate(
        (horizon_z - dir_z) / max(1.0 + horizon_z, 0.0001)
    );
}

fn elevation_blend(edge0: f32, edge1: f32, elevation_degrees: f32) -> f32 {
    return smoothstep(edge0, edge1, elevation_degrees);
}

const INV_4PI: f32 = 0.07957747154594767;
const RAYLEIGH_PHASE_SCALE: f32 = 0.05968310365946075;

fn rayleigh_phase(cos_theta: f32) -> f32 {
    return RAYLEIGH_PHASE_SCALE * (1.0 + cos_theta * cos_theta);
}

fn aerosol_phase(cos_theta: f32, anisotropy: f32) -> f32 {
    let g2 = anisotropy * anisotropy;
    let denom = max(1.0 + g2 - 2.0 * anisotropy * cos_theta, 0.0001);

    return INV_4PI * (1.0 - g2) / pow(denom, 1.5);
}

fn base_night_sky(dir: vec3<f32>, moon_dir: vec3<f32>, up: f32) -> vec3<f32> {
    let zenith = vec3<f32>(0.004, 0.006, 0.020);
    let horizon = sky.ambient_color.rgb * (0.08 + 0.35 * sky.ambient_color.a);

    var color = mix(horizon, zenith, pow(up, 0.7));

    // Moon-side lift.
    let moon_side = pow(saturate(dot(dir, moon_dir) * 0.5 + 0.5), 3.0);
    color += sky.moon_color.rgb * sky.moon_color.a * moon_side * 0.05;

    return color;
}

// Based on https://www.shadertoy.com/view/msXXDS
fn single_pass_physical_sky(dir: vec3<f32>, sun_dir: vec3<f32>, moon_dir: vec3<f32>, up: f32) -> vec3<f32> {
    let haze = saturate(sky.params.haze);
    let sun_elevation_sin = sin(sky.params.sun_elevation_degrees * 0.01745329252);
    let sun_cos_theta = clamp(dot(dir, sun_dir), -1.0, 1.0);

    // msXXDS precomputes transmittance in Buffer A and renders a sky-view LUT in
    // Buffer B. The Bevy skybox stays single-pass, so these are compact RGB
    // collision coefficients plus analytic air-mass approximations for the same
    // Rayleigh/aerosol/transmittance terms.
    let beta_rayleigh = vec3<f32>(5.80, 13.56, 33.10);
    let aerosol_turbidity = 0.35 + haze * 2.15;
    let beta_aerosol = vec3<f32>(20.0, 18.0, 16.0) * aerosol_turbidity;
    let beta_extinction = beta_rayleigh + beta_aerosol;

    // `up` is the existing horizon-anchored gradient coordinate. Using it here
    // keeps the atmospheric horizon attached to the MapLibre horizon line while
    // the angular phase term still follows the true view direction.
    let view_mu = max(pow(up, 0.9), 0.015);
    let view_air_mass = 1.0 / (view_mu + 0.065);
    let sun_air_mass = 1.0 / (max(sun_elevation_sin, 0.0) + 0.085);

    let view_transmittance = exp(-beta_extinction * (view_air_mass * 0.0065));
    let scatter_fraction = vec3<f32>(1.0) - view_transmittance;
    let sun_transmittance =
        exp(-(beta_rayleigh * 0.0040 + beta_aerosol * 0.0030) * sun_air_mass);

    let rayleigh = beta_rayleigh * rayleigh_phase(sun_cos_theta);
    let aerosol = beta_aerosol
        * aerosol_phase(sun_cos_theta, mix(0.86, 0.92, haze))
        * 0.05;
    let single_scattering = (rayleigh + aerosol) / max(beta_extinction, vec3<f32>(0.0001));

    // Keep twilight energy when the CPU-side direct sun intensity has already
    // reached zero below the horizon.
    let twilight_energy = smoothstep(-0.32, 0.05, sun_elevation_sin) * 0.42;
    let daylight_energy = max(sky.sun_color.a, twilight_energy);
    let solar_energy = daylight_energy * (0.9 + 1.4 * saturate(sun_elevation_sin + 0.15));
    let solar_color = sky.sun_color.rgb * solar_energy * sun_transmittance;

    var color = scatter_fraction * single_scattering * solar_color * 18.0;

    // A small multiple-scattering style lift prevents the horizon from becoming
    // too contrasty without adding the extra Shadertoy LUT pass.
    color += scatter_fraction
        * mix(vec3<f32>(0.30, 0.38, 0.48), vec3<f32>(0.72, 0.74, 0.74), haze)
        * daylight_energy
        * (0.10 + 0.30 * (1.0 - up));

    let horizon_amount = pow(1.0 - up, 2.3);
    let low_sun = (1.0 - smoothstep(0.08, 0.45, sun_elevation_sin))
        * smoothstep(-0.28, 0.05, sun_elevation_sin);
    // Keep this centered on the physical sun direction. The earlier horizontal
    // projection made the warm glow follow the sun azimuth but not the actual
    // sun elevation, which could make the apparent center drift off `sun_dir`.
    let sun_side = pow(saturate(sun_cos_theta), 32.0);

    color += sky.sun_color.rgb
        * vec3<f32>(1.30, 0.55, 0.25)
        * sun_side
        * horizon_amount
        * low_sun
        * 0.12;

    color = mix(
        color,
        sky.ambient_color.rgb * (0.15 + 0.35 * sky.ambient_color.a),
        0.08 * sky.ambient_color.a,
    );

    let night_color = base_night_sky(dir, moon_dir, up);
    let daylight_blend = elevation_blend(-18.0, -4.0, sky.params.sun_elevation_degrees);

    return mix(night_color, color, daylight_blend);
}

fn ground_sky(below: f32) -> vec3<f32> {
    let horizon_ground = sky.ambient_color.rgb * (0.18 + 0.40 * sky.ambient_color.a);
    let nadir_ground = sky.ambient_color.rgb * (0.025 + 0.12 * sky.ambient_color.a);

    return mix(horizon_ground, nadir_ground, pow(below, 0.55));
}

fn add_sun(dir: vec3<f32>, is_up: f32, sun_dir: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let sun_amount = max(dot(dir, sun_dir), 0.0);
    let sun_visible = smoothstep(-0.01, 0.03, sky.params.sun_elevation_degrees * 0.01745329252)
        * is_up;

    let broad_glow = pow(sun_amount, 180.0) * 0.012;
    let tight_glow = pow(sun_amount, 2048.0) * 0.12;
    let disc = smoothstep(0.99988, 1.0, sun_amount);

    var result = color;

    result += sky.sun_color.rgb * sky.sun_color.a * broad_glow * sun_visible;
    result += sky.sun_color.rgb * sky.sun_color.a * tight_glow * sun_visible;

    result = mix(
        result,
        sky.sun_color.rgb * 1.25,
        disc * sky.sun_color.a * sun_visible,
    );

    return result;
}

fn add_moon(dir: vec3<f32>, is_up: f32, moon_dir: vec3<f32>, color: vec3<f32>) -> vec3<f32> {
    let moon_amount = max(dot(dir, moon_dir), 0.0);

    let moon_visible = select(0.0, 1.0, sky.params.moon_elevation_degrees > 0.0);

    let glow = pow(moon_amount, 62.0) * 0.08;
    let disc = smoothstep(0.9997, 1.0, moon_amount);

    var result = color;

    result += sky.moon_color.rgb
        * sky.moon_color.a
        * glow
        * moon_visible
        * is_up;

    result = mix(
        result,
        sky.moon_color.rgb * 1.15,
        disc * sky.moon_color.a * moon_visible * is_up,
    );

    return result;
}

@fragment
fn fragment(in: SkyboxVertexOutput) -> @location(0) vec4<f32> {
    let dir = sky_view_direction_from_frag_coord(in.clip_position);

    let horizon_distance_px = horizon_signed_distance_px(in.clip_position);
    let horizon_frag_xy =
        in.clip_position.xy - sky.horizon.normal * horizon_distance_px;
    let horizon_dir = sky_view_direction_from_frag_coord(
        vec4<f32>(horizon_frag_xy, in.clip_position.zw)
    );
    let horizon_z = clamp(horizon_dir.z, -0.9999, 0.9999);

    let is_up = select(0.0, 1.0, horizon_distance_px >= 0.0);

    let up = remap_sky_up_from_horizon_z(dir.z, horizon_z);
    let below = remap_ground_down_from_horizon_z(dir.z, horizon_z);

    let sun_dir = normalize(sky.sun_direction.xyz);
    let moon_dir = normalize(sky.moon_direction.xyz);

    let sun_elevation = sky.params.sun_elevation_degrees;

    let day_factor = elevation_blend(6.0, 25.0, sun_elevation);

    var upper_color = single_pass_physical_sky(dir, sun_dir, moon_dir, up);
//    upper_color = add_sun(dir, is_up, sun_dir, upper_color);
    upper_color = add_moon(dir, is_up, moon_dir, upper_color);

    let lower_color = ground_sky(below);
    var final_color = select(lower_color, upper_color, is_up > 0.0);

    // Soft horizon seam.
    let horizon_line = 1.0 - smoothstep(
        0.0,
        max(sky.horizon.seam_width_px, 0.0001),
        abs(horizon_distance_px),
    );

    let horizon_color = mix(
        sky.ambient_color.rgb,
        single_pass_physical_sky(horizon_dir, sun_dir, moon_dir, 0.0),
        saturate(day_factor),
    );

    final_color = mix(final_color, horizon_color, horizon_line * 0.20);

    final_color *= sky.params.exposure;

    return vec4<f32>(max(final_color, vec3<f32>(0.0)), 1.0);
}
