use std::time::{Duration, Instant};

use crate::config::AppConfig;
use crate::domain::{
    AudioBuffer, FinalTranscript, FocusTarget, LatencyReport, ScreenContext, TranscriptDraft,
};
use crate::services::{
    AudioCapture, FocusResolver, TextPaster, TranscriptRefiner, TranscriptionEngine,
};

pub struct Pipeline<A, F, P, R, T>
where
    A: AudioCapture,
    F: FocusResolver,
    P: TextPaster,
    R: TranscriptRefiner,
    T: TranscriptionEngine,
{
    config: AppConfig,
    audio: A,
    focus: F,
    paster: P,
    refiner: R,
    transcriber: T,
}

impl<A, F, P, R, T> Pipeline<A, F, P, R, T>
where
    A: AudioCapture,
    F: FocusResolver,
    P: TextPaster,
    R: TranscriptRefiner,
    T: TranscriptionEngine,
{
    pub fn new(
        config: AppConfig,
        audio: A,
        focus: F,
        paster: P,
        refiner: R,
        transcriber: T,
    ) -> Self {
        Self {
            config,
            audio,
            focus,
            paster,
            refiner,
            transcriber,
        }
    }

    pub fn run_once(&mut self) -> Result<PipelineOutcome, String> {
        let capture_started = Instant::now();
        self.audio.begin_capture()?;
        std::thread::sleep(Duration::from_millis(900));
        let audio = self.audio.end_capture()?;
        let capture_ms = capture_started.elapsed().as_millis() as u64;

        let focus_target = self.focus.current_focus_target()?;
        let screen_context = self.focus.current_screen_context()?;

        let transcription_started = Instant::now();
        let draft = self
            .transcriber
            .transcribe(&audio, &screen_context, &[])?;
        let transcription_ms = transcription_started.elapsed().as_millis() as u64;

        self.paster.paste_text(&focus_target, &draft.raw_text)?;

        let refinement_started = Instant::now();
        let final_transcript = self.refiner.refine(&draft, &screen_context, &[])?;
        let refinement_ms = refinement_started.elapsed().as_millis() as u64;

        // Upgrade the already-pasted draft in-place if refinement changed it.
        if final_transcript.text != draft.raw_text {
            let prev_char_count = draft.raw_text.chars().count();
            self.paster.replace_recent_paste(
                &focus_target,
                prev_char_count,
                &final_transcript.text,
            )?;
        }

        let paste_ms = 8;
        let latency = LatencyReport {
            capture_ms,
            transcription_ms,
            refinement_ms,
            paste_ms,
        };

        Ok(PipelineOutcome {
            audio,
            focus_target,
            screen_context,
            draft,
            final_transcript,
            latency,
            config_snapshot: self.config.clone(),
        })
    }
}

#[derive(Debug)]
pub struct PipelineOutcome {
    pub audio: AudioBuffer,
    pub focus_target: FocusTarget,
    pub screen_context: ScreenContext,
    pub draft: TranscriptDraft,
    pub final_transcript: FinalTranscript,
    pub latency: LatencyReport,
    pub config_snapshot: AppConfig,
}
