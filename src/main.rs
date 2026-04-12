mod app;
mod config;
mod domain;
mod pipeline;
mod platform;
mod services;
mod tray;

use std::env;
use std::sync::mpsc;

use app::AssistantApp;
use config::AppConfig;

fn main() {
    dotenvy::dotenv().ok();

    let config = AppConfig::load();

    // Apply config-file model/language overrides to env before spawning threads.
    // Safe here: we're still single-threaded at startup.
    config.apply_env_overrides();

    let default_key = config.hold_key.clone();
    let mut app = AssistantApp::new(config.clone());
    let args: Vec<String> = env::args().collect();

    let result = match args.get(1).map(String::as_str) {
        // ── Live dictation + menubar ─────────────────────────────────────────
        Some("live") | None => {
            let trigger = args.get(2).map(String::as_str).unwrap_or(&default_key);
            run_with_tray(config, trigger.to_string())
        }

        // ── First-run wizard ─────────────────────────────────────────────────
        Some("setup") => app.run_setup(),

        // ── Permission info ──────────────────────────────────────────────────
        Some("permissions") => {
            app.print_permissions();
            Ok(())
        }

        // ── AX / focus inspection ────────────────────────────────────────────
        Some("focus") => app.inspect_focus(),
        Some("ax-debug") => app.print_ax_debug(),
        Some("ax-debug-after") => match parse_delay(args.get(2).map(String::as_str)) {
            Ok(delay) => app.print_ax_debug_after_delay(delay),
            Err(e) => Err(e),
        },
        Some("focus-after") => match parse_delay(args.get(2).map(String::as_str)) {
            Ok(delay) => app.inspect_focus_after_delay(delay),
            Err(e) => Err(e),
        },

        // ── Paste injection tests ────────────────────────────────────────────
        Some("paste") => {
            let text = args.get(2..).map(|p| p.join(" ")).unwrap_or_default();
            if text.is_empty() {
                Err("usage: openflow paste <text>".into())
            } else {
                app.paste_text(&text)
            }
        }
        Some("paste-after") => match parse_delay(args.get(2).map(String::as_str)) {
            Ok(delay) => {
                let text = args.get(3..).map(|p| p.join(" ")).unwrap_or_default();
                if text.is_empty() {
                    Err("usage: openflow paste-after <seconds> <text>".into())
                } else {
                    app.paste_text_after_delay(delay, &text)
                }
            }
            Err(e) => Err(e),
        },

        // ── Hotkey monitor ───────────────────────────────────────────────────
        Some("monitor-hotkey") => {
            let trigger = args.get(2).map(String::as_str).unwrap_or(&default_key);
            app.monitor_hotkey(trigger)
        }

        // ── Dry run (no mic, no API) ─────────────────────────────────────────
        Some("demo") => app.run_demo_session(),

        // ── Help ─────────────────────────────────────────────────────────────
        Some("help") => {
            app.print_help(args.first().map(String::as_str).unwrap_or("openflow"));
            Ok(())
        }

        Some(other) => Err(format!("unknown command: {other}. Run `openflow help` for usage.")),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Start live dictation with the menubar tray icon.
///
/// The tray icon **must** run on the main thread (macOS NSStatusItem
/// requirement). The pipeline runs on a background thread and sends
/// `TrayStatus` updates over an mpsc channel.
fn run_with_tray(config: AppConfig, trigger: String) -> Result<(), String> {
    let (status_tx, status_rx) = mpsc::channel();

    // Spawn pipeline on background thread.
    std::thread::spawn(move || {
        let app = AssistantApp::new(config);
        if let Err(e) = app.run_live_with_status(&trigger, Some(status_tx)) {
            eprintln!("pipeline error: {e}");
            std::process::exit(1);
        }
    });

    // Tray event loop blocks the main thread.
    tray::run_event_loop(status_rx);
    Ok(())
}

fn parse_delay(value: Option<&str>) -> Result<f64, String> {
    let value = value.ok_or_else(|| "missing delay in seconds".to_string())?;
    let seconds = value
        .parse::<f64>()
        .map_err(|_| format!("invalid delay: {value}"))?;
    if seconds < 0.0 {
        Err("delay must be non-negative".into())
    } else {
        Ok(seconds)
    }
}
