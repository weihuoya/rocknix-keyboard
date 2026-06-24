// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026-present ROCKNIX (https://github.com/ROCKNIX)

//! Mapping from logical keys to Linux input event codes.
//!
//! These constants mirror `<linux/input-event-codes.h>`.  XKB keycodes are the
//! event code plus 8 (the X11/XKB legacy offset).

#![allow(dead_code)]

pub const KEY_ESC: u32 = 1;
pub const KEY_1: u32 = 2;
pub const KEY_2: u32 = 3;
pub const KEY_3: u32 = 4;
pub const KEY_4: u32 = 5;
pub const KEY_5: u32 = 6;
pub const KEY_6: u32 = 7;
pub const KEY_7: u32 = 8;
pub const KEY_8: u32 = 9;
pub const KEY_9: u32 = 10;
pub const KEY_0: u32 = 11;
pub const KEY_MINUS: u32 = 12;
pub const KEY_EQUAL: u32 = 13;
pub const KEY_BACKSPACE: u32 = 14;
pub const KEY_TAB: u32 = 15;
pub const KEY_Q: u32 = 16;
pub const KEY_W: u32 = 17;
pub const KEY_E: u32 = 18;
pub const KEY_R: u32 = 19;
pub const KEY_T: u32 = 20;
pub const KEY_Y: u32 = 21;
pub const KEY_U: u32 = 22;
pub const KEY_I: u32 = 23;
pub const KEY_O: u32 = 24;
pub const KEY_P: u32 = 25;
pub const KEY_LEFTBRACE: u32 = 26;
pub const KEY_RIGHTBRACE: u32 = 27;
pub const KEY_ENTER: u32 = 28;
pub const KEY_LEFTCTRL: u32 = 29;
pub const KEY_A: u32 = 30;
pub const KEY_S: u32 = 31;
pub const KEY_D: u32 = 32;
pub const KEY_F: u32 = 33;
pub const KEY_G: u32 = 34;
pub const KEY_H: u32 = 35;
pub const KEY_J: u32 = 36;
pub const KEY_K: u32 = 37;
pub const KEY_L: u32 = 38;
pub const KEY_SEMICOLON: u32 = 39;
pub const KEY_APOSTROPHE: u32 = 40;
pub const KEY_GRAVE: u32 = 41;
pub const KEY_LEFTSHIFT: u32 = 42;
pub const KEY_BACKSLASH: u32 = 43;
pub const KEY_Z: u32 = 44;
pub const KEY_X: u32 = 45;
pub const KEY_C: u32 = 46;
pub const KEY_V: u32 = 47;
pub const KEY_B: u32 = 48;
pub const KEY_N: u32 = 49;
pub const KEY_M: u32 = 50;
pub const KEY_COMMA: u32 = 51;
pub const KEY_DOT: u32 = 52;
pub const KEY_SLASH: u32 = 53;
pub const KEY_RIGHTSHIFT: u32 = 54;
pub const KEY_KPASTERISK: u32 = 55;
pub const KEY_LEFTALT: u32 = 56;
pub const KEY_SPACE: u32 = 57;
pub const KEY_CAPSLOCK: u32 = 58;
pub const KEY_LEFTMETA: u32 = 125;

/// Translate a printable character to the Linux event code used by the US
/// layout.  This is sufficient for the basic QWERTY page implemented in the
/// first iteration.
pub fn char_to_event_code(ch: char) -> Option<u32> {
    match ch {
        'a' | 'A' => Some(KEY_A),
        'b' | 'B' => Some(KEY_B),
        'c' | 'C' => Some(KEY_C),
        'd' | 'D' => Some(KEY_D),
        'e' | 'E' => Some(KEY_E),
        'f' | 'F' => Some(KEY_F),
        'g' | 'G' => Some(KEY_G),
        'h' | 'H' => Some(KEY_H),
        'i' | 'I' => Some(KEY_I),
        'j' | 'J' => Some(KEY_J),
        'k' | 'K' => Some(KEY_K),
        'l' | 'L' => Some(KEY_L),
        'm' | 'M' => Some(KEY_M),
        'n' | 'N' => Some(KEY_N),
        'o' | 'O' => Some(KEY_O),
        'p' | 'P' => Some(KEY_P),
        'q' | 'Q' => Some(KEY_Q),
        'r' | 'R' => Some(KEY_R),
        's' | 'S' => Some(KEY_S),
        't' | 'T' => Some(KEY_T),
        'u' | 'U' => Some(KEY_U),
        'v' | 'V' => Some(KEY_V),
        'w' | 'W' => Some(KEY_W),
        'x' | 'X' => Some(KEY_X),
        'y' | 'Y' => Some(KEY_Y),
        'z' | 'Z' => Some(KEY_Z),
        '1' => Some(KEY_1),
        '2' => Some(KEY_2),
        '3' => Some(KEY_3),
        '4' => Some(KEY_4),
        '5' => Some(KEY_5),
        '6' => Some(KEY_6),
        '7' => Some(KEY_7),
        '8' => Some(KEY_8),
        '9' => Some(KEY_9),
        '0' => Some(KEY_0),
        '-' | '_' => Some(KEY_MINUS),
        '=' | '+' => Some(KEY_EQUAL),
        '*' => Some(KEY_8),
        '[' | '{' => Some(KEY_LEFTBRACE),
        ']' | '}' => Some(KEY_RIGHTBRACE),
        '\\' | '|' => Some(KEY_BACKSLASH),
        ';' | ':' => Some(KEY_SEMICOLON),
        '\'' | '"' => Some(KEY_APOSTROPHE),
        '`' | '~' => Some(KEY_GRAVE),
        ',' | '<' => Some(KEY_COMMA),
        '.' | '>' => Some(KEY_DOT),
        '/' | '?' => Some(KEY_SLASH),
        ' ' => Some(KEY_SPACE),
        '%' => Some(KEY_5),
        '^' => Some(KEY_6),
        '$' => Some(KEY_4),
        _ => None,
    }
}

/// Returns true for characters that require the Shift modifier on a standard
/// US QWERTY layout. These are pressed with shift held and released before
/// shift is restored.
pub fn char_needs_shift(ch: char) -> bool {
    matches!(
        ch,
        '!' | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '('
            | ')'
            | '_'
            | '+'
            | '{'
            | '}'
            | '|'
            | ':'
            | '"'
            | '<'
            | '>'
            | '?'
            | '~'
    )
}

/// Translate a short text label to the primary event code.  Used for keys whose
/// label is not a single ASCII character.
pub fn text_to_event_code(text: &str) -> Option<u32> {
    match text {
        "/" => Some(KEY_SLASH),
        "," => Some(KEY_COMMA),
        "." => Some(KEY_DOT),
        _ => text.chars().next().and_then(char_to_event_code),
    }
}
