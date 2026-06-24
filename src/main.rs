// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026-present ROCKNIX (https://github.com/ROCKNIX)

//! Standalone on-screen keyboard for ROCKNIX.
//!
//! Renders a GPUI layer-shell panel and injects key events through the Wayland
//! `zwp_virtual_keyboard_v1` protocol, making it usable by any application.

use std::sync::Arc;

use gpui::{
    App, Bounds, Context, DispatchPhase, Entity, EntityId, Global, Render, Size, TouchEvent,
    TouchPhase, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    layer_shell::*, point, prelude::*, px,
};
use gpui_platform::application;
use parking_lot::Mutex;

use crate::keyboard::keyboard_desired_width;
use crate::keyboard::{
    KeyboardPage, KeyboardState,
    backend::{KeyboardBackend, ModifierState},
    keycodes,
    wayland::WaylandVirtualKeyboard,
};

mod keyboard;

const TITLE_BAR_HEIGHT: f32 = 36.0;
const KEYBOARD_HEIGHT: f32 = 280.0;

#[allow(dead_code)]
struct KeyboardGlobals {
    state: Entity<KeyboardState>,
    backend: Arc<Mutex<dyn KeyboardBackend>>,
}

impl Global for KeyboardGlobals {}

struct KeyboardApp {
    state: Entity<KeyboardState>,
    backend: Arc<Mutex<dyn KeyboardBackend>>,
}

impl Render for KeyboardApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx).clone();
        let backend = self.backend.clone();
        let state_entity = self.state.clone();
        let view_id = cx.entity_id();

        // Title-bar drag support: move the layer-shell window by adjusting its
        // bottom margin (and, on Wayland, margin changes take effect after the
        // next surface commit).
        let state_for_touch = self.state.clone();
        window.on_touch_event(move |event: &TouchEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }

            let mut handled = false;
            match event.phase {
                TouchPhase::Started => {
                    let in_title_bar =
                        event.position.y >= px(0.) && event.position.y <= px(TITLE_BAR_HEIGHT);
                    if in_title_bar {
                        state_for_touch.update(cx, |state, _cx| {
                            state.dragging = true;
                            state.drag_touch_id = Some(event.id);
                            state.drag_start_touch = event.position;
                            state.drag_start_margin = state.margin;
                        });
                        handled = true;
                    }
                }
                TouchPhase::Moved => {
                    // Read drag state, drop the borrow, then query display bounds
                    // (which also needs cx) before writing the new margin.
                    let (dragging, drag_touch_id, drag_start_touch, drag_start_margin) = {
                        let state = state_for_touch.read(cx);
                        (
                            state.dragging,
                            state.drag_touch_id,
                            state.drag_start_touch,
                            state.drag_start_margin,
                        )
                    };
                    if dragging && drag_touch_id == Some(event.id) {
                        let delta_x = event.position.x - drag_start_touch.x;
                        let delta_y = event.position.y - drag_start_touch.y;

                        // Dragging up (negative delta_y) increases bottom margin and
                        // moves the keyboard up.  Dragging left (negative delta_x)
                        // increases left margin and moves the keyboard right.
                        // Use the cached screen size; querying `cx.primary_display()`
                        // inside the touch callback returns None on Wayland.
                        let keyboard_size = window.viewport_size();
                        let screen_size = state_for_touch.read(cx).screen_size;
                        let max_left = (screen_size.width - keyboard_size.width).max(px(0.));
                        let max_bottom = (screen_size.height - keyboard_size.height).max(px(0.));

                        let new_left = (drag_start_margin.3 + delta_x).clamp(px(0.), max_left);
                        let new_bottom = (drag_start_margin.2 - delta_y).clamp(px(0.), max_bottom);

                        state_for_touch.update(cx, |state, _cx| {
                            state.margin = (px(0.), px(0.), new_bottom, new_left);
                        });
                        window.set_layer_shell_margin((px(0.), px(0.), new_bottom, new_left));
                        handled = true;
                    }
                }
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    let was_dragging = state_for_touch.read(cx).drag_touch_id == Some(event.id);
                    if was_dragging {
                        state_for_touch.update(cx, |state, _cx| {
                            state.dragging = false;
                            state.drag_touch_id = None;
                        });
                        handled = true;
                    }
                }
            }

            if handled {
                cx.notify(view_id);
                cx.stop_propagation();
            }
        });

        div().size_full().child(crate::keyboard::render_keyboard(
            state,
            backend.clone(),
            move |action, _window, cx| {
                handle_action(action, state_entity.clone(), view_id, &backend, cx);
            },
            window,
            cx,
        ))
    }
}

