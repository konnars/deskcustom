use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use deskcustom_proto::{KeyAction, MouseButton};
use tracing::warn;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_SCANCODE, MOUSEEVENTF_MOVE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEINPUT, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_MENU, VK_RCONTROL, VK_RSHIFT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetCursorPos, HHOOK, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_MOUSEMOVE,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

use crate::{InputCapture, InputEvent, InputInject, MouseDelta};

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(true);

pub struct WinInputCapture {
    queue: Arc<Mutex<Vec<InputEvent>>>,
    mouse_hook: HHOOK,
    keyboard_hook: HHOOK,
}

pub struct WinInputInject;

impl WinInputCapture {
    pub fn new() -> Result<Self> {
        let queue = Arc::new(Mutex::new(Vec::new()));
        let mouse_queue = queue.clone();
        let keyboard_queue = queue.clone();

        let mouse_hook = unsafe {
            SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                None,
                0,
            )
            .context("install low-level mouse hook")?
        };

        let keyboard_hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                None,
                0,
            )
            .context("install low-level keyboard hook")?
        };

        MOUSE_QUEUE.with(|cell| *cell.borrow_mut() = Some(mouse_queue));
        KEYBOARD_QUEUE.with(|cell| *cell.borrow_mut() = Some(keyboard_queue));

        Ok(Self {
            queue,
            mouse_hook,
            keyboard_hook,
        })
    }

    pub fn set_active(active: bool) {
        CAPTURE_ACTIVE.store(active, Ordering::SeqCst);
    }
}

impl Drop for WinInputCapture {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.mouse_hook);
            let _ = UnhookWindowsHookEx(self.keyboard_hook);
        }
    }
}

impl InputCapture for WinInputCapture {
    fn poll(&mut self) -> Result<Vec<InputEvent>> {
        pump_messages(Duration::from_millis(0));
        let mut guard = self.queue.lock().expect("queue poisoned");
        Ok(std::mem::take(&mut *guard))
    }
}

impl WinInputInject {
    pub fn new() -> Self {
        Self
    }
}

impl InputInject for WinInputInject {
    fn inject(&mut self, event: &InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove(delta) => {
                send_mouse_move(*delta)?;
            }
            InputEvent::MouseButton { button, pressed } => {
                send_mouse_button(button, *pressed)?;
            }
            InputEvent::Key {
                scancode,
                action,
                modifiers: _,
            } => {
                send_key(*scancode, *action)?;
            }
        }
        Ok(())
    }
}

thread_local! {
    static MOUSE_QUEUE: std::cell::RefCell<Option<Arc<Mutex<Vec<InputEvent>>>>> = const { std::cell::RefCell::new(None) };
    static KEYBOARD_QUEUE: std::cell::RefCell<Option<Arc<Mutex<Vec<InputEvent>>>>> = const { std::cell::RefCell::new(None) };
    static LAST_MOUSE: std::cell::RefCell<Option<(i32, i32)>> = const { std::cell::RefCell::new(None) };
    static MOD_STATE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

const MOD_SHIFT: u8 = 0x01;
const MOD_CTRL: u8 = 0x02;
const MOD_ALT: u8 = 0x04;

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && CAPTURE_ACTIVE.load(Ordering::SeqCst) {
        let info = *(lparam.0 as *const MSLLHOOKSTRUCT);
        match wparam.0 as u32 {
            WM_MOUSEMOVE => {
                let mut point = info.pt;
                let current = (point.x, point.y);
                let delta = LAST_MOUSE.with(|cell| {
                    let mut last = cell.borrow_mut();
                    let delta = if let Some(prev) = *last {
                        MouseDelta {
                            dx: current.0 - prev.0,
                            dy: current.1 - prev.1,
                        }
                    } else {
                        MouseDelta { dx: 0, dy: 0 }
                    };
                    *last = Some(current);
                    delta
                });
                if delta.dx != 0 || delta.dy != 0 {
                    enqueue_mouse(InputEvent::MouseMove(delta));
                }
            }
            WM_LBUTTONDOWN => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: true,
            }),
            WM_LBUTTONUP => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Left,
                pressed: false,
            }),
            WM_RBUTTONDOWN => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Right,
                pressed: true,
            }),
            WM_RBUTTONUP => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Right,
                pressed: false,
            }),
            WM_MBUTTONDOWN => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: true,
            }),
            WM_MBUTTONUP => enqueue_mouse(InputEvent::MouseButton {
                button: MouseButton::Middle,
                pressed: false,
            }),
            _ => {}
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && CAPTURE_ACTIVE.load(Ordering::SeqCst) {
        let info = *(lparam.0 as *const KBDLLHOOKSTRUCT);
        let action = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => KeyAction::Down,
            WM_KEYUP | WM_SYSKEYUP => KeyAction::Up,
            _ => return CallNextHookEx(HHOOK::default(), code, wparam, lparam),
        };
        update_modifiers(info.vkCode, action);
        enqueue_keyboard(InputEvent::Key {
            scancode: info.scanCode as u16,
            action,
            modifiers: MOD_STATE.with(|state| state.get()),
        });
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

