use std::env;

use reqwest::blocking::{multipart, Client};
use serde::Deserialize;

use crate::domain::{AudioBuffer, ScreenContext, TranscriptDraft};

pub trait TranscriptionEngine {
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        screen_context: &ScreenContext,
        vocabulary: &[String],
    ) -> Result<TranscriptDraft, String>;
}

#[derive(Debug, Default)]
pub struct DemoTranscriptionEngine;

impl TranscriptionEngine for DemoTranscriptionEngine {
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        screen_context: &ScreenContext,
        _vocabulary: &[String],
    ) -> Result<TranscriptDraft, String> {
        let low_latency_bias = if audio.duration.as_millis() < 1_500 {
            "Ship the latency fixes today"
        } else {
            "Ship the latency fixes today and review the accessibility path"
        };

        let text = format!(
            "{} [{}]",
            low_latency_bias,
            screen_context.prompt_fragment(90)
        );

        Ok(TranscriptDraft {
            raw_text: text,
            confidence_hint: 0.92,
        })
    }
}

pub struct GroqTranscriptionEngine {
    client: Client,
    api_key: String,
    model: String,
    language: Option<String>,
    base_url: String,
}

impl GroqTranscriptionEngine {
    pub fn from_env() -> Result<Self, String> {
        let api_key = env::var("GROQ_API_KEY")
            .map_err(|_| "missing GROQ_API_KEY in the environment or .env file".to_string())?;
        let model = env::var("GROQ_TRANSCRIPTION_MODEL")
            .unwrap_or_else(|_| "whisper-large-v3-turbo".to_string());
        let language = env::var("GROQ_TRANSCRIPTION_LANGUAGE").ok();
        let base_url = env::var("GROQ_API_BASE")
            .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_string());
        let client = Client::builder()
            .build()
            .map_err(|error| format!("failed to build HTTP client: {error}"))?;

        Ok(Self {
            client,
            api_key,
            model,
            language,
            base_url,
        })
    }
}

impl TranscriptionEngine for GroqTranscriptionEngine {
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        screen_context: &ScreenContext,
        vocabulary: &[String],
    ) -> Result<TranscriptDraft, String> {
        if audio.wav_bytes.is_empty() {
            return Err("captured audio buffer is empty".into());
        }

        let file = multipart::Part::bytes(audio.wav_bytes.clone())
            .file_name("capture.wav")
            .mime_str("audio/wav")
            .map_err(|error| format!("failed to build multipart audio part: {error}"))?;

        let mut form = multipart::Form::new()
            .text("model", self.model.clone())
            .part("file", file)
            .text("temperature", "0")
            .text("response_format", "json");

        if let Some(language) = self.language.as_ref() {
            form = form.text("language", language.clone());
        }

        let prompt = build_transcription_prompt(screen_context, vocabulary);
        if !prompt.is_empty() {
            form = form.text("prompt", prompt);
        }

        let response = self
            .client
            .post(format!("{}/audio/transcriptions", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .map_err(|error| format!("transcription request failed: {error}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(format!("transcription request failed with {status}: {body}"));
        }

        let parsed: GroqTranscriptionResponse = response
            .json()
            .map_err(|error| format!("failed to parse transcription response: {error}"))?;
        let text = parsed.text.trim().to_string();
        if text.is_empty() {
            return Err("transcription response was empty".into());
        }

        Ok(TranscriptDraft {
            raw_text: text,
            confidence_hint: 0.9,
        })
    }
}

/// Build a tight Whisper biasing prompt using a curated vocabulary list rather
/// than dumping the whole visible screen. The 896-byte limit is precious — we
/// spend it on rare names/terms most likely to be misheard.
fn build_transcription_prompt(screen_context: &ScreenContext, vocabulary: &[String]) -> String {
    const MAX_PROMPT_BYTES: usize = 800;

    let header = format!(
        "App: {}. Window: {}.",
        screen_context.app_name, screen_context.window_title,
    );

    if vocabulary.is_empty() {
        return truncate_to_byte_limit(&header, MAX_PROMPT_BYTES);
    }

    // Pack as many vocab terms as fit after the header.
    let mut out = header;
    out.push_str(" Vocabulary: ");
    let base_len = out.len();

    let mut first = true;
    for term in vocabulary {
        let candidate = if first {
            term.clone()
        } else {
            format!(", {term}")
        };
        if out.len() + candidate.len() + 1 > MAX_PROMPT_BYTES {
            break;
        }
        out.push_str(&candidate);
        first = false;
    }
    out.push('.');

    if out.len() == base_len + 1 {
        // Nothing fit; drop the dangling "Vocabulary: ." suffix.
        out.truncate(base_len.saturating_sub(" Vocabulary: ".len()));
    }

    truncate_to_byte_limit(&out, MAX_PROMPT_BYTES)
}

/// Truncate a string to fit within `max_bytes` of UTF-8, cutting at a char boundary.
fn truncate_to_byte_limit(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[derive(Debug, Deserialize)]
struct GroqTranscriptionResponse {
    text: String,
}
