#import bevy_pbr::{
    view_transformations::{
        frag_coord_to_ndc,
        position_ndc_to_world,
    },
    forward_io::Vertex,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0)
var<uniform> sun_direction: vec4<f32>;

@group(#{MATERIAL_BIND_GROUP}) @binding(1)
var<uniform> moon_direction: vec4<f32>;

struct SkyboxVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> SkyboxVertexOutput {
    var out: SkyboxVertexOutput;

    // The mesh contains one oversized fullscreen triangle:
    //
    //   (-1, -1)
    //   ( 3, -1)
    //   (-1,  3)
    //
    // These coordinates are already clip-space x/y.
    //
    // For Bevy's reversed-Z convention:
    //   z = 0.0 is far
    //   z = 1.0 is near
    //
    // Use far depth so the sky behaves like a background.
    out.clip_position = vec4<f32>(
        vertex.position.xy,
        0.0,
        1.0,
    );

    return out;
}

fn sky_view_direction_from_frag_coord(frag_coord: vec4<f32>) -> vec3<f32> {
    let ndc = frag_coord_to_ndc(frag_coord);

    // Bevy/WGPU reversed-depth convention:
    // z = 1.0 is near, z = 0.0 is far for perspective projection.
    let near_world = position_ndc_to_world(vec3<f32>(ndc.xy, 1.0));
    let far_world = position_ndc_to_world(vec3<f32>(ndc.xy, 0.0));

    return normalize(far_world - near_world);
}

@fragment
fn fragment(in: SkyboxVertexOutput) -> @location(0) vec4<f32> {
    let dir = sky_view_direction_from_frag_coord(in.clip_position);

    let sun_dir = normalize(sun_direction.xyz);
    let moon_dir = normalize(moon_direction.xyz);

    // Z-up world.
    let above_horizon = clamp(dir.z, 0.0, 1.0);
    let below_horizon = clamp(-dir.z, 0.0, 1.0);

    let horizon_color = vec3<f32>(0.68, 0.78, 0.92);
    let zenith_color = vec3<f32>(0.10, 0.28, 0.62);
    let nadir_color = vec3<f32>(0.34, 0.30, 0.26);

    let sky_color = mix(
        horizon_color,
        zenith_color,
        pow(above_horizon, 0.65),
    );

    let ground_color = mix(
        horizon_color,
        nadir_color,
        pow(below_horizon, 0.45),
    );

    let gradient_color = select(ground_color, sky_color, dir.z >= 0.0);

    let horizon_line_strength = 1.0 - smoothstep(0.0, 0.025, abs(dir.z));
    let horizon_line_color = vec3<f32>(0.93, 0.95, 0.98);

    var final_color = mix(
        gradient_color,
        horizon_line_color,
        horizon_line_strength,
    );

    // Sun disc and glow.
    let sun_amount = max(dot(dir, sun_dir), 0.0);
    let sun_disc = smoothstep(0.9992, 1.0, sun_amount);
    let sun_glow = pow(sun_amount, 128.0) * 0.35;

    final_color += vec3<f32>(1.0, 0.82, 0.45) * sun_glow;
    final_color = mix(final_color, vec3<f32>(1.0, 0.92, 0.65), sun_disc);

    // Moon disc and glow.
    let moon_amount = max(dot(dir, moon_dir), 0.0);
    let moon_disc = smoothstep(0.9994, 1.0, moon_amount);
    let moon_glow = pow(moon_amount, 96.0) * 0.12;

    final_color += vec3<f32>(0.55, 0.62, 0.78) * moon_glow;
    final_color = mix(final_color, vec3<f32>(0.86, 0.88, 0.92), moon_disc);

    return vec4<f32>(final_color, 1.0);
}