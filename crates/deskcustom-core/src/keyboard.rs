use deskcustom_config::{ClipboardConfig, KeyboardConfig};
use deskcustom_platform::InputEvent;
use deskcustom_proto::KeyAction;

pub const MOD_SHIFT: u8 = 0x01;
pub const MOD_CTRL: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_CMD: u8 = 0x08;

pub const SC_LEFT_SHIFT: u16 = 0xE1;
pub const SC_RIGHT_SHIFT: u16 = 0xE5;
pub const SC_LEFT_ALT: u16 = 0xE2;
pub const SC_RIGHT_ALT: u16 = 0xE6;
pub const SC_LEFT_CTRL: u16 = 0xE0;
pub const SC_RIGHT_CTRL: u16 = 0xE4;

/// Windows Set 1 scancodes from the server keyboard.
pub const WIN_SC_CTRL: u16 = 0x001D;
pub const WIN_SC_SHIFT_L: u16 = 0x002A;
pub const WIN_SC_SHIFT_R: u16 = 0x0036;
pub const WIN_SC_ALT: u16 = 0x0038;
pub const WIN_SC_A: u16 = 0x001E;
pub const WIN_SC_C: u16 = 0x002E;
pub const WIN_SC_V: u16 = 0x002F;
pub const WIN_SC_X: u16 = 0x002D;

/// macOS virtual keycodes for inject.
pub const MAC_KC_A: u16 = 0;
pub const MAC_KC_C: u16 = 8;
pub const MAC_KC_V: u16 = 9;
pub const MAC_KC_X: u16 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardTrigger {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
pub struct ProcessedKey {
    pub event: InputEvent,
    pub clipboard: Option<ClipboardTrigger>,
}

pub struct KeyboardPolicy {
    config: KeyboardConfig,
    clipboard: ClipboardConfig,
    alt_held: bool,
    shift_held: bool,
    ctrl_held: bool,
}

impl KeyboardPolicy {
    pub fn new(config: KeyboardConfig, clipboard: ClipboardConfig) -> Self {
        Self {
            config,
            clipboard,
            alt_held: false,
            shift_held: false,
            ctrl_held: false,
        }
    }

    pub fn filter_capture(&mut self, event: InputEvent) -> Option<InputEvent> {
        if let InputEvent::Key {
            scancode,
            action,
            modifiers,
            ..
        } = &event
        {
            self.track_state(*scancode, *action, *modifiers);
            if self.config.alt_shift_policy == "local_only" && self.alt_shift_layout_toggle() {
                return None;
            }
        }
        Some(event)
    }

    pub fn process_inject(&mut self, event: InputEvent) -> Option<ProcessedKey> {
        if let InputEvent::Key {
            scancode,
            action,
            modifiers,
        } = &event
        {
            self.track_state(*scancode, *action, *modifiers);

            if self.config.alt_shift_policy == "local_only" && self.alt_shift_layout_toggle() {
                return None;
            }

            #[cfg(target_os = "macos")]
            if self.config.translate_ctrl_to_cmd_on_mac {
                return Some(self.map_ctrl_shortcuts_for_mac(event));
            }

            let clipboard = self.clipboard_trigger(*scancode, *action, *modifiers);
            return Some(ProcessedKey { event, clipboard });
        }

        Some(ProcessedKey {
            event,
            clipboard: None,
        })
    }

    pub fn clipboard_after_local_key(&mut self, event: &InputEvent) -> Option<ClipboardTrigger> {
        if !self.clipboard.enabled || !self.clipboard.sync_on_copy {
            return None;
        }
        let InputEvent::Key {
            scancode,
            action,
            modifiers,
        } = event
        else {
            return None;
        };
        self.track_state(*scancode, *action, *modifiers);
        self.clipboard_trigger(*scancode, *action, *modifiers)
    }

    #[cfg(target_os = "macos")]
    fn map_ctrl_shortcuts_for_mac(&self, event: InputEvent) -> ProcessedKey {
        let InputEvent::Key {
            scancode,
            action,
            modifiers,
        } = event
        else {
            return ProcessedKey {
                event,
                clipboard: None,
            };
        };

        let ctrl = self.ctrl_held || modifiers & MOD_CTRL != 0;
        if !ctrl {
            return ProcessedKey {
                event: InputEvent::Key {
                    scancode,
                    action,
                    modifiers,
                },
                clipboard: None,
            };
        }

        let mapped_scancode = match scancode {
            WIN_SC_C | MAC_KC_C => MAC_KC_C,
            WIN_SC_V | MAC_KC_V => MAC_KC_V,
            WIN_SC_X | MAC_KC_X => MAC_KC_X,
            WIN_SC_A | MAC_KC_A => MAC_KC_A,
            other => other,
        };

        let mapped_mods = (modifiers & !MOD_CTRL) | MOD_CMD;
        let clipboard = match (scancode, action) {
            (WIN_SC_C, KeyAction::Up) | (MAC_KC_C, KeyAction::Up) => Some(ClipboardTrigger::Copy),
            (WIN_SC_X, KeyAction::Up) | (MAC_KC_X, KeyAction::Up) => Some(ClipboardTrigger::Cut),
            _ => None,
        };

        ProcessedKey {
            event: InputEvent::Key {
                scancode: mapped_scancode,
                action,
                modifiers: mapped_mods,
            },
            clipboard,
        }
    }

    fn clipboard_trigger(
        &self,
        scancode: u16,
        action: KeyAction,
        modifiers: u8,
    ) -> Option<ClipboardTrigger> {
        if !self.clipboard.enabled || !self.clipboard.sync_on_copy {
            return None;
        }
        if action != KeyAction::Up {
            return None;
        }
        let ctrl = self.ctrl_held || modifiers & MOD_CTRL != 0;
        if !ctrl {
            return None;
        }
        match scancode {
            WIN_SC_C | MAC_KC_C => Some(ClipboardTrigger::Copy),
            WIN_SC_X | MAC_KC_X => Some(ClipboardTrigger::Cut),
            _ => None,
        }
    }

    fn track_state(&mut self, scancode: u16, action: KeyAction, modifiers: u8) {
        let down = action == KeyAction::Down;
        match scancode {
            SC_LEFT_SHIFT | SC_RIGHT_SHIFT | WIN_SC_SHIFT_L | WIN_SC_SHIFT_R => {
                self.shift_held = down
            }
            SC_LEFT_ALT | SC_RIGHT_ALT | WIN_SC_ALT => self.alt_held = down,
            SC_LEFT_CTRL | SC_RIGHT_CTRL | WIN_SC_CTRL => self.ctrl_held = down,
            _ => {
                if modifiers & MOD_CTRL != 0 && down {
                    self.ctrl_held = true;
                }
            }
        }
    }

    fn alt_shift_layout_toggle(&self) -> bool {
        self.alt_held && self.shift_held
    }
}
