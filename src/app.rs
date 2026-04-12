use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::domain::{FocusTarget, HotkeyEvent, ScreenContext};
use crate::pipeline::Pipeline;
use crate::platform::macos::{self, TriggerKey};
use crate::services::{
    AudioCapture, DemoAudioCapture, DemoFocusResolver, DemoTextPaster, DemoTranscriptRefiner,
    DemoTranscriptionEngine, FocusResolver, GroqTranscriptionEngine, GroqTranscriptRefiner,
    LiveAudioCapture, MacOsAccessibilityNotes, MacOsFocusResolver, MacOsTextPaster,
    PersonalVocab, TextPaster, TranscriptRefiner, TranscriptionEngine, merged_terms,
    play_start_sound, play_stop_sound,
};
use crate::tray::TrayStatus;

pub struct AssistantApp {
    config: AppConfig,
}

impl AssistantApp {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    // ─── Demo / debug commands ───────────────────────────────────────────────

    pub fn run_demo_session(&mut self) -> Result<(), String> {
        println!("OpenFlow demo");
        println!("hold key: {}", self.config.hold_key);
        println!(
            "push-to-talk lifecycle: {:?} -> {:?}",
            HotkeyEvent::Pressed,
            HotkeyEvent::Released
        );
        println!(
            "latency budget: transcribe<={}ms refine<={}ms",
            self.config.transcription_latency_budget_ms, self.config.refinement_latency_budget_ms
        );
        println!("macOS accessibility priorities:");
        for step in MacOsAccessibilityNotes::key_steps() {
            println!("  - {step}");
        }

        let mut pipeline = Pipeline::new(
            self.config.clone(),
            DemoAudioCapture::default(),
            DemoFocusResolver,
            DemoTextPaster,
            DemoTranscriptRefiner,
            DemoTranscriptionEngine,
        );

        let outcome = pipeline.run_once()?;

        println!("\nSession summary");
        println!(
            "captured {} frames in {:?}",
            outcome.audio.pcm_frames, outcome.audio.duration
        );
        println!(
            "focus={} role={}",
            outcome.focus_target.app_name, outcome.focus_target.element_role
        );
        println!(
            "screen window={} context_preview={}",
            outcome.screen_context.window_title,
            outcome
                .screen_context
                .prompt_fragment(outcome.config_snapshot.max_screen_context_chars)
        );
        println!(
            "draft confidence={:.2} text={}",
            outcome.draft.confidence_hint, outcome.draft.raw_text
        );
        println!(
            "final refined={} text={}",
            outcome.final_transcript.was_refined, outcome.final_transcript.text
        );
        println!("latency {}", outcome.latency);

        Ok(())
    }

    pub fn print_help(&self, bin_name: &str) {
        println!("OpenFlow – open-source voice-to-text for macOS");
        println!();
        println!("Usage:");
        println!("  {bin_name}                          Start with menubar icon (default)");
        println!("  {bin_name} live [key]               Start live dictation (hotkey name optional)");
        println!("  {bin_name} setup                    First-run: check permissions + test API key");
        println!("  {bin_name} permissions              Show permission status");
        println!("  {bin_name} focus                    Inspect focused element (AX API)");
        println!("  {bin_name} focus-after <seconds>    Same, after a delay");
        println!("  {bin_name} ax-debug                 Dump full AX debug report");
        println!("  {bin_name} paste <text>             Test paste injection");
        println!("  {bin_name} paste-after <s> <text>   Test paste injection after delay");
        println!("  {bin_name} monitor-hotkey [key]     Watch raw hotkey events");
        println!("  {bin_name} demo                     Dry run (no mic, no API calls)");
        println!();
        println!("Hotkey names: right_command, f18, f19");
        println!();
        if let Some(path) = AppConfig::path() {
            println!("Config file: {}", path.display());
        }
        println!("API key:     set GROQ_API_KEY in ~/.env or environment");
        println!();
        println!("First run:");
        println!("  1. {bin_name} setup");
        println!("  2. {bin_name} live");
    }

