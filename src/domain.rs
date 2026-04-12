use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub pcm_frames: usize,
    pub duration: Duration,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub wav_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ScreenContext {
    pub app_name: String,
    pub window_title: String,
    pub focused_role: String,
    pub focused_value_preview: String,
    pub visible_text: String,
}

impl ScreenContext {
    pub fn prompt_fragment(&self, max_chars: usize) -> String {
        let raw = format!(
            "App: {}\nWindow: {}\nFocused role: {}\nFocused field preview: {}\nVisible screen text: {}",
            self.app_name, self.window_title, self.focused_role, self.focused_value_preview, self.visible_text
        );

        raw.chars().take(max_chars).collect()
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptDraft {
    pub raw_text: String,
    pub confidence_hint: f32,
}

#[derive(Debug, Clone)]
pub struct FinalTranscript {
    pub text: String,
    pub was_refined: bool,
}

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub app_name: String,
    pub element_role: String,
}

#[derive(Debug, Clone)]
pub struct LatencyReport {
    pub capture_ms: u64,
    pub transcription_ms: u64,
    pub refinement_ms: u64,
    pub paste_ms: u64,
}

impl fmt::Display for LatencyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "capture={}ms transcription={}ms refinement={}ms paste={}ms",
            self.capture_ms, self.transcription_ms, self.refinement_ms, self.paste_ms
        )
    }
}
