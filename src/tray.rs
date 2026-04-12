/// Menubar tray icon for OpenFlow.
///
/// Architecture: the tray runs on the main thread (macOS requirement for
/// NSStatusItem / NSApplication). The pipeline runs on a background thread
/// and sends `TrayStatus` updates over an mpsc channel.
///
/// # Icon strategy
///
/// When `assets/icon.png` exists at build time (detected by `build.rs`), the
/// tray uses that PNG as the base shape and applies a colour tint per state.
/// The PNG should be 32×32 px RGBA, white mic on transparent background
/// (macOS "template image" style).
///
/// Without the PNG the tray falls back to a procedurally-drawn coloured circle
/// so the app always works out of the box. See `assets/ICON.md` for the Figma
/// workflow to produce the real icon.
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, TrayIconBuilder, TrayIconEvent,
};

#[cfg(target_os = "macos")]
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget};

/// Status messages sent from the pipeline thread to the tray.
#[derive(Debug, Clone)]
pub enum TrayStatus {
    Idle,
    Recording,
    Processing,
    /// Pasted successfully — briefly shown before reverting to Idle.
    Success,
    Error(String),
}

/// Run the tray icon event loop on the **main thread**. Blocks until the user
/// chooses Quit from the tray menu or the process is otherwise terminated.
pub fn run_event_loop(status_rx: Receiver<TrayStatus>) {
    // macOS: ActivationPolicy::Accessory = no Dock icon, pure background agent.
    #[cfg(target_os = "macos")]
    let event_loop = EventLoopBuilder::new()
        .with_activation_policy(ActivationPolicy::Accessory)
        .build()
        .expect("failed to create event loop");

    #[cfg(not(target_os = "macos"))]
    let event_loop = EventLoopBuilder::new()
        .build()
        .expect("failed to create event loop");

    let quit_item = MenuItem::new("Quit OpenFlow", true, None);
    let quit_id = quit_item.id().clone();

    let menu = Menu::new();
    menu.append(&quit_item).unwrap();

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("OpenFlow – idle")
        .with_icon(state_icon(IconColor::Idle))
        .build()
        .expect("failed to create tray icon — ensure the app has screen access");

    // After a Success flash we revert to Idle after this delay.
    const SUCCESS_FLASH_MS: u64 = 1_200;
    let mut success_at: Option<Instant> = None;

    event_loop
        .run(move |_event: Event<()>, elwt: &EventLoopWindowTarget<()>| {
            // Wake up every 80ms to check channels and handle the success flash.
            elwt.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(80),
            ));

            // Revert success icon back to idle after flash window expires.
            if let Some(t) = success_at {
                if t.elapsed().as_millis() as u64 >= SUCCESS_FLASH_MS {
                    success_at = None;
                    let _ = tray.set_tooltip(Some("OpenFlow – idle"));
                    let _ = tray.set_icon(Some(state_icon(IconColor::Idle)));
                }
            }

            // Handle menu item clicks (Quit).
            if let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == quit_id {
                    elwt.exit();
                    return;
                }
            }

            // Drain any tray icon click events (unused for now).
            let _ = TrayIconEvent::receiver().try_recv();

            // Drain pipeline status updates — last one wins per tick.
            let mut latest: Option<TrayStatus> = None;
            while let Ok(status) = status_rx.try_recv() {
                latest = Some(status);
            }

            if let Some(status) = latest {
                match &status {
                    TrayStatus::Idle => {
                        success_at = None;
                        let _ = tray.set_tooltip(Some("OpenFlow – idle"));
                        let _ = tray.set_icon(Some(state_icon(IconColor::Idle)));
                    }
                    TrayStatus::Recording => {
                        success_at = None;
                        let _ = tray.set_tooltip(Some("OpenFlow – recording…"));
                        let _ = tray.set_icon(Some(state_icon(IconColor::Recording)));
                    }
                    TrayStatus::Processing => {
                        success_at = None;
                        let _ = tray.set_tooltip(Some("OpenFlow – processing…"));
                        let _ = tray.set_icon(Some(state_icon(IconColor::Processing)));
                    }
                    TrayStatus::Success => {
                        success_at = Some(Instant::now());
                        let _ = tray.set_tooltip(Some("OpenFlow – pasted ✓"));
                        let _ = tray.set_icon(Some(state_icon(IconColor::Success)));
                    }
                    TrayStatus::Error(msg) => {
                        success_at = None;
                        let tooltip = format!("OpenFlow – error: {msg}");
                        let _ = tray.set_tooltip(Some(&tooltip));
                        let _ = tray.set_icon(Some(state_icon(IconColor::Error)));
                    }
                }
            }
        })
        .unwrap();
}

