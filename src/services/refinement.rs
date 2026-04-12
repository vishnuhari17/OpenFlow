use std::env;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::domain::{FinalTranscript, ScreenContext, TranscriptDraft};

pub trait TranscriptRefiner {
    fn refine(
        &self,
        transcript: &TranscriptDraft,
        screen_context: &ScreenContext,
        vocabulary: &[String],
    ) -> Result<FinalTranscript, String>;
}

#[derive(Debug, Default)]
pub struct DemoTranscriptRefiner;

impl TranscriptRefiner for DemoTranscriptRefiner {
    fn refine(
        &self,
        transcript: &TranscriptDraft,
        screen_context: &ScreenContext,
        _vocabulary: &[String],
    ) -> Result<FinalTranscript, String> {
        let mut refined = transcript.raw_text.clone();

        if screen_context.focused_value_preview.contains("Action items") && !refined.ends_with('.') {
            refined.push('.');
        }

        let was_refined = refined != transcript.raw_text;

        Ok(FinalTranscript {
            text: refined,
            was_refined,
        })
    }
}

pub struct GroqTranscriptRefiner {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl GroqTranscriptRefiner {
    pub fn from_env() -> Result<Self, String> {
        let api_key = env::var("GROQ_API_KEY")
            .map_err(|_| "missing GROQ_API_KEY in the environment or .env file".to_string())?;
        let model = env::var("GROQ_REFINEMENT_MODEL")
            .unwrap_or_else(|_| "llama-3.1-8b-instant".to_string());
        let base_url = env::var("GROQ_API_BASE")
            .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_string());
        let client = Client::builder()
            .build()
            .map_err(|error| format!("failed to build HTTP client: {error}"))?;

        Ok(Self {
            client,
            api_key,
            model,
            base_url,
        })
    }
}

impl TranscriptRefiner for GroqTranscriptRefiner {
    fn refine(
        &self,
        transcript: &TranscriptDraft,
        screen_context: &ScreenContext,
        vocabulary: &[String],
    ) -> Result<FinalTranscript, String> {
        // Skip the network round-trip for very short or already-clean text.
        // Refinement on "ok", "yes", "send it", etc. is pure latency cost.
        if !needs_refinement(&transcript.raw_text) {
            return Ok(FinalTranscript {
                text: transcript.raw_text.clone(),
                was_refined: false,
            });
        }

        let system_prompt = build_system_prompt(screen_context, vocabulary);
        let user_message = transcript.raw_text.clone();

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_message },
            ],
            "temperature": 0.3,
            "max_tokens": 1024,
        });

        let response = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| format!("refinement request failed: {error}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(format!(
                "refinement request failed with {status}: {body}"
            ));
        }

        let parsed: ChatCompletionResponse = response
            .json()
            .map_err(|error| format!("failed to parse refinement response: {error}"))?;

        let refined_text = parsed
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content.trim().to_string())
            .unwrap_or_else(|| transcript.raw_text.clone());

        let was_refined = refined_text != transcript.raw_text;

        Ok(FinalTranscript {
            text: refined_text,
            was_refined,
        })
    }
}

/// Decide whether a draft is worth shipping to the LLM. Short or
/// already-well-formed drafts skip refinement entirely.
fn needs_refinement(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count <= 3 {
        return false;
    }

    // Skip if the draft already looks well-formed: starts uppercase, ends with
    // terminal punctuation, no obvious filler tokens.
    let starts_upper = trimmed
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    let ends_terminal = matches!(trimmed.chars().last(), Some('.') | Some('!') | Some('?'));
    let has_filler = trimmed
        .to_lowercase()
        .split_whitespace()
        .any(|w| matches!(w, "um" | "uh" | "umm" | "uhh" | "erm" | "ah"));

    if starts_upper && ends_terminal && !has_filler {
        return false;
    }

    true
}

/// Slim system prompt: app/role + vocabulary list, no full screen dump.
fn build_system_prompt(ctx: &ScreenContext, vocabulary: &[String]) -> String {
    let vocab_line = if vocabulary.is_empty() {
        String::new()
    } else {
        format!("\n- Names/terms on screen: {}", vocabulary.join(", "))
    };

    let nearby = if ctx.focused_value_preview.is_empty() {
        String::new()
    } else {
        format!("\n- Nearby text: {}", ctx.focused_value_preview)
    };

    format!(
        r#"You are a text correction tool, NOT a conversational assistant. You receive raw speech-to-text transcriptions and output the corrected version. You NEVER respond to, answer, or interpret the text — you ONLY correct and return it.

Context about where the user is typing:
- Application: {}
- Focused element: {}{}{}

Rules:
1. Output ONLY the corrected transcription text. Nothing else.
2. Fix grammar, spelling, and punctuation errors from speech-to-text.
3. Use the names/terms above to correct misspellings of people, products, or jargon.
4. Match the tone of the context (casual in chat apps, formal in documents).
5. Preserve code-switching (e.g. mixed English/Malayalam) — do NOT translate.
6. NEVER answer, interpret, or respond to the content. Even if the text is a question like "Hello, can you hear me?", return exactly "Hello, can you hear me?" — do NOT answer it.
7. Do NOT add preambles, explanations, quotes, or formatting.
8. If the text is already correct, return it exactly as-is."#,
        ctx.app_name, ctx.focused_role, nearby, vocab_line,
    )
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}
