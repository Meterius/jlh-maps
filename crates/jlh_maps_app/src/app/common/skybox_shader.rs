use bevy::{
    app::{App, Plugin},
    asset::{load_internal_asset, uuid_handle, RenderAssetUsages},
    camera::visibility::{NoCpuCulling, NoFrustumCulling},
    mesh::{Indices, MeshVertexBufferLayoutRef},
    pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin},
    prelude::*,
    render::render_resource::{
        AsBindGroup, CompareFunction, PrimitiveTopology, RenderPipelineDescriptor,
        SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

pub const SKYBOX_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("a2f7d6bd-7f79-4c4d-8e63-2a8c1f1a9c42");

pub struct SkyboxShaderPlugin;

impl Plugin for SkyboxShaderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SKYBOX_SHADER_HANDLE,
            "../../../assets-internal/shaders/skybox.wgsl",
            Shader::from_wgsl
        );

        app.add_plugins(MaterialPlugin::<SkyboxShaderMaterial>::default())
            .add_systems(Update, spawn_skybox);
    }
}

#[derive(Component)]
pub struct SkyboxShaderMesh;

#[derive(Component)]
pub struct SkyboxShaderCamera;

#[derive(Asset, TypePath, AsBindGroup, Debug, Default, Clone)]
pub struct SkyboxShaderMaterial {
    #[uniform(0)]
    pub sun_direction: Vec4,
    #[uniform(1)]
    pub moon_direction: Vec4,
}

impl Material for SkyboxShaderMaterial {
    fn vertex_shader() -> ShaderRef {
        SKYBOX_SHADER_HANDLE.into()
    }

    fn fragment_shader() -> ShaderRef {
        SKYBOX_SHADER_HANDLE.into()
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<SkyboxShaderMaterial>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Fullscreen triangle; culling is unnecessary and can make the triangle vanish
        // depending on winding conventions.
        descriptor.primitive.cull_mode = None;

        if let Some(depth_stencil) = &mut descriptor.depth_stencil {
            depth_stencil.depth_write_enabled = false;

            // Fullscreen shader writes far depth in WGSL using clip_position.z = 0.0.
            // With Bevy/reversed-Z style depth, far depth passes against the cleared
            // depth buffer with GreaterEqual, but fails behind already-rendered geometry.
            depth_stencil.depth_compare = CompareFunction::Always;
        }

        Ok(())
    }
}

pub fn spawn_skybox(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyboxShaderMaterial>>,
    camera: Query<(), Added<SkyboxShaderCamera>>,
    existing_skybox: Query<(), With<SkyboxShaderMesh>>,
) {
    if existing_skybox.iter().next().is_some() {
        return;
    }

    if camera.iter().next().is_none() {
        return;
    }

    commands.spawn((
        Name::new("Skybox Fullscreen Triangle"),
        SkyboxShaderMesh,

        NoFrustumCulling,
        NoCpuCulling,

        Mesh3d(meshes.add(fullscreen_triangle_mesh())),
        MeshMaterial3d(materials.add(SkyboxShaderMaterial::default())),

        Transform::IDENTITY,
    ));
}

fn fullscreen_triangle_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );

    // Clip-space positions.
    // Vertex shader passes vertex.position.xy directly to @builtin(position),
    // producing a single oversized triangle that covers the full screen.
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1.0, -1.0, 0.0],
            [3.0, -1.0, 0.0],
            [-1.0, 3.0, 0.0],
        ],
    );

    mesh.insert_indices(Indices::U32(vec![0, 1, 2]));

    mesh
}

pub fn sky_direction_from_az_el_degrees(
    azimuth_degrees: f32,
    elevation_degrees: f32,
) -> Vec4 {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.clamp(-89.0, 89.0).to_radians();
    let horizontal = elevation.cos();

    Vec3::new(
        horizontal * azimuth.cos(),
        horizontal * azimuth.sin(),
        elevation.sin(),
    )
    .normalize_or_zero()
    .extend(0.0)
}