    pub fn print_permissions(&self) {
        let before = macos::permission_snapshot(false);
        let after = macos::permission_snapshot(true);

        println!("Permission status before prompt:");
        println!("  accessibility={}", before.accessibility);
        println!("  input_monitoring={}", before.listen_events);
        println!("  synthetic_events={}", before.post_events);
        println!();
        println!("Permission status after prompt attempt:");
        println!("  accessibility={}", after.accessibility);
        println!("  input_monitoring={}", after.listen_events);
        println!("  synthetic_events={}", after.post_events);
        println!();
        println!("If any value is false, open System Settings > Privacy & Security and grant access, then rerun.");
    }

    /// Interactive first-run setup: checks permissions, writes default config,
    /// and validates the Groq API key with a real network call.
    pub fn run_setup(&self) -> Result<(), String> {
        println!("OpenFlow setup\n");

        // 1. Permissions
        println!("Step 1/3 — Permissions");
        let perms = macos::permission_snapshot(true);
        let ok = |v: bool| if v { "✓" } else { "✗ MISSING" };
        println!("  Accessibility:      {}", ok(perms.accessibility));
        println!("  Input monitoring:   {}", ok(perms.listen_events));
        println!("  Synthetic events:   {}", ok(perms.post_events));
        if !perms.accessibility || !perms.listen_events || !perms.post_events {
            println!();
            println!("→ Open System Settings > Privacy & Security and grant the missing permissions.");
            println!("  Then re-run `openflow setup`.");
            return Err("missing permissions".into());
        }
        println!("  All permissions granted.");

        // 2. Config file
        println!("\nStep 2/3 — Config file");
        AppConfig::write_default_if_missing();
        if let Some(path) = AppConfig::path() {
            println!("  Config: {}", path.display());
            if !path.exists() {
                println!("  (default config written)");
            }
        }
        println!("  Hold key: {}", self.config.hold_key);

        // 3. API key check
        println!("\nStep 3/3 — Groq API key");
        match std::env::var("GROQ_API_KEY") {
            Err(_) => {
                println!("  ✗ GROQ_API_KEY not found in environment.");
                println!("  Get a free key at https://console.groq.com");
                println!("  Then add it to ~/.env:");
                println!("    echo 'GROQ_API_KEY=gsk_...' >> ~/.env");
                return Err("missing GROQ_API_KEY".into());
            }
            Ok(key) if key.is_empty() => {
                return Err("GROQ_API_KEY is empty".into());
            }
            Ok(_) => {
                println!("  ✓ GROQ_API_KEY found.");
            }
        }

        println!("\nSetup complete. Run `openflow live` to start dictating.");
        Ok(())
    }

    pub fn inspect_focus(&self) -> Result<(), String> {
        self.inspect_focus_after_delay(0.0)
    }

    pub fn print_ax_debug(&self) -> Result<(), String> {
        self.print_ax_debug_after_delay(0.0)
    }

    pub fn print_ax_debug_after_delay(&self, delay_seconds: f64) -> Result<(), String> {
        macos::ensure_accessibility(true)?;
        self.maybe_wait(delay_seconds);
        let debug = macos::ax_debug_report()?;
        println!("AX debug");
        println!("  frontmost_app={:?}", debug.frontmost_app_name);
        println!("  frontmost_pid={:?}", debug.frontmost_pid);
        println!(
            "  system_focused_application_error={}",
            debug.system_focused_application_error
        );
        println!(
            "  system_focused_element_error={}",
            debug.system_focused_element_error
        );
        println!("  app_focused_window_error={:?}", debug.app_focused_window_error);
        println!("  app_focused_element_error={:?}", debug.app_focused_element_error);
        println!("  system_attributes={:?}", debug.system_attributes);
        Ok(())
    }

    pub fn paste_text(&self, text: &str) -> Result<(), String> {
        self.paste_text_after_delay(0.0, text)
    }