// ─── Icon rendering ──────────────────────────────────────────────────────────

enum IconColor {
    Idle,
    Recording,
    Processing,
    Success,
    Error,
}

impl IconColor {
    fn tint(&self) -> (u8, u8, u8) {
        match self {
            Self::Idle       => (130, 130, 130), // neutral grey
            Self::Recording  => (220,  40,  40), // red
            Self::Processing => (200, 160,   0), // amber
            Self::Success    => ( 40, 180,  40), // green
            Self::Error      => (200,   0,   0), // deep red
        }
    }
}

/// Return the appropriate icon for the given pipeline state.
///
/// Uses the PNG base icon (tinted) when available; falls back to a
/// procedurally-drawn circle otherwise.
fn state_icon(color: IconColor) -> Icon {
    let (r, g, b) = color.tint();
    #[cfg(has_icon_png)]
    {
        return png_icon(r, g, b);
    }
    #[cfg(not(has_icon_png))]
    circle_icon(r, g, b)
}

// ─── PNG-based icon (used when assets/icon.png is present at build time) ─────

/// Embedded PNG bytes — only compiled in when `assets/icon.png` exists.
#[cfg(has_icon_png)]
static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// Decode the embedded PNG and tint every opaque pixel with `(r, g, b)`.
/// The PNG should be white (#FFF) on transparent — the tint replaces the
/// white, preserving the shape defined by the alpha channel.
#[cfg(has_icon_png)]
fn png_icon(r: u8, g: u8, b: u8) -> Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
    let mut reader = decoder
        .read_info()
        .expect("assets/icon.png: failed to read PNG header");

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .expect("assets/icon.png: failed to decode PNG frame");

    let width = info.width;
    let height = info.height;

    // Ensure we have RGBA. Convert RGB → RGBA if needed.
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let src = &buf[..info.buffer_size()];
            let mut out = Vec::with_capacity((width * height * 4) as usize);
            for chunk in src.chunks(3) {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            out
        }
        _ => panic!("assets/icon.png: unsupported color type {:?}", info.color_type),
    };

    // Scale down to the macOS menu-bar size, then tint.
    const TARGET: u32 = 22;
    let scaled = scale_nearest(&rgba, width, height, TARGET, TARGET);
    let tinted = tint_rgba(&scaled, r, g, b);
    Icon::from_rgba(tinted, TARGET, TARGET).expect("failed to build PNG tray icon")
}

/// Nearest-neighbour downscale of an RGBA buffer from `(sw×sh)` to `(dw×dh)`.
#[cfg(has_icon_png)]
fn scale_nearest(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        for dx in 0..dw {
            let sx = (dx * sw / dw) as usize;
            let sy = (dy * sh / dh) as usize;
            let si = (sy * sw as usize + sx) * 4;
            let di = (dy as usize * dw as usize + dx as usize) * 4;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// Tint RGBA pixels: multiply each channel by the tint fraction derived from
/// the original pixel's brightness. Works best with a white-on-transparent
/// source image (macOS template-image style).
#[cfg(has_icon_png)]
fn tint_rgba(rgba: &[u8], r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for pixel in out.chunks_mut(4) {
        if pixel[3] == 0 {
            continue;
        }
        let brightness = pixel[0] as f32 / 255.0;
        pixel[0] = (r as f32 * brightness) as u8;
        pixel[1] = (g as f32 * brightness) as u8;
        pixel[2] = (b as f32 * brightness) as u8;
    }
    out
}

// ─── Procedural circle fallback ───────────────────────────────────────────────

/// Draw a solid anti-aliased circle as a 22×22 RGBA icon.
/// Used when no `assets/icon.png` is present at build time.
#[cfg(not(has_icon_png))]
fn circle_icon(r: u8, g: u8, b: u8) -> Icon {
    const SIZE: u32 = 22;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = SIZE as f32 / 2.0;
    let radius = center - 2.5;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= radius {
                let idx = ((y * SIZE + x) * 4) as usize;
                rgba[idx]     = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = if dist > radius - 1.0 {
                    ((radius - dist) * 255.0) as u8
                } else {
                    255
                };
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).expect("failed to build circle tray icon")
}
