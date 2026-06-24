# ROCKNIX Keyboard

A GPUI-based on-screen keyboard for ROCKNIX handhelds. It renders as a
layer-shell panel and injects key events through the Wayland
`zwp_virtual_keyboard_v1` protocol, so it works with any focused application
just like a physical keyboard.

## Features (first iteration)

- QWERTY letter layout
- Number and symbol pages
- Shift modifier
- Backspace / Enter / Space
- Wayland `zwp_virtual_keyboard_v1` input injection

## Building

```bash
cargo build --release
```

The resulting binary is `target/release/rocknix-keyboard`.

## Running locally (on a Wayland compositor)

```bash
./target/release/rocknix-keyboard
```

The keyboard will appear as a bottom panel. Clicking a key will send the
corresponding event to the currently focused application.

## ROCKNIX integration

See `projects/ROCKNIX/packages/apps/rocknix-keyboard-gpui/` in the ROCKNIX
distribution repository for the package definition and systemd service.

## Architecture

- `src/keyboard/mod.rs` — keyboard state, layouts, and GPUI rendering
- `src/keyboard/backend.rs` — generic input injection backend trait
- `src/keyboard/wayland.rs` — Wayland `zwp_virtual_keyboard_v1` backend
- `src/keyboard/keycodes.rs` — Linux input event code mapping
- `src/main.rs` — layer-shell window setup and action routing

The Wayland virtual-keyboard connection runs on its own thread so that it does
not interfere with GPUI's internal Wayland event loop.

## License

GPL-3.0-or-later