fn update_modifiers(vk: u32, action: KeyAction) {
    let down = action == KeyAction::Down;
    MOD_STATE.with(|state| {
        let mut mods = state.get();
        let mask = match vk {
            v if v == VK_SHIFT.0 as u32
                || v == VK_LSHIFT.0 as u32
                || v == VK_RSHIFT.0 as u32 =>
            {
                Some(MOD_SHIFT)
            }
            v if v == VK_CONTROL.0 as u32
                || v == VK_LCONTROL.0 as u32
                || v == VK_RCONTROL.0 as u32 =>
            {
                Some(MOD_CTRL)
            }
            v if v == VK_MENU.0 as u32 || v == VK_LMENU.0 as u32 => Some(MOD_ALT),
            _ => None,
        };
        if let Some(bit) = mask {
            if down {
                mods |= bit;
            } else {
                mods &= !bit;
            }
            state.set(mods);
        }
    });
}

fn enqueue_mouse(event: InputEvent) {
    MOUSE_QUEUE.with(|cell| {
        if let Some(queue) = cell.borrow().as_ref() {
            if let Ok(mut guard) = queue.lock() {
                guard.push(event);
            }
        }
    });
}

fn enqueue_keyboard(event: InputEvent) {
    KEYBOARD_QUEUE.with(|cell| {
        if let Some(queue) = cell.borrow().as_ref() {
            if let Ok(mut guard) = queue.lock() {
                guard.push(event);
            }
        }
    });
}

fn pump_messages(timeout: Duration) {
    use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, PM_REMOVE, TranslateMessage, DispatchMessageW, MSG};

    let start = Instant::now();
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, HWND(null_mut()), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if start.elapsed() >= timeout {
                break;
            }
        }
    }
}

fn send_mouse_move(delta: MouseDelta) -> Result<()> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: delta.dx,
                dy: delta.dy,
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_inputs(&[input])
}

fn send_mouse_button(button: &MouseButton, pressed: bool) -> Result<()> {
    let flags = match (button, pressed) {
        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
        _ => {
            warn!("unsupported mouse button");
            return Ok(());
        }
    };
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_inputs(&[input])
}

fn send_key(scancode: u16, action: KeyAction) -> Result<()> {
    let mut flags = KEYEVENTF_SCANCODE;
    if action == KeyAction::Up {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scancode,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_inputs(&[input])
}

fn send_inputs(inputs: &[INPUT]) -> Result<()> {
    let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        anyhow::bail!("SendInput wrote {sent}/{} events", inputs.len());
    }
    Ok(())
}

pub fn cursor_position() -> Result<(i32, i32)> {
    unsafe {
        let mut point = windows::Win32::Foundation::POINT::default();
        GetCursorPos(&mut point).context("GetCursorPos")?;
        Ok((point.x, point.y))
    }
}

pub fn screen_width() -> i32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    unsafe { GetSystemMetrics(SM_CXSCREEN) }
}
