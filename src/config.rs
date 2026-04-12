use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Key to hold while speaking. Valid values: right_command, f18, f19
    pub hold_key: String,
    pub transcription_latency_budget_ms: u64,
    pub refinement_latency_budget_ms: u64,
    pub max_screen_context_chars: usize,
    /// Override GROQ_TRANSCRIPTION_MODEL env var (optional)
    pub transcription_model: Option<String>,
    /// Override GROQ_REFINEMENT_MODEL env var (optional)
    pub refinement_model: Option<String>,
    /// ISO 639-1 language code hint for Whisper, e.g. "en". Empty = auto-detect.
    pub language: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hold_key: "right_command".to_string(),
            transcription_latency_budget_ms: 450,
            refinement_latency_budget_ms: 120,
            max_screen_context_chars: 2_000,
            transcription_model: None,
            refinement_model: None,
            language: None,
        }
    }
}

impl AppConfig {
    /// Load config from `~/.config/openflow/config.toml`, falling back to defaults
    /// on any read or parse error. Never panics.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<Self>(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("[config] failed to parse {path:?}: {e}. Using defaults.");
                Self::default()
            }
        }
    }

    /// Path to the config file. Also ensures the parent directory exists.
    pub fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        let mut p = PathBuf::from(home);
        p.push(".config");
        p.push("openflow");
        let _ = fs::create_dir_all(&p);
        p.push("config.toml");
        Some(p)
    }

    /// Write a default config file if none exists. Useful for first-run setup.
    pub fn write_default_if_missing() {
        let Some(path) = Self::path() else { return };
        if path.exists() {
            return;
        }
        let content = r#"# OpenFlow configuration
# ~/.config/openflow/config.toml

# Key to hold while speaking.
# Options: right_command, f18, f19
hold_key = "right_command"

# How much surrounding screen text to send to Whisper as context (chars).
max_screen_context_chars = 2000

# Whisper model for transcription (via Groq).
# transcription_model = "whisper-large-v3-turbo"

# LLM model for filler-word removal and cleanup (via Groq).
# refinement_model = "llama-3.1-8b-instant"

# ISO 639-1 language code for Whisper. Empty string = auto-detect.
# language = "en"
"#;
        let _ = fs::write(&path, content);
    }

    /// Apply config values as env-var overrides for services that read from env.
    /// Only sets a var if it is not already present in the environment.
    /// Must be called before spawning service threads.
    pub fn apply_env_overrides(&self) {
        if let Some(model) = &self.transcription_model {
            if std::env::var("GROQ_TRANSCRIPTION_MODEL").is_err() {
                // Safety: single-threaded context at startup — called before threads spawn.
                unsafe { std::env::set_var("GROQ_TRANSCRIPTION_MODEL", model) };
            }
        }
        if let Some(model) = &self.refinement_model {
            if std::env::var("GROQ_REFINEMENT_MODEL").is_err() {
                unsafe { std::env::set_var("GROQ_REFINEMENT_MODEL", model) };
            }
        }
        if let Some(lang) = &self.language {
            if !lang.is_empty() && std::env::var("GROQ_TRANSCRIPTION_LANGUAGE").is_err() {
                unsafe { std::env::set_var("GROQ_TRANSCRIPTION_LANGUAGE", lang) };
            }
        }
    }
}
