use deskcustom_proto::{KeyAction, MouseButton};

#[derive(Debug, Clone, Copy)]
pub struct MouseDelta {
    pub dx: i32,
    pub dy: i32,
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    MouseMove(MouseDelta),
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    Key {
        scancode: u16,
        action: KeyAction,
        modifiers: u8,
    },
}

pub trait InputCapture: Send {
    fn poll(&mut self) -> anyhow::Result<Vec<InputEvent>>;
}

pub trait InputInject: Send {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()>;
}
