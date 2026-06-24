// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026-present ROCKNIX (https://github.com/ROCKNIX)

//! Wayland `zwp_virtual_keyboard_v1` backend.
//!
//! This module runs its own `wayland-client` connection on a dedicated thread
//! so that it does not interfere with GPUI's internal Wayland event loop.

use std::{
    fs::File,
    io::{Seek, Write},
    os::fd::{AsFd, FromRawFd, OwnedFd},
    path::PathBuf,
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_keyboard, wl_registry, wl_seat},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use xkbcommon::xkb;

use super::backend::{KeyboardBackend, ModifierState};

/// Events sent from the GPUI thread to the Wayland backend thread.
#[derive(Clone, Debug)]
enum BackendRequest {
    Press(u32),
    Release(u32),
    Modifiers(ModifierState),
    Character(char),
    StartRepeat(u32),
    StopRepeat,
}

/// A handle to the running Wayland virtual-keyboard backend.
pub struct WaylandVirtualKeyboard {
    sender: Sender<BackendRequest>,
    _thread: JoinHandle<()>,
}

impl WaylandVirtualKeyboard {
    /// Start the backend thread and wait until the virtual keyboard is ready.
    pub fn new() -> anyhow::Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<Sender<BackendRequest>>>();

        let thread = thread::spawn(move || {
            let result = Self::run_backend_thread();
            let _ = ready_tx.send(result);
        });

