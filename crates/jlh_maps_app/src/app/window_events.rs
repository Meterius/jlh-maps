use crate::app::instance::BevyInstanceInner;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput, NativeKey, NativeKeyCode};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::math::Vec2;
use bevy::prelude;
use bevy::prelude::{
    CursorEntered, CursorLeft, CursorMoved, Entity, KeyCode, MouseButton, Window, World,
};
use bevy::window::{
    WindowEvent as BevyWindowEvent, WindowFocused, WindowResized, WindowScaleFactorChanged,
};
use std::rc::Weak;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct WindowInstanceRef {
    pub(crate) instance: Weak<BevyInstanceInner>,
    pub(crate) window_eid: Entity,
}

impl WindowInstanceRef {
    fn execute(&self, command: impl FnOnce(&mut World)) -> Result<(), String> {
        let Some(instance) = self.instance.upgrade() else {
            return Err("Bevy instance is not mounted".to_string());
        };

        instance.execute(command)
    }
}

#[wasm_bindgen]
impl WindowInstanceRef {
    pub fn resize(
        &self,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> prelude::Result<(), String> {
        self.execute(|world| {
            resize_window(world, self.window_eid, width, height, scale_factor);
        })
    }

    pub fn forward_focus(&self, focused: bool) -> prelude::Result<(), String> {
        self.execute(|world| {
            let event = WindowFocused {
                window: self.window_eid,
                focused,
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::WindowFocused(event));
        })
    }

    pub fn forward_cursor_entered(&self) -> prelude::Result<(), String> {
        self.execute(|world| {
            let event = CursorEntered {
                window: self.window_eid,
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorEntered(event));
        })
    }

    pub fn forward_cursor_left(&self) -> prelude::Result<(), String> {
        self.execute(|world| {
            if let Some(mut window_component) = world.get_mut::<Window>(self.window_eid) {
                window_component.set_cursor_position(None);
            }

            let event = CursorLeft {
                window: self.window_eid,
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorLeft(event));
        })
    }

    pub fn forward_cursor_moved(
        &self,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
    ) -> prelude::Result<(), String> {
        self.execute(|world| {
            let position = Vec2::new(x, y);
            let delta = Vec2::new(delta_x, delta_y);

            if let Some(mut window_component) = world.get_mut::<Window>(self.window_eid) {
                window_component.set_cursor_position(Some(position));
            }

            let event = CursorMoved {
                window: self.window_eid,
                position,
                delta: Some(delta),
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::CursorMoved(event));
            world.write_message(MouseMotion { delta });
            world.write_message(BevyWindowEvent::MouseMotion(MouseMotion { delta }));
        })
    }

    pub fn forward_mouse_button(&self, button: i16, pressed: bool) -> prelude::Result<(), String> {
        self.execute(|world| {
            let event = MouseButtonInput {
                button: web_mouse_button(button),
                state: button_state(pressed),
                window: self.window_eid,
            };
            world.write_message(event);
            world.write_message(BevyWindowEvent::MouseButtonInput(event));
        })
    }

    pub fn forward_mouse_wheel(
        &self,
        delta_x: f32,
        delta_y: f32,
        delta_mode: u32,
    ) -> prelude::Result<(), String> {
        self.execute(|world| {
            let event = MouseWheel {
                unit: if delta_mode == 1 {
                    MouseScrollUnit::Line
                } else {
                    MouseScrollUnit::Pixel
                },
                x: delta_x,
                y: -delta_y,
                window: self.window_eid,
            };
            world.write_message(event);
            world.write_message(BevyWindowEvent::MouseWheel(event));
        })
    }

    pub fn forward_keyboard_input(
        &self,
        code: String,
        key: String,
        pressed: bool,
        repeat: bool,
    ) -> prelude::Result<(), String> {
        self.execute(|world| {
            let logical_key = web_logical_key(&key);
            let text = match (&logical_key, pressed) {
                (Key::Character(text), true) => Some(text.clone()),
                _ => None,
            };
            let event = KeyboardInput {
                key_code: web_key_code(&code),
                logical_key,
                state: button_state(pressed),
                text,
                repeat,
                window: self.window_eid,
            };
            world.write_message(event.clone());
            world.write_message(BevyWindowEvent::KeyboardInput(event));
        })
    }
}

fn resize_window(
    world: &mut World,
    window_eid: Entity,
    width: u32,
    height: u32,
    scale_factor: f32,
) {
    let Some((scale_factor_changed, resized)) =
        world.get_mut::<Window>(window_eid).map(|mut window| {
            let scale_factor = scale_factor.max(1.0);
            let scale_factor_changed = (window.scale_factor() - scale_factor).abs() > f32::EPSILON;
            let size_changed = window.width() as u32 != width || window.height() as u32 != height;

            if !scale_factor_changed && !size_changed {
                return (false, None);
            }

            window.resolution.set_scale_factor(scale_factor);
            window.resolution.set(width as f32, height as f32);

            let resized = WindowResized {
                window: window_eid,
                width: window.width(),
                height: window.height(),
            };
            (scale_factor_changed, Some(resized))
        })
    else {
        return;
    };

    if scale_factor_changed {
        let event = WindowScaleFactorChanged {
            window: window_eid,
            scale_factor: scale_factor.max(1.0) as f64,
        };
        world.write_message(event.clone());
        world.write_message(BevyWindowEvent::WindowScaleFactorChanged(event));
    }

    if let Some(event) = resized {
        world.write_message(event.clone());
        world.write_message(BevyWindowEvent::WindowResized(event));
    }
}

fn button_state(pressed: bool) -> ButtonState {
    if pressed {
        ButtonState::Pressed
    } else {
        ButtonState::Released
    }
}

fn web_mouse_button(button: i16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        other => MouseButton::Other(other as u16),
    }
}

