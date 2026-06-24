// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026-present ROCKNIX (https://github.com/ROCKNIX)

//! Abstraction over the system input injection backend used by `rocknix-keyboard`.

/// A modifier mask compatible with XKB / Wayland `modifiers()` request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
}

impl ModifierState {
    /// Return the XKB modifier mask used by `zwp_virtual_keyboard_v1::modifiers`.
    pub fn to_xkb_mask(&self) -> u32 {
        let mut mask = 0u32;
        if self.shift {
            mask |= 1 << 0; // Shift
        }
        if self.ctrl {
            mask |= 1 << 2; // Control
        }
        if self.alt {
            mask |= 1 << 3; // Mod1 (Alt)
        }
        if self.logo {
            mask |= 1 << 6; // Mod4 (Super/Logo)
        }
        mask
    }
}

/// Backend capable of injecting key events into the system.
pub trait KeyboardBackend: Send + 'static {
    /// Inject a key press.
    fn press(&mut self, keycode: u32);
    /// Inject a key release.
    fn release(&mut self, keycode: u32);
    /// Notify the compositor that the modifier state has changed.
    fn set_modifiers(&mut self, modifiers: ModifierState);
    /// Inject a character that has no direct physical key mapping by uploading a
    /// temporary keymap containing only that character.
    fn send_character(&mut self, ch: char);
    /// Start auto-repeating the given keycode while it is held down.
    fn start_repeat(&mut self, keycode: u32);
    /// Stop any active key repeat.
    fn stop_repeat(&mut self);
}