fn handle_action(
    action: &crate::keyboard::KeyAction,
    state_entity: Entity<KeyboardState>,
    view_id: EntityId,
    backend: &Arc<Mutex<dyn KeyboardBackend>>,
    cx: &mut App,
) {
    use crate::keyboard::KeyAction;

    match action {
        KeyAction::Shift => {
            let shifted = state_entity.update(cx, |state, _cx| {
                state.toggle_shift();
                state.shifted
            });
            backend.lock().set_modifiers(ModifierState {
                shift: shifted,
                ..Default::default()
            });
            cx.notify(view_id);
        }
        KeyAction::SwitchPage(page) => {
            let page = *page;
            state_entity.update(cx, |state, _cx| state.set_page(page));
            cx.notify(view_id);
        }
        KeyAction::Char(ch) => {
            let shifted = state_entity.read(cx).shifted;
            send_key_press(*ch, shifted, backend);
        }
        KeyAction::Text(text) => {
            let shifted = state_entity.read(cx).shifted;
            for ch in text.chars() {
                send_key_press(ch, shifted, backend);
            }
        }
        KeyAction::Space => {
            send_physical_key(keycodes::KEY_SPACE, backend);
        }
        KeyAction::Enter => {
            send_physical_key(keycodes::KEY_ENTER, backend);
        }
        KeyAction::Backspace => {
            send_physical_key(keycodes::KEY_BACKSPACE, backend);
        }
    }
}

/// Send a printable character.  If the character maps to a physical key we use
/// that keycode; otherwise we fall back to a direct keysym (not implemented in
/// the first iteration).
fn send_key_press(ch: char, shifted: bool, backend: &Arc<Mutex<dyn KeyboardBackend>>) {
    // Uppercase letters and many US-layout symbols require shift held while the
    // key is pressed. If the keyboard is already shifted we leave the modifier
    // state alone; otherwise we press shift for this key and restore afterwards.
    let needs_shift = ch.is_ascii_uppercase() || keycodes::char_needs_shift(ch);
    let effective_shift = shifted || needs_shift;

    if effective_shift != shifted {
        backend.lock().set_modifiers(ModifierState {
            shift: effective_shift,
            ..Default::default()
        });
    }

    if let Some(code) = keycodes::char_to_event_code(ch) {
        send_physical_key(code, backend);
    } else {
        backend.lock().send_character(ch);
    }

    if effective_shift != shifted {
        backend.lock().set_modifiers(ModifierState {
            shift: shifted,
            ..Default::default()
        });
    }
}

fn send_physical_key(code: u32, backend: &Arc<Mutex<dyn KeyboardBackend>>) {
    // `zwp_virtual_keyboard_v1::key` expects Linux evdev scancodes (the same
    // numbering used by `wl_keyboard.key` events), not XKB keycodes which are
    // evdev code + 8.
    let mut backend = backend.lock();
    backend.press(code);
    backend.release(code);
}

fn main() {
    tracing_subscriber::fmt::init();
    let backend = match WaylandVirtualKeyboard::new() {
        Ok(b) => Arc::new(Mutex::new(b)) as Arc<Mutex<dyn KeyboardBackend>>,
        Err(err) => {
            eprintln!("failed to initialize Wayland virtual keyboard: {err:#}");
            std::process::exit(1);
        }
    };

    application().run(move |cx: &mut App| {
        gpui_component::init(cx);

        // Capture the screen size once at startup. On Wayland `primary_display()`
        // returns None, so use the first display from `displays()` and fall back
        // to a 1280x720 default for the AYANEO Pocket S 2K.
        let screen_size = cx
            .displays()
            .first()
            .map(|d| d.bounds().size)
            .unwrap_or_else(|| Size::new(px(1280.), px(720.)));

        let state = cx.new(|_cx| KeyboardState {
            page: KeyboardPage::Letters,
            shifted: false,
            position: point(px(0.), px(0.)),
            margin: (px(0.), px(0.), px(0.), px(0.)),
            screen_size,
            dragging: false,
            drag_touch_id: None,
            drag_start_touch: point(px(0.), px(0.)),
            drag_start_margin: (px(0.), px(0.), px(0.), px(0.)),
        });

        cx.set_global(KeyboardGlobals {
            state,
            backend: backend.clone(),
        });

        let state_for_window = cx.global::<KeyboardGlobals>().state.clone();
        let keyboard_width = keyboard_desired_width(&state_for_window.read(cx));
        let window_size = Size::new(keyboard_width, px(KEYBOARD_HEIGHT));

        // Center the keyboard horizontally on the cached screen size.
        let initial_left = ((screen_size.width - keyboard_width) / 2.0).max(px(0.));
        state_for_window.update(cx, |state, _cx| {
            state.margin.3 = initial_left;
        });

        cx.open_window(
            WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: window_size,
                })),
                app_id: Some("rocknix-keyboard".to_string()),
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::LayerShell(LayerShellOptions {
                    namespace: "rocknix-keyboard".to_string(),
                    layer: Layer::Overlay,
                    // Anchor to the left and bottom so horizontal margin directly
                    // controls the horizontal position.
                    anchor: Anchor::LEFT | Anchor::BOTTOM,
                    exclusive_zone: None,
                    margin: Some((px(0.), px(0.), px(0.), initial_left)),
                    keyboard_interactivity: KeyboardInteractivity::None,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| {
                cx.new(|_cx| KeyboardApp {
                    state: state_for_window,
                    backend: backend.clone(),
                })
            },
        )
        .unwrap();
    });
}