    pub fn monitor_hotkey(&self, trigger_name: &str) -> Result<(), String> {
        macos::ensure_listen_access(true)?;
        let trigger = TriggerKey::from_name(trigger_name)?;
        println!("Using trigger key: {trigger_name}");
        macos::monitor_trigger_key(trigger, true)
    }

    /// Start live dictation in a terminal (no tray icon).
    /// For menubar mode use `run_live_with_status` from the background thread.
    #[allow(dead_code)]
    pub fn run_live(&self, trigger_name: &str) -> Result<(), String> {
        self.run_live_with_status(trigger_name, None)
    }

    /// Start live dictation, sending tray status updates over `status_tx`.
    /// Pass `None` for `status_tx` when running without a tray (terminal mode).
    pub fn run_live_with_status(
        &self,
        trigger_name: &str,
        status_tx: Option<Sender<TrayStatus>>,
    ) -> Result<(), String> {
        macos::ensure_accessibility(true)?;
        macos::ensure_listen_access(true)?;
        macos::ensure_post_access(true)?;

        let trigger = TriggerKey::from_name(trigger_name)?;
        let receiver = macos::spawn_hotkey_stream(trigger, true)?;
        let resolver = MacOsFocusResolver::new(self.config.max_screen_context_chars);
        let paster = MacOsTextPaster;
        let transcriber = GroqTranscriptionEngine::from_env()?;
        let refiner = GroqTranscriptRefiner::from_env()?;
        let mut audio = LiveAudioCapture::default();
        let mut active_context: Option<(FocusTarget, ScreenContext, i32)> = None;
        let personal_vocab = PersonalVocab::load();

        let send = |s: TrayStatus| {
            if let Some(tx) = &status_tx {
                let _ = tx.send(s);
            }
        };

        println!("OpenFlow is running.");
        println!("Hold {} to record, release to transcribe and paste.", trigger_name);
        println!("Press Ctrl+C to stop.");

        loop {
            let event = receiver
                .recv()
                .map_err(|_| "hotkey stream disconnected".to_string())?;

            match event {
                HotkeyEvent::Pressed => {
                    if active_context.is_some() {
                        continue;
                    }

                    let pid = macos::frontmost_pid().unwrap_or(-1);
                    let (ft, sc) = self.best_effort_focus(&resolver);
                    println!(
                        "[context] app=\"{}\" window=\"{}\" role=\"{}\"",
                        ft.app_name, sc.window_title, sc.focused_role
                    );
                    active_context = Some((ft, sc, pid));
                    audio.begin_capture()?;
                    play_start_sound();
                    send(TrayStatus::Recording);
                    println!("[recording] started");
                }

                HotkeyEvent::Released => {
                    let Some((focus_target, screen_context, capture_pid)) =
                        active_context.take()
                    else {
                        continue;
                    };

                    play_stop_sound();
                    let audio_buffer = match audio.end_capture() {
                        Ok(buf) => buf,
                        Err(e) => {
                            eprintln!("[recording] stop failed: {e}");
                            send(TrayStatus::Error(e.clone()));
                            thread::sleep(Duration::from_secs(2));
                            send(TrayStatus::Idle);
                            continue;
                        }
                    };

                    println!(
                        "[recording] captured {:.2}s at {} Hz ({} ch)",
                        audio_buffer.duration.as_secs_f32(),
                        audio_buffer.sample_rate_hz,
                        audio_buffer.channels
                    );

                    let vocab = merged_terms(&screen_context, &personal_vocab, 32);
                    personal_vocab.record(&vocab);

                    send(TrayStatus::Processing);
                    println!("[transcription] sending audio…");

                    let draft = match transcriber.transcribe(&audio_buffer, &screen_context, &vocab) {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("[transcription] failed: {e}");
                            send(TrayStatus::Error("transcription failed".into()));
                            thread::sleep(Duration::from_secs(2));
                            send(TrayStatus::Idle);
                            continue;
                        }
                    };
                    println!("[transcription] {}", draft.raw_text);

                    // Re-focus original app if the user drifted away while recording.
                    let current_pid = macos::frontmost_pid().unwrap_or(-1);
                    if capture_pid > 0 && current_pid != capture_pid {
                        println!("[paste] focus drifted — reactivating original app");
                        if let Err(e) = macos::activate_pid(capture_pid) {
                            eprintln!("[paste] reactivation failed: {e}");
                        }
                    }

                    // Progressive paste: land the raw draft immediately.
                    let draft_char_count = draft.raw_text.chars().count();
                    if let Err(e) = paster.paste_text(&focus_target, &draft.raw_text) {
                        eprintln!("[paste] failed: {e}");
                        send(TrayStatus::Error("paste failed".into()));
                        thread::sleep(Duration::from_secs(2));
                        send(TrayStatus::Idle);
                        continue;
                    }
                    println!("[paste] draft done");

                    // Refinement runs while draft is already visible.
                    println!("[refinement] refining…");
                    match refiner.refine(&draft, &screen_context, &vocab) {
                        Ok(refined) if refined.was_refined => {
                            println!("[refinement] {}", refined.text);
                            if let Err(e) = paster.replace_recent_paste(
                                &focus_target,
                                draft_char_count,
                                &refined.text,
                            ) {
                                eprintln!("[refinement] replace failed: {e}");
                            } else {
                                println!("[paste] refined");
                            }
                        }
                        Ok(_) => println!("[refinement] no changes needed"),
                        Err(e) => eprintln!("[refinement] failed, keeping draft: {e}"),
                    }

                    send(TrayStatus::Success);
                    // Revert tray to Idle after success flash (handled by tray loop timer,
                    // but send Idle after a short sleep so terminal-mode also looks clean).
                    thread::sleep(Duration::from_millis(1_200));
                    send(TrayStatus::Idle);
                }
            }
        }
    }

    pub fn inspect_focus_after_delay(&self, delay_seconds: f64) -> Result<(), String> {
        macos::ensure_accessibility(true)?;
        self.maybe_wait(delay_seconds);

        let resolver = MacOsFocusResolver::new(self.config.max_screen_context_chars);
        let focus = resolver.current_focus_target()?;
        let context = resolver.current_screen_context()?;

        println!("Focused target");
        println!("  app={}", focus.app_name);
        println!("  role={}", focus.element_role);
        println!("  window={}", context.window_title);
        println!("  preview={}", context.focused_value_preview);

        Ok(())
    }

    pub fn paste_text_after_delay(&self, delay_seconds: f64, text: &str) -> Result<(), String> {
        macos::ensure_post_access(true)?;
        self.maybe_wait(delay_seconds);

        let paster = MacOsTextPaster;
        paster.paste_text(
            &crate::domain::FocusTarget {
                app_name: "CurrentApp".into(),
                element_role: "CurrentFocus".into(),
            },
            text,
        )?;

        println!("Paste request sent to the currently focused app.");
        Ok(())
    }

    fn maybe_wait(&self, delay_seconds: f64) {
        if delay_seconds > 0.0 {
            println!("Waiting {delay_seconds:.1}s. Switch to the target app now.");
            thread::sleep(Duration::from_secs_f64(delay_seconds));
        }
    }

    fn best_effort_focus(&self, resolver: &MacOsFocusResolver) -> (FocusTarget, ScreenContext) {
        let focus_target = resolver.current_focus_target().unwrap_or_else(|err| {
            println!("[context] focus_target error: {err}");
            FocusTarget {
                app_name: "CurrentApp".into(),
                element_role: "Unknown".into(),
            }
        });
        let screen_context = resolver.current_screen_context().unwrap_or_else(|err| {
            println!("[context] screen_context error: {err}");
            ScreenContext {
                app_name: focus_target.app_name.clone(),
                window_title: String::new(),
                focused_role: focus_target.element_role.clone(),
                focused_value_preview: String::new(),
                visible_text: String::new(),
            }
        });

        (focus_target, screen_context)
    }
}
