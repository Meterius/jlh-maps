pub mod commands;

use crate::app::instance_management::commands::InstanceCommandQueue;
use bevy::prelude::*;

pub struct InstanceManagementPlugin {
    pub command_queue: InstanceCommandQueue,
}

impl Plugin for InstanceManagementPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(commands::CommandsPlugin {
            command_queue: self.command_queue.clone(),
        });
    }
}