        let sender = ready_rx.recv().map_err(|_| {
            anyhow::anyhow!("Wayland virtual-keyboard backend thread died during startup")
        })??;
        Ok(Self {
            sender,
            _thread: thread,
        })
    }

    fn run_backend_thread() -> anyhow::Result<Sender<BackendRequest>> {
        let conn = Connection::connect_to_env()?;

        let (globals, mut event_queue) = registry_queue_init(&conn)?;
        let qh = event_queue.handle();

        // Find the first advertised wl_seat and the virtual-keyboard manager.
        let globals_list = globals.contents().clone_list();

        let seat_global = globals_list
            .iter()
            .find(|g| g.interface == "wl_seat")
            .ok_or_else(|| anyhow::anyhow!("no wl_seat global found"))?;
        let vk_manager_global = globals_list
            .iter()
            .find(|g| g.interface == "zwp_virtual_keyboard_manager_v1")
            .ok_or_else(|| anyhow::anyhow!("zwp_virtual_keyboard_manager_v1 is not supported"))?;

        let seat = globals.registry().bind::<wl_seat::WlSeat, _, _>(
            seat_global.name,
            seat_global.version,
            &qh,
            (),
        );
        let vk_manager = globals
            .registry()
            .bind::<ZwpVirtualKeyboardManagerV1, _, _>(
                vk_manager_global.name,
                vk_manager_global.version,
                &qh,
                (),
            );

        let vk = vk_manager.create_virtual_keyboard(&seat, &qh, ());

        // Roundtrip once so the compositor processes the keymap before any key
        // events are sent.
        // Upload the US keymap before sending any key events and keep a copy so
        // we can restore it after sending characters through temporary keymaps.
        let us_keymap = upload_keymap(&vk)?;
        let mut temp_state = WaylandState {
            seat: Some(seat.clone()),
            vk_manager: Some(vk_manager.clone()),
            vk: Some(vk.clone()),
            us_keymap: Some(us_keymap.clone()),
        };
        if event_queue.roundtrip(&mut temp_state).is_err() {
            anyhow::bail!("failed to roundtrip after uploading keymap");
        }

        let (tx, rx) = mpsc::channel::<BackendRequest>();

        // Spawn a helper thread that pumps the Wayland event queue and applies
        // incoming key requests.  Keeping dispatch on the same thread that owns
        // the Wayland objects is the simplest ownership model.
        thread::spawn(move || {
            let mut state = WaylandState {
                seat: Some(seat),
                vk_manager: Some(vk_manager),
                vk: Some(vk),
                us_keymap: Some(us_keymap),
            };
            let start_time = std::time::Instant::now();
            let mut current_mods = ModifierState::default();
            let mut repeat_state: Option<(u32, std::time::Instant, bool)> = None;
            const REPEAT_INITIAL_DELAY: std::time::Duration = std::time::Duration::from_millis(400);
            const REPEAT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(80);

            // The virtual-keyboard connection rarely receives events, so a
            // blocking dispatch would stall key requests.  Poll the Wayland fd
            // with a short timeout so channel requests are handled promptly.
            loop {
                // Drain pending Wayland events.
                if event_queue.dispatch_pending(&mut state).is_err() {
                    break;
                }

                // Apply all pending backend requests.
                if let Some(vk) = state.vk.as_ref() {
                    while let Ok(req) = rx.try_recv() {
                        let time = start_time.elapsed().as_millis() as u32;
                        match req {
                            BackendRequest::Press(code) => {
                                let mask = current_mods.to_xkb_mask();
                                vk.modifiers(mask, 0, 0, 0);
                                vk.key(time, code, wl_keyboard::KeyState::Pressed as u32);
                            }
                            BackendRequest::Release(code) => {
                                vk.key(time, code, wl_keyboard::KeyState::Released as u32);
                            }
                            BackendRequest::Modifiers(mods) => {
                                current_mods = mods;
                                let mask = current_mods.to_xkb_mask();
                                vk.modifiers(mask, 0, 0, 0);
                            }
                            BackendRequest::Character(ch) => {
                                let Some(us_keymap) = state.us_keymap.as_ref() else {
                                    continue;
                                };
                                let temp_keymap = build_character_keymap(ch);
                                if upload_keymap_bytes(vk, &temp_keymap).is_err() {
                                    continue;
                                }
                                vk.modifiers(0, 0, 0, 0);
                                // XKB keycode 100 -> Linux evdev scancode 92.
                                vk.key(time, 92, wl_keyboard::KeyState::Pressed as u32);
                                vk.key(time, 92, wl_keyboard::KeyState::Released as u32);
                                let _ = upload_keymap_bytes(vk, us_keymap);
                            }
                            BackendRequest::StartRepeat(code) => {
                                repeat_state = Some((code, std::time::Instant::now(), false));
                            }
                            BackendRequest::StopRepeat => {
                                repeat_state = None;
                            }
                        }
                    }

                    // Auto-repeat handling: after an initial delay, emit repeated
                    // press/release pairs for the held keycode.
                    if let Some((code, last_time, initial_done)) = repeat_state.as_mut() {
                        let threshold = if *initial_done {
                            REPEAT_INTERVAL
                        } else {
                            REPEAT_INITIAL_DELAY
                        };
                        if last_time.elapsed() >= threshold {
                            let time = start_time.elapsed().as_millis() as u32;
                            vk.key(time, *code, wl_keyboard::KeyState::Pressed as u32);
                            vk.key(time, *code, wl_keyboard::KeyState::Released as u32);
                            *last_time = std::time::Instant::now();
                            *initial_done = true;
                        }
                    }
                } else {
                    while let Ok(_req) = rx.try_recv() {}
                }

                // Flush outbound requests so they reach the compositor.
                if event_queue.flush().is_err() {
                    break;
                }

                // Wait a short while for Wayland events; channel requests are
                // processed each iteration, so latency is bounded by this timeout.
                // Create the read guard *before* polling so the backend knows we
                // intend to read from the socket.
                if let Some(guard) = conn.prepare_read() {
                    let fd = guard.connection_fd();
                    let mut poll_fd = rustix::event::PollFd::new(&fd, rustix::event::PollFlags::IN);
                    match rustix::event::poll(
                        std::slice::from_mut(&mut poll_fd),
                        std::time::Duration::from_millis(10).as_millis() as i32,
                    ) {
                        Ok(0) => {
                            // Timeout: drop the guard to cancel the prepared read.
                        }
                        Ok(_) => {
                            if let Err(err) = guard.read() {
                                // Spurious wakeups / no data are fine; keep looping.
                                let is_wouldblock = matches!(
                                    err,
                                    wayland_client::backend::WaylandError::Io(ref io_err)
                                        if io_err.kind() == std::io::ErrorKind::WouldBlock
                                );
                                if !is_wouldblock {
                                    break;
                                }
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                } else {
                    // Another thread filled the inner queue; dispatch it.
                    if conn.backend().dispatch_inner_queue().is_err() {
                        break;
                    }
                }
            }
        });

        Ok(tx)
    }
}

impl KeyboardBackend for WaylandVirtualKeyboard {
    fn press(&mut self, keycode: u32) {
        let _ = self.sender.send(BackendRequest::Press(keycode));
    }

    fn release(&mut self, keycode: u32) {
        let _ = self.sender.send(BackendRequest::Release(keycode));
    }

    fn set_modifiers(&mut self, modifiers: ModifierState) {
        let _ = self.sender.send(BackendRequest::Modifiers(modifiers));
    }

    fn send_character(&mut self, ch: char) {
        let _ = self.sender.send(BackendRequest::Character(ch));
    }

    fn start_repeat(&mut self, keycode: u32) {
        let _ = self.sender.send(BackendRequest::StartRepeat(keycode));
    }

    fn stop_repeat(&mut self) {
        let _ = self.sender.send(BackendRequest::StopRepeat);
    }
}

struct WaylandState {
    seat: Option<wl_seat::WlSeat>,
    vk_manager: Option<ZwpVirtualKeyboardManagerV1>,
    vk: Option<ZwpVirtualKeyboardV1>,
    us_keymap: Option<Vec<u8>>,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_seat" => {
                    let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ());
                    state.seat = Some(seat);
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    let manager =
                        registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(name, version, qh, ());
                    state.vk_manager = Some(manager);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        event: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        event: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

/// Build a default US keymap and return its serialized bytes.
fn build_us_keymap() -> anyhow::Result<Vec<u8>> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
        &context,
        "evdev",
        "pc105",
        "us",
        "",
        None,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or_else(|| anyhow::anyhow!("failed to compile US XKB keymap"))?;

    let keymap_string = keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1);
    let keymap_bytes = keymap_string.into_bytes();
    Ok(keymap_bytes)
}

