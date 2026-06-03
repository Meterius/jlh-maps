use crate::app::map::core::MAP_VIEW_COLOR_RENDER_LAYER;
use crate::app::map::core::MapViewSettings;
use crate::app::map::feature::bucket_layer::TileBucketLayerMeta;
use crate::app::map::feature::bucket_manager::TileBucket;
use crate::app::map::feature::edge_distance_texture::FeatureTileEdgeDistanceTexture;
use crate::app::map::feature::mesh::{FeatureTileMesh, FeatureTileMeshConfig};
use crate::app::map::feature::tile::FeatureTile;
use bevy::asset::{Asset, AssetApp, Handle, load_internal_asset, uuid_handle};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParamItem;
use bevy::light::NotShadowCaster;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, OpaqueRendererMethod};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub struct WatersPlugin;

impl Plugin for WatersPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            WATER_GRADIENT_MATERIAL_SHADER_HANDLE,
            "../../../../assets/shaders/water_gradient.fragment.wgsl",
            Shader::from_wgsl
        );
        app.register_type::<WaterMaterialUniform>()
            .register_type::<WaterMaterialExtension>()
            .register_asset_reflect::<WaterMaterial>()
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_systems(Update, update_water_material_time);
    }
}

const WATER_GRADIENT_MATERIAL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("7b3c87e1-6c8c-4ae6-9c9f-9e3e271d8b90");
const WATER_SOURCE_LAYER: &str = "water";
const WATER_EDGE_DISTANCE_TEXTURE_RESOLUTION: UVec2 = UVec2::new(512, 512);

const WATER_COLOR: Hsva = Hsva::hsv(213., 0.4, 0.95);
const WATER2_COLOR: Hsva = Hsva::hsv(216., 0.5, 0.92);

pub(super) type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterMaterialExtension>;

#[derive(ShaderType, Reflect, Debug, Clone, Copy)]
pub struct WaterMaterialUniform {
    pub water_color: Vec4,
    pub water2_color: Vec4,
    pub time: f32,
    _webgl2_padding_8b: u32,
    _webgl2_padding_12b: u32,
    _webgl2_padding_16b: u32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct WaterMaterialExtension {
    #[texture(100)]
    #[sampler(101)]
    pub edge_distance_texture: Handle<Image>,

    #[uniform(102)]
    pub uniform: WaterMaterialUniform,
}

impl MaterialExtension for WaterMaterialExtension {
    fn fragment_shader() -> ShaderRef {
        WATER_GRADIENT_MATERIAL_SHADER_HANDLE.into()
    }
}

#[derive(Component)]
struct WaterTile;

#[derive(Component)]
pub(crate) struct WaterTileBucket;

pub(super) struct WaterTileBucketLayer;

type WaterInitializeTileParams = (
    ResMut<'static, Assets<Image>>,
    ResMut<'static, Assets<WaterMaterial>>,
);

impl TileBucketLayerMeta for WaterTileBucketLayer {
    type BucketMarker = WaterTileBucket;
    type EnabledParams = Res<'static, MapViewSettings>;
    type SpawnParams = WaterInitializeTileParams;

    fn is_enabled(settings: &SystemParamItem<'_, '_, Self::EnabledParams>) -> bool {
        settings.enable_waters
    }

    fn spawn(
        mut e_commands: EntityCommands,
        params: &mut SystemParamItem<'_, '_, Self::SpawnParams>,
        _: Entity,
        bucket: &TileBucket,
    ) {
        let edge_distance_texture = FeatureTileEdgeDistanceTexture::new(
            WATER_SOURCE_LAYER,
            WATER_EDGE_DISTANCE_TEXTURE_RESOLUTION,
            &mut params.0,
        );
        let material = params.1.add(ExtendedMaterial {
            base: StandardMaterial {
                opaque_render_method: OpaqueRendererMethod::Forward,
                base_color: Color::WHITE,
                depth_bias: 40000.0,
                ..default()
            },
            extension: WaterMaterialExtension {
                edge_distance_texture: edge_distance_texture.texture().clone(),
                uniform: WaterMaterialUniform {
                    water_color: Srgba::from(WATER_COLOR).to_vec4(),
                    water2_color: Srgba::from(WATER2_COLOR).to_vec4(),
                    time: 0.,
                    _webgl2_padding_8b: 0,
                    _webgl2_padding_12b: 0,
                    _webgl2_padding_16b: 0,
                },
            },
        });

        e_commands.insert((
            Name::new(format!(
                "Water tile {}/{:?}",
                bucket.source_id, bucket.tile_id
            )),
            RenderLayers::layer(MAP_VIEW_COLOR_RENDER_LAYER),
            MeshMaterial3d(material),
            NotShadowCaster,
            WaterTile,
            FeatureTile::new(
                bucket.maplibre_int_id,
                &bucket.source_id,
                bucket.tile_id,
                bucket.center,
            ),
            FeatureTileMesh::new(FeatureTileMeshConfig {
                layer_id: WATER_SOURCE_LAYER,
                ..default()
            }),
            edge_distance_texture,
        ));
    }
}

fn update_water_material_time(time: Res<Time>, mut materials: ResMut<Assets<WaterMaterial>>) {
    let elapsed = time.elapsed_secs();
    for (_, material) in materials.iter_mut() {
        material.extension.uniform.time = elapsed;
    }
}
