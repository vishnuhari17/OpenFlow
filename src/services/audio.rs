use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
use crate::domain::AudioBuffer;

pub trait AudioCapture {
    fn begin_capture(&mut self) -> Result<(), String>;
    fn end_capture(&mut self) -> Result<AudioBuffer, String>;
}

#[derive(Debug, Default)]
pub struct DemoAudioCapture {
    active: bool,
}

impl AudioCapture for DemoAudioCapture {
    fn begin_capture(&mut self) -> Result<(), String> {
        if self.active {
            return Err("audio capture already active".into());
        }

        self.active = true;
        Ok(())
    }

    fn end_capture(&mut self) -> Result<AudioBuffer, String> {
        if !self.active {
            return Err("audio capture was not active".into());
        }

        self.active = false;
        Ok(AudioBuffer {
            pcm_frames: 14_400,
            duration: Duration::from_millis(900),
            sample_rate_hz: 16_000,
            channels: 1,
            wav_bytes: Vec::new(),
        })
    }
}

pub struct LiveAudioCapture {
    state: Option<LiveCaptureState>,
}

struct LiveCaptureState {
    stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate_hz: u32,
    started_at: Instant,
}

impl Default for LiveAudioCapture {
    fn default() -> Self {
        Self { state: None }
    }
}

/// Target sample rate for upload. Whisper internally downsamples to 16 kHz, so
/// shipping anything higher is wasted bandwidth and slower transcription.
const TARGET_SAMPLE_RATE_HZ: u32 = 16_000;

/// VAD threshold in linear amplitude. Roughly −45 dBFS.
const VAD_THRESHOLD: f32 = 0.0056;

/// Window size for VAD analysis at the *target* sample rate (20 ms).
const VAD_WINDOW_SAMPLES: usize = (TARGET_SAMPLE_RATE_HZ as usize) * 20 / 1000;

/// Padding kept around detected speech (100 ms on each side) so we don't clip
/// breath onsets.
const VAD_PADDING_SAMPLES: usize = (TARGET_SAMPLE_RATE_HZ as usize) * 100 / 1000;

impl AudioCapture for LiveAudioCapture {
    fn begin_capture(&mut self) -> Result<(), String> {
        if self.state.is_some() {
            return Err("audio capture already active".into());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "no default input device found".to_string())?;
        let supported = select_input_config(&device)?;

        let sample_rate_hz = supported.sample_rate().0;
        let input_channels = supported.channels();
        let config: StreamConfig = supported.config();

        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let err_fn = |error| eprintln!("audio input stream error: {error}");

        let stream = match supported.sample_format() {
            SampleFormat::F32 => {
                let samples = Arc::clone(&samples);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _| push_input_samples_f32(data, input_channels, &samples),
                        err_fn,
                        None,
                    )
                    .map_err(|error| format!("failed to build f32 input stream: {error}"))?
            }
            SampleFormat::I16 => {
                let samples = Arc::clone(&samples);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _| push_input_samples_i16(data, input_channels, &samples),
                        err_fn,
                        None,
                    )
                    .map_err(|error| format!("failed to build i16 input stream: {error}"))?
            }
            SampleFormat::U16 => {
                let samples = Arc::clone(&samples);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u16], _| push_input_samples_u16(data, input_channels, &samples),
                        err_fn,
                        None,
                    )
                    .map_err(|error| format!("failed to build u16 input stream: {error}"))?
            }
            sample_format => {
                return Err(format!("unsupported input sample format: {sample_format:?}"));
            }
        };

        stream
            .play()
            .map_err(|error| format!("failed to start audio stream: {error}"))?;

        self.state = Some(LiveCaptureState {
            stream,
            samples,
            sample_rate_hz,
            started_at: Instant::now(),
        });

        Ok(())
    }

    fn end_capture(&mut self) -> Result<AudioBuffer, String> {
        let state = self
            .state
            .take()
            .ok_or_else(|| "audio capture was not active".to_string())?;

        let duration = state.started_at.elapsed();
        drop(state.stream);

        let samples = state
            .samples
            .lock()
            .map_err(|_| "failed to lock captured audio buffer".to_string())?
            .clone();

        if samples.is_empty() {
            return Err("no microphone samples were captured".into());
        }

        // 1. Downsample to 16 kHz mono (Whisper's native rate).
        let resampled = resample_linear(&samples, state.sample_rate_hz, TARGET_SAMPLE_RATE_HZ);

        // 2. Trim leading/trailing silence so we only ship voiced audio.
        let trimmed = trim_silence(&resampled);

        if trimmed.is_empty() {
            return Err("no speech detected in captured audio".into());
        }

        // 3. Encode WAV in-memory (no tempfile round-trip).
        let wav_bytes = encode_wav_in_memory(&trimmed, TARGET_SAMPLE_RATE_HZ)?;

        Ok(AudioBuffer {
            pcm_frames: trimmed.len(),
            duration,
            sample_rate_hz: TARGET_SAMPLE_RATE_HZ,
            channels: 1,
            wav_bytes,
        })
    }
}