fn web_logical_key(key: &str) -> Key {
    match key {
        "Alt" => Key::Alt,
        "Backspace" => Key::Backspace,
        "Control" => Key::Control,
        "Delete" => Key::Delete,
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Meta" => Key::Meta,
        "Shift" => Key::Shift,
        "Tab" => Key::Tab,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        "" => Key::Unidentified(NativeKey::Unidentified),
        text => Key::Character(text.into()),
    }
}

fn web_key_code(code: &str) -> KeyCode {
    match code {
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "F1" => KeyCode::F1,
        "F2" => KeyCode::F2,
        "F3" => KeyCode::F3,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F8" => KeyCode::F8,
        "F9" => KeyCode::F9,
        "F10" => KeyCode::F10,
        "F11" => KeyCode::F11,
        "F12" => KeyCode::F12,
        "KeyA" => KeyCode::KeyA,
        "KeyB" => KeyCode::KeyB,
        "KeyC" => KeyCode::KeyC,
        "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE,
        "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG,
        "KeyH" => KeyCode::KeyH,
        "KeyI" => KeyCode::KeyI,
        "KeyJ" => KeyCode::KeyJ,
        "KeyK" => KeyCode::KeyK,
        "KeyL" => KeyCode::KeyL,
        "KeyM" => KeyCode::KeyM,
        "KeyN" => KeyCode::KeyN,
        "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        "KeyS" => KeyCode::KeyS,
        "KeyT" => KeyCode::KeyT,
        "KeyU" => KeyCode::KeyU,
        "KeyV" => KeyCode::KeyV,
        "KeyW" => KeyCode::KeyW,
        "KeyX" => KeyCode::KeyX,
        "KeyY" => KeyCode::KeyY,
        "KeyZ" => KeyCode::KeyZ,
        "Space" => KeyCode::Space,
        "Tab" => KeyCode::Tab,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "ArrowUp" => KeyCode::ArrowUp,
        "ShiftLeft" => KeyCode::ShiftLeft,
        "ShiftRight" => KeyCode::ShiftRight,
        "ControlLeft" => KeyCode::ControlLeft,
        "ControlRight" => KeyCode::ControlRight,
        "AltLeft" => KeyCode::AltLeft,
        "AltRight" => KeyCode::AltRight,
        "MetaLeft" => KeyCode::SuperLeft,
        "MetaRight" => KeyCode::SuperRight,
        _ => KeyCode::Unidentified(NativeKeyCode::Unidentified),
    }
}
