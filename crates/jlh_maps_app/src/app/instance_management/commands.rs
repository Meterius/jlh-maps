use bevy::prelude::*;
use std::sync::{Arc, Mutex};

#[derive(Resource, Clone, Default)]
pub struct InstanceCommandQueue {
    commands: Arc<Mutex<Vec<InstanceCommand>>>,
}

impl InstanceCommandQueue {
    pub fn enqueue(&self, command: impl FnOnce(&mut World) + Send + 'static) {
        self.commands.lock().unwrap().push(InstanceCommand {
            command: Box::new(command),
        });
    }

    pub fn clear(&self) {
        self.commands.lock().unwrap().clear();
    }
}

struct InstanceCommand {
    command: Box<dyn FnOnce(&mut World) + Send>,
}

pub(super) struct CommandsPlugin {
    pub command_queue: InstanceCommandQueue,
}

impl Plugin for CommandsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.command_queue.clone());
        app.add_systems(PreUpdate, drain_command_queue);
    }
}

fn drain_command_queue(world: &mut World) {
    let commands = {
        let Some(command_queue) = world.get_resource::<InstanceCommandQueue>() else {
            return;
        };
        let mut commands = command_queue.commands.lock().unwrap();
        std::mem::take(&mut *commands)
    };

    for command in commands {
        (command.command)(world);
    }
}
