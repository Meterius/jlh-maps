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
use bevy::render::render_resource::ShaderType;

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

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct SkyboxShaderParams {
    pub sun_elevation_degrees: f32,
    pub moon_elevation_degrees: f32,
    pub haze: f32,
    pub exposure: f32,
}

impl Default for SkyboxShaderParams {
    fn default() -> Self {
        Self {
            sun_elevation_degrees: 45.0,
            moon_elevation_degrees: -45.0,
            haze: 0.25,
            exposure: 1.0,
        }
    }
}

#[derive(ShaderType, Debug, Clone, Copy)]
pub struct SkyboxShaderUniform {
    pub sun_direction: Vec4,
    pub moon_direction: Vec4,

    pub sun_color: Vec4,
    pub moon_color: Vec4,
    pub ambient_color: Vec4,

    pub params: SkyboxShaderParams,
}

impl Default for SkyboxShaderUniform {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.0, 1.0, 1.0).normalize().extend(0.0),
            moon_direction: Vec3::new(0.0, -1.0, -1.0).normalize().extend(0.0),
            sun_color: Vec4::new(1.0, 0.92, 0.75, 1.0),
            moon_color: Vec4::new(0.62, 0.68, 1.0, 0.0),
            ambient_color: Vec4::new(0.55, 0.68, 1.0, 1.0),
            params: SkyboxShaderParams::default(),
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Default, Clone)]
pub struct SkyboxShaderMaterial {
    #[uniform(0)]
    pub sky: SkyboxShaderUniform,
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
            
            // TODO: reasoning

            // Fullscreen shader writes far depth in WGSL using clip_position.z = 0.0.
            // With Bevy/reversed-Z style depth, far depth passes against the cleared
            // depth buffer with GreaterEqual, but fails behind already-rendered geometry.
            depth_stencil.depth_compare = CompareFunction::GreaterEqual;
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