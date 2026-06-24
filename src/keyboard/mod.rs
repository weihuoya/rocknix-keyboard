// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026-present ROCKNIX (https://github.com/ROCKNIX)

//! On-screen keyboard UI for `rocknix-keyboard`.

use gpui::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Size,
    StatefulInteractiveElement, Styled as _, TouchEvent, TouchPhase, Window, div, px,
};
use parking_lot::Mutex;
use std::sync::Arc;

use crate::keyboard::backend::KeyboardBackend;
use gpui_component::{
    ActiveTheme as _, Selectable as _, button::Button, button::ButtonVariants as _, h_flex, v_flex,
};

pub mod backend;
pub mod keycodes;
pub mod wayland;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyboardPage {
    Letters,
    Numbers,
    Symbols,
}

#[derive(Clone, Debug)]
pub enum KeyAction {
    /// Insert a single Unicode character.
    Char(char),
    /// Insert a literal string.
    Text(&'static str),
    /// Backspace key.
    Backspace,
    /// Return/Enter key.
    Enter,
    /// Space bar.
    Space,
    /// Toggle shift state.
    Shift,
    /// Switch to another page.
    SwitchPage(KeyboardPage),
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct KeyboardState {
    pub page: KeyboardPage,
    pub shifted: bool,
    pub position: Point<Pixels>,
    /// Layer-shell margin: top, right, bottom, left.
    pub margin: (Pixels, Pixels, Pixels, Pixels),
    /// Cached screen size used for drag clamping. `primary_display()` is not
    /// implemented on Wayland, so we capture it once at startup from the first
    /// available display (or fall back to a sensible default).
    pub screen_size: Size<Pixels>,
    pub dragging: bool,
    pub drag_touch_id: Option<i32>,
    pub drag_start_touch: Point<Pixels>,
    pub drag_start_margin: (Pixels, Pixels, Pixels, Pixels),
}

#[allow(dead_code)]
impl KeyboardState {
    pub fn new(window: &Window) -> Self {
        let size = window.viewport_size();
        let width = px(720.);
        Self {
            page: KeyboardPage::Letters,
            shifted: false,
            position: gpui::point(size.width / 2. - width / 2., size.height - px(360.)),
            margin: (px(0.), px(0.), px(0.), px(0.)),
            screen_size: size,
            dragging: false,
            drag_touch_id: None,
            drag_start_touch: Point::default(),
            drag_start_margin: (px(0.), px(0.), px(0.), px(0.)),
        }
    }

    pub fn toggle_shift(&mut self) {
        self.shifted = !self.shifted;
    }

    pub fn set_page(&mut self, page: KeyboardPage) {
        self.page = page;
    }
}

pub struct KeyDef {
    pub label: &'static str,
    pub action: KeyAction,
    pub width: Option<Pixels>,
}

impl KeyDef {
    fn char(label: &'static str, action: char) -> Self {
        Self {
            label,
            action: KeyAction::Char(action),
            width: None,
        }
    }

    fn text(label: &'static str, action: &'static str) -> Self {
        Self {
            label,
            action: KeyAction::Text(action),
            width: None,
        }
    }

    fn action(label: &'static str, action: KeyAction, width: Pixels) -> Self {
        Self {
            label,
            action,
            width: Some(width),
        }
    }
}

fn letter_layout(shifted: bool) -> Vec<Vec<KeyDef>> {
    let letters: Vec<Vec<char>> = if shifted {
        vec![
            "QWERTYUIOP".chars().collect(),
            "ASDFGHJKL".chars().collect(),
            "ZXCVBNM".chars().collect(),
        ]
    } else {
        vec![
            "qwertyuiop".chars().collect(),
            "asdfghjkl".chars().collect(),
            "zxcvbnm".chars().collect(),
        ]
    };

    let mut rows: Vec<Vec<KeyDef>> = letters
        .into_iter()
        .map(|row| row.into_iter().map(|c| KeyDef::char("", c)).collect())
        .collect();

    if let Some(last) = rows.last_mut() {
        last.insert(0, KeyDef::action("⇧", KeyAction::Shift, px(64.)));
        last.push(KeyDef::action("⌫", KeyAction::Backspace, px(64.)));
    }

    rows.push(vec![
        KeyDef::action("123", KeyAction::SwitchPage(KeyboardPage::Numbers), px(64.)),
        KeyDef::action(",", KeyAction::Char(','), px(48.)),
        KeyDef::action("space", KeyAction::Space, px(320.)),
        KeyDef::text(".", "."),
        KeyDef::action("return", KeyAction::Enter, px(80.)),
    ]);

    rows
}

fn number_layout() -> Vec<Vec<KeyDef>> {
    vec![
        "1234567890".chars().map(|c| KeyDef::char("", c)).collect(),
        vec![
            KeyDef::text("-", "-"),
            KeyDef::text("/", "/"),
            KeyDef::text(":", ":"),
            KeyDef::text("_", "_"),
            KeyDef::text("(", "("),
            KeyDef::text(")", ")"),
            KeyDef::text("#", "#"),
            KeyDef::text("&", "&"),
            KeyDef::text("@", "@"),
            KeyDef::text("\"", "\""),
        ],
        vec![
            KeyDef::action("#+=", KeyAction::SwitchPage(KeyboardPage::Symbols), px(64.)),
            KeyDef::text(".", "."),
            KeyDef::text(",", ","),
            KeyDef::text("?", "?"),
            KeyDef::text("!", "!"),
            KeyDef::text("…", "…"),
            KeyDef::text("'", "'"),
            KeyDef::action("⌫", KeyAction::Backspace, px(64.)),
        ],
        vec![
            KeyDef::action("ABC", KeyAction::SwitchPage(KeyboardPage::Letters), px(64.)),
            KeyDef::text("/", "/"),
            KeyDef::action("space", KeyAction::Space, px(320.)),
            KeyDef::text(".", "."),
            KeyDef::action("return", KeyAction::Enter, px(80.)),
        ],
    ]
}

fn symbol_layout() -> Vec<Vec<KeyDef>> {
    vec![
        vec![
            KeyDef::text("[", "["),
            KeyDef::text("]", "]"),
            KeyDef::text("{", "{"),
            KeyDef::text("}", "}"),
            KeyDef::text("÷", "÷"),
            KeyDef::text("%", "%"),
            KeyDef::text("^", "^"),
            KeyDef::text("*", "*"),
            KeyDef::text("+", "+"),
            KeyDef::text("=", "="),
        ],
        vec![
            KeyDef::text("\\", "\\"),
            KeyDef::text("|", "|"),
            KeyDef::text(";", ";"),
            KeyDef::text("¢", "¢"),
            KeyDef::text("฿", "฿"),
            KeyDef::text("€", "€"),
            KeyDef::text("£", "£"),
            KeyDef::text("$", "$"),
            KeyDef::text("¥", "¥"),
            KeyDef::text("·", "·"),
        ],
        vec![
            KeyDef::action("123", KeyAction::SwitchPage(KeyboardPage::Numbers), px(64.)),
            KeyDef::text("<", "<"),
            KeyDef::text(">", ">"),
            KeyDef::text("♀", "♀"),
            KeyDef::text("♂", "♂"),
            KeyDef::text("→", "→"),
            KeyDef::text("~", "~"),
            KeyDef::action("⌫", KeyAction::Backspace, px(64.)),
        ],
        vec![
            KeyDef::action("ABC", KeyAction::SwitchPage(KeyboardPage::Letters), px(64.)),
            KeyDef::text("/", "/"),
            KeyDef::action("space", KeyAction::Space, px(320.)),
            KeyDef::text(".", "."),
            KeyDef::action("return", KeyAction::Enter, px(80.)),
        ],
    ]
}

pub fn keyboard_layout(state: &KeyboardState) -> Vec<Vec<KeyDef>> {
    match state.page {
        KeyboardPage::Letters => letter_layout(state.shifted),
        KeyboardPage::Numbers => number_layout(),
        KeyboardPage::Symbols => symbol_layout(),
    }
}

/// Compute the desired window width for the current keyboard page so that the
/// key panel fits snugly without stretching across the entire screen.
pub fn keyboard_desired_width(state: &KeyboardState) -> Pixels {
    let layout = keyboard_layout(state);
    let key_size = px(54.);
    let key_gap = px(5.);
    let padding = px(16.); // keys_panel p_2 on both sides

    let mut max_width = px(0.);
    for row in layout {
        let mut row_width = px(0.);
        let key_count = row.len();
        for (idx, key) in row.iter().enumerate() {
            row_width += key.width.unwrap_or(key_size);
            if idx + 1 < key_count {
                row_width += key_gap;
            }
        }
        if row_width > max_width {
            max_width = row_width;
        }
    }

    max_width + padding
}

pub fn render_keyboard<F>(
    state: KeyboardState,
    backend: Arc<Mutex<dyn KeyboardBackend>>,
    on_action: F,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement + 'static
where
    F: Fn(&KeyAction, &mut Window, &mut App) + Clone + 'static,
{
    let theme = cx.theme();
    let layout = keyboard_layout(&state);
    let key_size = px(54.);
    let key_gap = px(5.);

    let state_for_rows = state.clone();
    let key_rows = layout.into_iter().enumerate().map(move |(row_idx, row)| {
        let mut row_flex = h_flex().gap(key_gap).items_center().justify_center();
        for (idx, key) in row.into_iter().enumerate() {
            let width = key.width.unwrap_or(key_size);
            let label = if key.label.is_empty() {
                match &key.action {
                    KeyAction::Char(c) => c.to_string(),
                    KeyAction::Text(s) => s.to_string(),
                    _ => key.label.to_string(),
                }
            } else {
                key.label.to_string()
            };

            // Include the row index so keys in different rows don't share the
            // same GlobalElementId, which breaks GPUI's per-element touch state.
            let id = format!("kbd-key-{}-{}-{}", state_for_rows.page as u8, row_idx, idx);
            let on_action = on_action.clone();
            let action = key.action.clone();

            if matches!(action, KeyAction::Backspace) {
                // Backspace uses a custom div so we can handle touch events for
                // key repeat. Button currently only exposes on_click, which only
                // fires once per touch.
                let backend = backend.clone();
                let backspace_key = div()
                    .id(id.clone())
                    .w(width)
                    .h(key_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(theme.colors.button_secondary)
                    .hover(|style| style.bg(theme.colors.button_secondary_hover))
                    .active(|style| style.bg(theme.colors.button_secondary_active))
                    .text_color(theme.colors.button_secondary_foreground)
                    .child(label)
                    .on_touch_event(move |event: &TouchEvent, window, cx| match event.phase {
                        TouchPhase::Started => {
                            on_action(&KeyAction::Backspace, window, cx);
                            backend.lock().start_repeat(keycodes::KEY_BACKSPACE);
                        }
                        TouchPhase::Ended | TouchPhase::Cancelled => {
                            backend.lock().stop_repeat();
                        }
                        _ => {}
                    });
                row_flex = row_flex.child(backspace_key);
            } else {
                let mut btn = Button::new(id);
                match key.action {
                    KeyAction::Enter => btn = btn.primary(),
                    KeyAction::Shift => {
                        btn = btn.secondary();
                        if state_for_rows.shifted {
                            btn = btn.selected(true);
                        }
                    }
                    KeyAction::SwitchPage(_) => {
                        btn = btn.secondary();
                    }
                    _ => {}
                }
                row_flex = row_flex.child(btn.label(label).w(width).h(key_size).on_click(
                    move |_, window, cx| {
                        on_action(&action, window, cx);
                    },
                ));
            }
        }
        row_flex
    });

    let title_bar = div()
        .id("keyboard-title-bar")
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .px_2()
        .py_1()
        .h(px(36.))
        .bg(theme.tab_bar)
        .border_t_1()
        .border_l_1()
        .border_r_1()
        .border_color(theme.border)
        .rounded_t_md()
        .child(
            div()
                .id("keyboard-drag-handle")
                .w(px(48.))
                .h(px(6.))
                .rounded_md()
                .bg(theme.border),
        );

    let keys_panel = div()
        .id("keyboard-keys")
        .flex()
        .flex_col()
        .gap(key_gap)
        .p_2()
        .bg(theme.popover)
        .border_b_1()
        .border_l_1()
        .border_r_1()
        .border_color(theme.border)
        .rounded_b_md()
        .children(key_rows);

    v_flex()
        .id("on-screen-keyboard")
        .size_full()
        .child(title_bar)
        .child(keys_panel)
}
