use crate::app::map::core::MAP_VIEW_COLOR_RENDER_LAYER;
use crate::app::map::core::MapViewSettings;
use crate::app::map::feature::bucket_layer::TileBucketLayerMeta;
use crate::app::map::feature::bucket_manager::TileBucket;
use crate::app::map::feature::mesh::{FeatureTileMesh, FeatureTileMeshConfig};
use crate::app::map::feature::tile::FeatureTile;
use crate::app::map::feature_layers::MapFeatureDistanceVisibility;
use bevy::asset::{Asset, AssetApp, Handle, load_internal_asset, uuid_handle};
use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParamItem;
use bevy::pbr::{
    DefaultOpaqueRendererMethod, ExtendedMaterial, MaterialExtension, MaterialPlugin,
    OpaqueRendererMethod,
};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            BUILDING_MATERIAL_SHADER_HANDLE,
            "../../../../assets-internal/shaders/building_pbr.fragment.wgsl",
            Shader::from_wgsl
        );
        app.register_type::<BuildingMaterialUniform>()
            .register_type::<BuildingMaterialExtension>()
            .register_asset_reflect::<BuildingMaterial>()
            .add_plugins(MaterialPlugin::<BuildingMaterial>::default())
            .init_resource::<GlobalBuildingMaterial>()
            .add_systems(PreUpdate, sync_building_material_opaque_render_method);
    }
}

const BUILDING_SOURCE_LAYER: &str = "building";
const BUILDING_BASE_ALTITUDE_PROPERTY_KEYS: &[&str] = &["render_min_height", "min_height"];
const BUILDING_TOP_ALTITUDE_PROPERTY_KEYS: &[&str] = &["render_height", "height"];
const BUILDING_MATERIAL_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("6821f839-72cf-4b53-a709-d0260d921b72");

#[derive(Resource, Reflect)]
pub(crate) struct GlobalBuildingMaterial(Handle<BuildingMaterial>);

type BuildingMaterial = ExtendedMaterial<StandardMaterial, BuildingMaterialExtension>;

#[derive(ShaderType, Reflect, Debug, Clone, Copy)]
struct BuildingMaterialUniform {
    height_gradient_strength: f32,
    height_gradient_upper_altitude: f32,

    base_shadow_strength: f32,
    base_shadow_upper_altitude: f32,

    lambert_tint_strength: f32,
    lambert_shade_strength: f32,
    _webgl2_padding_24b: u32,
    _webgl2_padding_28b: u32,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct BuildingMaterialExtension {
    #[uniform(100)]
    uniform: BuildingMaterialUniform,
}

impl MaterialExtension for BuildingMaterialExtension {
    fn fragment_shader() -> ShaderRef {
        BUILDING_MATERIAL_SHADER_HANDLE.into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        BUILDING_MATERIAL_SHADER_HANDLE.into()
    }
}

impl GlobalBuildingMaterial {
    fn material() -> BuildingMaterial {
        ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::hsv(20., 0.08, 0.76),
                perceptual_roughness: 0.8,
                reflectance: 0.05,
                opaque_render_method: OpaqueRendererMethod::Auto,
                ..default()
            },
            extension: BuildingMaterialExtension {
                uniform: BuildingMaterialUniform {
                    height_gradient_strength: 0.25,
                    height_gradient_upper_altitude: 40.0,
                    base_shadow_strength: 0.1,
                    base_shadow_upper_altitude: 3.0,
                    lambert_tint_strength: 0.2,
                    lambert_shade_strength: 0.1,
                    _webgl2_padding_24b: 0,
                    _webgl2_padding_28b: 0,
                },
            },
        }
    }
}

impl FromWorld for GlobalBuildingMaterial {
    fn from_world(world: &mut World) -> Self {
        let mut materials = world.resource_mut::<Assets<BuildingMaterial>>();
        Self(materials.add(GlobalBuildingMaterial::material()))
    }
}

fn sync_building_material_opaque_render_method(
    default_opaque_renderer_method: Res<DefaultOpaqueRendererMethod>,
    handle: Res<GlobalBuildingMaterial>,
    mut materials: ResMut<Assets<BuildingMaterial>>,
) {
    if default_opaque_renderer_method.is_changed()
        && let Some(material) = materials.get_mut(&handle.0)
    {
        *material = GlobalBuildingMaterial::material();
    }
}

#[derive(Component)]
struct BuildingTile;

#[derive(Component)]
pub(crate) struct BuildingTileBucket;

pub(super) struct BuildingTileBucketLayer;

type BuildingInitializeTileParams = Res<'static, GlobalBuildingMaterial>;

impl TileBucketLayerMeta for BuildingTileBucketLayer {
    type BucketMarker = BuildingTileBucket;
    type EnabledParams = Res<'static, MapViewSettings>;
    type SpawnParams = BuildingInitializeTileParams;

    fn is_enabled(settings: &SystemParamItem<'_, '_, Self::EnabledParams>) -> bool {
        settings.enable_buildings
    }

    fn spawn(
        mut e_commands: EntityCommands,
        params: &mut SystemParamItem<'_, '_, Self::SpawnParams>,
        _bucket_eid: Entity,
        bucket: &TileBucket,
    ) {
        e_commands.insert((
            Name::new(format!(
                "Building tile {}/{:?}",
                bucket.source_id, bucket.tile_id
            )),
            Visibility::Hidden,
            RenderLayers::layer(MAP_VIEW_COLOR_RENDER_LAYER),
            MapFeatureDistanceVisibility {
                flat_half_extents: bucket.half_extents,
            },
            BuildingTile,
            FeatureTile::new(
                bucket.maplibre_int_eid,
                &bucket.source_id,
                bucket.tile_id,
                bucket.center,
            ),
            FeatureTileMesh::new(FeatureTileMeshConfig {
                layer_id: BUILDING_SOURCE_LAYER,
                base_property_keys: Some(BUILDING_BASE_ALTITUDE_PROPERTY_KEYS),
                top_property_keys: Some(BUILDING_TOP_ALTITUDE_PROPERTY_KEYS),
                wall_normal_smooth_angle: Some(35.0_f32.to_radians()),
            }),
            MeshMaterial3d(params.0.clone()),
        ));
    }
}