/// Build an XKB keymap that maps a single virtual key to the given Unicode
/// character. The virtual key uses XKB keycode 100, which corresponds to Linux
/// evdev scancode 92.
fn build_character_keymap(ch: char) -> Vec<u8> {
    let symbol = format!("U{:04X}", ch as u32);
    format!(
        "xkb_keymap {{
        xkb_keycodes {{
            minimum = 8;
            maximum = 100;
            <COPY> = 100;
        }};
        xkb_types {{ include \"complete\" }};
        xkb_compat {{ include \"complete\" }};
        xkb_symbols {{
            key <COPY> {{ [ {} ] }};
        }};
    }};",
        symbol
    )
    .into_bytes()
}

/// Upload a keymap from bytes to the compositor via an anonymous file.
fn upload_keymap_bytes(vk: &ZwpVirtualKeyboardV1, keymap_bytes: &[u8]) -> anyhow::Result<()> {
    let fd = create_anonymous_fd(keymap_bytes)?;
    let mut file = File::from(fd);
    file.write_all(keymap_bytes)?;
    file.flush()?;

    vk.keymap(
        WEnum::Value(wl_keyboard::KeymapFormat::XkbV1).into(),
        file.as_fd(),
        keymap_bytes.len() as u32,
    );
    // Keep the file open long enough for the compositor to read it; it will
    // dup the fd. The file is unlinked from the filesystem immediately.
    let _ = file;
    Ok(())
}

/// Build the default US keymap and upload it to the compositor.
fn upload_keymap(vk: &ZwpVirtualKeyboardV1) -> anyhow::Result<Vec<u8>> {
    let keymap_bytes = build_us_keymap()?;
    upload_keymap_bytes(vk, &keymap_bytes)?;
    Ok(keymap_bytes)
}

/// Create an anonymous file in `XDG_RUNTIME_DIR` and write the given data into
/// it, returning a file descriptor positioned at the start.
fn create_anonymous_fd(data: &[u8]) -> anyhow::Result<OwnedFd> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("XDG_RUNTIME_DIR not set"))?;

    let mut path = runtime_dir;
    path.push("rocknix-keyboard-keymap-XXXXXX");

    let mut c_template = path.into_os_string().into_encoded_bytes();
    c_template.push(0);

    let raw = unsafe {
        let raw = libc::mkostemp(
            c_template.as_mut_ptr() as *mut libc::c_char,
            libc::O_CLOEXEC,
        );
        if raw < 0 {
            anyhow::bail!("mkostemp failed: {}", std::io::Error::last_os_error());
        }
        // Unlink the file immediately so it is truly anonymous.
        libc::unlink(c_template.as_ptr() as *const libc::c_char);
        raw
    };

    let mut file = unsafe { File::from_raw_fd(raw) };
    file.set_len(data.len() as u64)?;
    file.write_all(data)?;
    file.flush()?;
    file.rewind()?;
    Ok(file.into())
}
