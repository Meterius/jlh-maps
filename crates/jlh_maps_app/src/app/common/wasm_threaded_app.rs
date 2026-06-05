use bevy::app::{App, Plugin};
use bevy::pbr::{MeshInputUniform, MeshUniform};
use bevy::render::RenderApp;
use bevy::render::batching::gpu_preprocessing::BatchedInstanceBuffers;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::PipelineCache;
use bevy::render::renderer::{
    RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
};
use bevy::render::sync_world::TemporaryRenderEntity;
use bevy::render::texture::{
    DefaultImageSampler, FallbackImage, FallbackImageCubemap, FallbackImageFormatMsaaCache,
    FallbackImageZero, GpuImage, ManualTextureViews, TextureCache,
};
use bevy::render::view::{
    ExtractedWindows, ViewDepthTexture, ViewTarget, ViewTargetAttachments, WindowSurfaces,
};
use tracing::{debug, warn};

use bevy::ecs::schedule::{ExecutorKind, Schedules, ThreadLocalResources};
use bevy_ecs::prelude::{Component, Resource, World};

pub struct WasmThreadedAppPlugin;

impl Plugin for WasmThreadedAppPlugin {
    fn build(&self, app: &mut App) {
        mark_wasm_thread_local_resources(app);
        configure_wasm_threaded_schedules(app);
    }
}

// Thread Local Resources

fn mark_wasm_thread_local_resources(app: &mut App) {
    mark_main_world_thread_local_resources(app.world_mut());

    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        warn!("Could not configure wasm threaded render schedules: RenderApp is missing");
        return;
    };

    mark_render_world_thread_local_resources(render_app.world_mut());
}

fn mark_main_world_thread_local_resources(world: &mut World) {
    mark_shared_render_thread_local_resources(world, "main world");
}

fn mark_render_world_thread_local_resources(world: &mut World) {
    mark_shared_render_thread_local_resources(world, "render world");
    mark_thread_affine_resource::<RenderInstance>(world, "render world", "RenderInstance");
    mark_thread_affine_resource::<ExtractedWindows>(world, "render world", "ExtractedWindows");
    mark_thread_affine_resource::<RenderAssets<GpuImage>>(
        world,
        "render world",
        "RenderAssets<GpuImage>",
    );
    mark_thread_affine_resource::<ManualTextureViews>(world, "render world", "ManualTextureViews");
    mark_thread_affine_resource::<ViewTargetAttachments>(
        world,
        "render world",
        "ViewTargetAttachments",
    );
    mark_thread_affine_resource::<PipelineCache>(world, "render world", "PipelineCache");
    mark_thread_affine_resource::<DefaultImageSampler>(
        world,
        "render world",
        "DefaultImageSampler",
    );
    mark_thread_affine_resource::<TextureCache>(world, "render world", "TextureCache");
    mark_thread_affine_resource::<FallbackImage>(world, "render world", "FallbackImage");
    mark_thread_affine_resource::<FallbackImageZero>(world, "render world", "FallbackImageZero");
    mark_thread_affine_resource::<FallbackImageCubemap>(
        world,
        "render world",
        "FallbackImageCubemap",
    );
    mark_thread_affine_resource::<FallbackImageFormatMsaaCache>(
        world,
        "render world",
        "FallbackImageFormatMsaaCache",
    );
    mark_thread_affine_resource::<WindowSurfaces>(world, "render world", "WindowSurfaces");
    mark_thread_affine_resource::<BatchedInstanceBuffers<MeshUniform, MeshInputUniform>>(
        world,
        "render world",
        "BatchedInstanceBuffers<MeshUniform, MeshInputUniform>",
    );

    mark_thread_affine_component::<ViewTarget>(world, "render world", "ViewTarget");
    mark_thread_affine_component::<ViewDepthTexture>(world, "render world", "ViewDepthTexture");
    mark_thread_affine_component::<TemporaryRenderEntity>(
        world,
        "render world",
        "TemporaryRenderEntity",
    );
}

fn mark_shared_render_thread_local_resources(world: &mut World, world_name: &str) {
    mark_thread_affine_resource::<RenderDevice>(world, world_name, "RenderDevice");
    mark_thread_affine_resource::<RenderQueue>(world, world_name, "RenderQueue");
    mark_thread_affine_resource::<RenderAdapter>(world, world_name, "RenderAdapter");
    mark_thread_affine_resource::<RenderAdapterInfo>(world, world_name, "RenderAdapterInfo");
}

fn mark_thread_affine_resource<T: Resource>(
    world: &mut World,
    world_name: &str,
    resource_name: &str,
) {
    let resource_id = world.register_resource::<T>();
    world.init_resource::<ThreadLocalResources>();
    world
        .resource_mut::<ThreadLocalResources>()
        .insert(resource_id);
    debug!("Marked {world_name} {resource_name} as wasm thread-affine");
}

fn mark_thread_affine_component<T: Component>(
    world: &mut World,
    world_name: &str,
    component_name: &str,
) {
    let component_id = world.register_component::<T>();
    world.init_resource::<ThreadLocalResources>();
    world
        .resource_mut::<ThreadLocalResources>()
        .insert_component(component_id);
    debug!("Marked {world_name} {component_name} component as wasm thread-affine");
}

// Scheduling

fn configure_wasm_threaded_schedules(app: &mut App) {
    configure_world_schedules(app.world_mut(), "main world", ExecutorKind::MultiThreaded);

    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        warn!("Could not configure wasm threaded render schedules: RenderApp is missing");
        return;
    };

    configure_world_schedules(
        render_app.world_mut(),
        "render world",
        ExecutorKind::MultiThreaded,
    );
}

fn configure_world_schedules(world: &mut World, world_name: &str, executor_kind: ExecutorKind) {
    let Some(mut schedules) = world.get_resource_mut::<Schedules>() else {
        warn!(
            "Could not configure wasm threaded {world_name} schedules: Schedules resource is missing"
        );
        return;
    };

    let mut configured = 0usize;
    for (label, schedule) in schedules.iter_mut() {
        let before = schedule.get_executor_kind();
        schedule.set_executor_kind(executor_kind);
        let runtime = schedule.get_executor_kind();
        configured += 1;
        debug!(
            "Configured {world_name} {label:?} schedule executor: {before:?} -> runtime {runtime:?}"
        );
    }

    debug!("Configured {configured} wasm threaded {world_name} schedules to {executor_kind:?}");
}
