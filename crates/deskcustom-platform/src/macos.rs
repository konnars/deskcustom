use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use anyhow::Result;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, CGMouseButton, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use deskcustom_proto::{KeyAction, MouseButton};
use tracing::error;

use crate::{InputCapture, InputEvent, InputInject, MouseDelta};

pub struct MacInputCapture {
    rx: Receiver<InputEvent>,
}

pub struct MacInputInject;

impl MacInputCapture {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            if let Err(err) = run_event_tap(tx) {
                error!(?err, "macOS event tap failed");
            }
        });

        Ok(Self { rx })
    }
}

impl InputCapture for MacInputCapture {
    fn poll(&mut self) -> anyhow::Result<Vec<InputEvent>> {
        let mut events = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            events.push(ev);
        }
        Ok(events)
    }
}

impl MacInputInject {
    pub fn new() -> Self {
        Self
    }
}

impl InputInject for MacInputInject {
    fn inject(&mut self, event: &InputEvent) -> anyhow::Result<()> {
        let source = match event_source() {
            Ok(source) => source,
            Err(()) => return Ok(()),
        };

        match event {
            InputEvent::MouseMove(delta) => {
                if let Ok(ev) = CGEvent::new_mouse_event(
                    source,
                    CGEventType::MouseMoved,
                    CGPoint::new(0.0, 0.0),
                    CGMouseButton::Left,
                ) {
                    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, delta.dx.into());
                    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, delta.dy.into());
                    ev.post(CGEventTapLocation::HID);
                }
            }
            InputEvent::MouseButton { button, pressed } => {
                let event_type = match (button, pressed) {
                    (MouseButton::Left, true) => CGEventType::LeftMouseDown,
                    (MouseButton::Left, false) => CGEventType::LeftMouseUp,
                    (MouseButton::Right, true) => CGEventType::RightMouseDown,
                    (MouseButton::Right, false) => CGEventType::RightMouseUp,
                    (MouseButton::Middle, true) => CGEventType::OtherMouseDown,
                    (MouseButton::Middle, false) => CGEventType::OtherMouseUp,
                    _ => return Ok(()),
                };
                if let Ok(ev) = CGEvent::new_mouse_event(
                    source,
                    event_type,
                    CGPoint::new(0.0, 0.0),
                    CGMouseButton::Left,
                ) {
                    ev.post(CGEventTapLocation::HID);
                }
            }
            InputEvent::Key {
                scancode,
                action,
                modifiers,
            } => {
                let keycode = scancode_to_keycode(*scancode);
                let down = matches!(action, KeyAction::Down);
                if let Ok(ev) = CGEvent::new_keyboard_event(source, keycode, down) {
                    ev.set_flags(modifiers_to_flags(*modifiers));
                    ev.post(CGEventTapLocation::HID);
                }
            }
        }
        Ok(())
    }
}

fn event_source() -> Result<CGEventSource, ()> {
    CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
}

fn run_event_tap(tx: Sender<InputEvent>) -> Result<()> {
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![
            CGEventType::MouseMoved,
            CGEventType::LeftMouseDragged,
            CGEventType::RightMouseDragged,
            CGEventType::OtherMouseDragged,
            CGEventType::KeyDown,
            CGEventType::KeyUp,
        ],
        move |_proxy: CGEventTapProxy, event_type: CGEventType, event: &CGEvent| {
            dispatch_event(&tx, event_type, event);
            None
        },
    )
    .map_err(|_| anyhow::anyhow!("create CGEventTap — grant Accessibility + Input Monitoring"))?;

    let run_loop_source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| anyhow::anyhow!("create runloop source"))?;

    unsafe {
        let run_loop = CFRunLoop::get_current();
        run_loop.add_source(&run_loop_source, kCFRunLoopCommonModes);
    }

    tap.enable();
    CFRunLoop::run_current();

    Ok(())
}

fn dispatch_event(tx: &Sender<InputEvent>, event_type: CGEventType, event: &CGEvent) {
    let result = match event_type {
        CGEventType::MouseMoved
        | CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => {
            let dx = clamp_i64_to_i32(event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X));
            let dy = clamp_i64_to_i32(event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y));
            if dx == 0 && dy == 0 {
                return;
            }
            tx.send(InputEvent::MouseMove(MouseDelta { dx, dy }))
        }
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let scancode =
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let action = match event_type {
                CGEventType::KeyDown => KeyAction::Down,
                CGEventType::KeyUp => KeyAction::Up,
                _ => return,
            };
            let modifiers = flags_to_modifiers(event.get_flags());
            tx.send(InputEvent::Key {
                scancode,
                action,
                modifiers,
            })
        }
        _ => return,
    };

    if let Err(err) = result {
        error!(?err, "failed to enqueue captured event");
    }
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn scancode_to_keycode(scancode: u16) -> u16 {
    scancode.min(127)
}

fn modifiers_to_flags(modifiers: u8) -> CGEventFlags {
    let mut flags = CGEventFlags::empty();
    if modifiers & 0x01 != 0 {
        flags.insert(CGEventFlags::CGEventFlagShift);
    }
    if modifiers & 0x02 != 0 {
        flags.insert(CGEventFlags::CGEventFlagControl);
    }
    if modifiers & 0x04 != 0 {
        flags.insert(CGEventFlags::CGEventFlagAlternate);
    }
    if modifiers & 0x08 != 0 {
        flags.insert(CGEventFlags::CGEventFlagCommand);
    }
    flags
}

fn flags_to_modifiers(flags: CGEventFlags) -> u8 {
    let mut m = 0u8;
    if flags.contains(CGEventFlags::CGEventFlagShift) {
        m |= 0x01;
    }
    if flags.contains(CGEventFlags::CGEventFlagControl) {
        m |= 0x02;
    }
    if flags.contains(CGEventFlags::CGEventFlagAlternate) {
        m |= 0x04;
    }
    if flags.contains(CGEventFlags::CGEventFlagCommand) {
        m |= 0x08;
    }
    m
}