fn select_input_config(device: &cpal::Device) -> Result<SupportedStreamConfig, String> {
    let default = device
        .default_input_config()
        .map_err(|error| format!("failed to get default input config: {error}"))?;

    let preferred = device
        .supported_input_configs()
        .map_err(|error| format!("failed to query supported input configs: {error}"))?
        .find(|config| config.channels() == 1)
        .map(|config| config.with_max_sample_rate());

    Ok(preferred.unwrap_or(default))
}

fn push_input_samples_f32(data: &[f32], channels: u16, sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut buffer) = sink.lock() {
        for frame in data.chunks(channels as usize) {
            if let Some(sample) = frame.first() {
                buffer.push(*sample);
            }
        }
    }
}

fn push_input_samples_i16(data: &[i16], channels: u16, sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut buffer) = sink.lock() {
        for frame in data.chunks(channels as usize) {
            if let Some(sample) = frame.first() {
                buffer.push((*sample as f32) / (i16::MAX as f32));
            }
        }
    }
}

fn push_input_samples_u16(data: &[u16], channels: u16, sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut buffer) = sink.lock() {
        for frame in data.chunks(channels as usize) {
            if let Some(sample) = frame.first() {
                buffer.push(((*sample as f32) / (u16::MAX as f32)) * 2.0 - 1.0);
            }
        }
    }
}

/// Linear resample mono f32 samples from `src_rate` to `dst_rate`.
///
/// Linear interpolation is fine for speech ASR — Whisper does its own
/// mel-spectrogram extraction and is robust to mild aliasing in this range.
/// Avoiding a heavy resampler dependency keeps build times tight.
fn resample_linear(samples: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = src_rate as f64 / dst_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos.floor() as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        let s0 = samples[src_idx];
        let s1 = if src_idx + 1 < samples.len() {
            samples[src_idx + 1]
        } else {
            s0
        };
        out.push(s0 + (s1 - s0) * frac);
    }

    out
}

/// Trim leading and trailing silence using a simple RMS gate. Operates on
/// 16 kHz mono samples and keeps `VAD_PADDING_SAMPLES` of context around the
/// detected speech region so word onsets aren't clipped.
fn trim_silence(samples: &[f32]) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let window = VAD_WINDOW_SAMPLES.max(1);
    let mut first_voiced: Option<usize> = None;
    let mut last_voiced: Option<usize> = None;

    let mut idx = 0;
    while idx < samples.len() {
        let end = (idx + window).min(samples.len());
        let chunk = &samples[idx..end];
        let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
        if rms >= VAD_THRESHOLD {
            if first_voiced.is_none() {
                first_voiced = Some(idx);
            }
            last_voiced = Some(end);
        }
        idx += window;
    }

    let (Some(start), Some(end)) = (first_voiced, last_voiced) else {
        // No voiced windows detected — caller will treat empty as "no speech".
        return Vec::new();
    };

    let start = start.saturating_sub(VAD_PADDING_SAMPLES);
    let end = (end + VAD_PADDING_SAMPLES).min(samples.len());
    samples[start..end].to_vec()
}

/// Encode mono f32 samples as a 16-bit PCM WAV in memory. No filesystem round
/// trip; this is faster and removes a class of disk-pressure failure modes.
fn encode_wav_in_memory(samples: &[f32], sample_rate_hz: u32) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|error| format!("failed to create wav writer: {error}"))?;
        for sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let pcm = (clamped * (i16::MAX as f32)) as i16;
            writer
                .write_sample(pcm)
                .map_err(|error| format!("failed to write wav sample: {error}"))?;
        }
        writer
            .finalize()
            .map_err(|error| format!("failed to finalize wav writer: {error}"))?;
    }

    Ok(cursor.into_inner())
}
