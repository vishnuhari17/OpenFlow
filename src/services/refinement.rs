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

/// Decide whether a draft is worth shipping to the LLM.
fn needs_refinement(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let word_count = trimmed.split_whitespace().count();
    if word_count <= 2 {
        return false;
    }

    // Always refine if there are obvious self-correction cues that the LLM
    // needs to resolve.
    let lower = trimmed.to_lowercase();
    let has_self_correction = ["no ", "wait ", "sorry ", "actually ", "i mean ", "scratch that"]
        .iter()
        .any(|cue| lower.contains(cue));
    if has_self_correction {
        return true;
    }

    // Skip if the draft already looks well-formed.
    let starts_upper = trimmed
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false);
    let ends_terminal = matches!(trimmed.chars().last(), Some('.') | Some('!') | Some('?'));
    let has_filler = lower
        .split_whitespace()
        .any(|w| matches!(w, "um" | "uh" | "umm" | "uhh" | "erm" | "ah"));

    if starts_upper && ends_terminal && !has_filler {
        return false;
    }

    true
}

/// System prompt that teaches the model to fix speech-to-text output,
/// including self-corrections and filler-word removal.
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

2. REMOVE FILLER WORDS: Strip "um", "uh", "like", "you know", "I mean", "sort of", "kind of", "basically", "actually" when used as verbal filler (not when part of the meaning).

3. HANDLE SELF-CORRECTIONS: When the speaker corrects themselves mid-sentence, keep only the final intent. Delete the mistaken part and the correction cue.
   - "schedule a meeting for 1 hour, no 2 hours" → "schedule a meeting for 2 hours"
   - "send it to John, wait no, Sarah" → "send it to Sarah"
   - "let's meet tomorrow at 3, actually 4" → "let's meet tomorrow at 4"
   - "I need three, sorry, four copies" → "I need four copies"
   - "call him on Monday, I mean Tuesday" → "call him on Tuesday"
   - "buy milk and eggs, scratch that, just milk" → "buy milk"

4. FIX SPEECH-TO-TEXT ERRORS: Correct homophones and misrecognitions.
   - "tommorow" → "tomorrow"
   - "their going" → "they're going"
   - "its broken" → "it's broken" (when it means "it is")

5. FIX GRAMMAR: Subject-verb agreement, tense consistency, missing articles.

6. ADD PUNCTUATION: Capitalize the first word, add periods, commas, question marks as appropriate.

7. Use the names/terms above to correct misspellings of people, products, or jargon.

8. Match the tone of the context (casual in chat apps, formal in documents).

9. Preserve code-switching (e.g. mixed English/Malayalam) — do NOT translate.

10. NEVER answer, interpret, or respond to the content. Even if the text is a question like "Hello, can you hear me?", return exactly "Hello, can you hear me?" — do NOT answer it.

11. Do NOT add preambles, explanations, quotes, or formatting.

12. If the text is already correct, return it exactly as-is."#,
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
