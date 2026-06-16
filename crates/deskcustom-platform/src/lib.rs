mod inject;
mod traits;

#[cfg(any(windows, target_os = "macos"))]
mod clipboard;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(windows)]
mod windows;

pub use inject::*;
pub use traits::*;

#[cfg(any(windows, target_os = "macos"))]
pub use clipboard::{read_text, write_text};

#[cfg(target_os = "macos")]
pub use macos::{MacInputCapture, MacInputInject};

#[cfg(windows)]
pub use windows::{WinInputCapture, WinInputInject, cursor_position, screen_width};
