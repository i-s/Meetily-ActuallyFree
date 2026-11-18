// Retranscription module - allows re-processing stored audio with different settings

use crate::audio::decoder::decode_audio_file;
use crate::audio::vad::get_speech_chunks_with_thresholds_and_progress;
use super::common::{create_transcript_segments, split_segment_at_silence};
use super::constants::AUDIO_EXTENSIONS;
use crate::config::{DEFAULT_WHISPER_MODEL, DEFAULT_PARAKEET_MODEL};
use crate::database::models::DateTimeUtc;
use crate::database::repositories::vocabulary::VocabularyRepository;
use crate::parakeet_engine::ParakeetEngine;
use crate::state::AppState;
use crate::whisper_engine::WhisperEngine;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Global flag to track if retranscription is in progress
static RETRANSCRIPTION_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Global flag to signal cancellation
static RETRANSCRIPTION_CANCELLED: AtomicBool = AtomicBool::new(false);
static ACTIVE_RETRANSCRIPTION: Mutex<Option<(String, String)>> = Mutex::new(None);

/// RAII guard for RETRANSCRIPTION_IN_PROGRESS flag
/// Ensures flag is cleared even if retranscription panics or returns early
struct RetranscriptionGuard;

impl RetranscriptionGuard {
    /// Create guard and set flag atomically
    fn acquire() -> Result<Self, String> {
        if RETRANSCRIPTION_IN_PROGRESS
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("Retranscription already in progress".to_string());
        }
        Ok(RetranscriptionGuard)
    }
}

impl Drop for RetranscriptionGuard {
    fn drop(&mut self) {
        *ACTIVE_RETRANSCRIPTION.lock().unwrap_or_else(|e| e.into_inner()) = None;
        RETRANSCRIPTION_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

/// VAD redemption time in milliseconds - bridges natural pauses in speech
/// Batch processing needs longer redemption (2000ms) than the live pipeline
/// because the entire file is processed at once and short redemption fragments
/// speech at every natural sentence/topic pause (500ms-2s)
const VAD_REDEMPTION_TIME_MS: u32 = 2000;

/// Progress update emitted during retranscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionProgress {
    pub meeting_id: String,
    pub stage: String, // "decoding", "transcribing", "saving"
    pub progress_percentage: u32,
    pub message: String,
}

/// Result of retranscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionResult {
    pub meeting_id: String,
    pub segments_count: usize,
    pub duration_seconds: f64,
    pub language: Option<String>,
    pub version_id: String,
}

/// Error during retranscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionError {
    pub meeting_id: String,
    pub error: String,
}

/// Check if retranscription is currently in progress
pub fn is_retranscription_in_progress() -> bool {
    RETRANSCRIPTION_IN_PROGRESS.load(Ordering::SeqCst)
}

pub(crate) fn refinement_cancelled() -> bool { RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) }

/// Cancel ongoing retranscription
pub fn cancel_retranscription() {
    RETRANSCRIPTION_CANCELLED.store(true, Ordering::SeqCst);
}

/// Start retranscription of a meeting's audio
async fn start_retranscription<R: Runtime>(
    _guard: RetranscriptionGuard,
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    initial_prompt: Option<String>,
    version_id: String,
    reuse_saved_asr: bool,
) -> Result<RetranscriptionResult> {
    let use_parakeet = provider.as_deref() == Some("parakeet");
    let batch_lease = super::common::acquire_stt_batch_lease().await;
    let retained = if reuse_saved_asr {
        let state = app.state::<AppState>();
        let json: Option<String> = sqlx::query_scalar("SELECT segments_json FROM transcript_versions WHERE id=? AND meeting_id=?")
            .bind(&version_id).bind(&meeting_id).fetch_one(state.db_manager.pool()).await?;
        json.map(|json| serde_json::from_str::<Vec<crate::api::TranscriptSegment>>(&json)).transpose()?
    } else { None };
    let result = if let Some(segments) = retained {
        Ok(RetranscriptionResult { meeting_id: meeting_id.clone(), segments_count: segments.len(),
            duration_seconds: segments.iter().filter_map(|s| s.audio_end_time).fold(0.0, f64::max),
            language: language.clone(), version_id: version_id.clone() })
    } else { run_retranscription(
        app.clone(),
        meeting_id.clone(),
        meeting_folder_path,
        language,
        model,
        provider,
        initial_prompt,
        version_id.clone(),
    )
    .await };
    drop(batch_lease);

    // Unload the engine after the batch job (success, failure, or cancellation)
    super::common::unload_engine_after_batch(use_parakeet).await;

    // Guard will automatically clear flag on drop
    // No need for manual: RETRANSCRIPTION_IN_PROGRESS.store(false, Ordering::SeqCst);

    let result = match result {
        Ok(res) => {
            let state = app.state::<AppState>();
            let pool = state.db_manager.pool();
            emit_progress(&app, &meeting_id, "diarizing", 90, "Refining speakers locally (cancellation waits for the current inference)...");
            let refinement = if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
                Err("Retranscription cancelled".into())
            } else {
                crate::diarization::diarize_version(&state, &meeting_id, &version_id).await.map(|_| ())
            };
            match refinement {
                Ok(_) if !RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) => {
                    crate::database::transcript_versions::set_status(pool, &version_id, "ready", None).await.map_err(|e| anyhow!(e))?;
                    Ok(res)
                }
                Ok(_) => Err(anyhow!("Retranscription cancelled")),
                Err(e) => Err(anyhow!(e)),
            }
        }
        Err(e) => Err(e),
    };
    if let Err(error) = &result {
        let state = app.state::<AppState>();
        let status = if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) { "cancelled" } else { "failed" };
        crate::database::transcript_versions::set_status(state.db_manager.pool(), &version_id, status, Some(error.to_string())).await.map_err(|e| anyhow!(e))?;
    }

    match &result {
        Ok(res) => {
            emit_progress(&app, &meeting_id, "complete", 100, "Refined version ready for review; current text unchanged");
            let _ = app.emit(
                "retranscription-complete",
                serde_json::json!({
                    "meeting_id": res.meeting_id,
                    "segments_count": res.segments_count,
                    "duration_seconds": res.duration_seconds,
                    "language": res.language,
                    "version_id": res.version_id
                }),
            );
        }
        Err(_) => {} // The owning task monitor emits every failure, including persistence errors.
    }

    result
}

/// Find audio file in meeting folder
/// Tries common names first, then scans for any file with an audio extension
pub(crate) fn find_audio_file(folder: &Path) -> Result<PathBuf> {
    let candidates = [
        "audio.mp4", "audio.m4a", "audio.wav", "audio.mp3",
        "audio.flac", "audio.ogg", "recording.mp4",
        "audio.mkv", "audio.webm", "audio.wma",
    ];

    for name in candidates {
        let path = folder.join(name);
        if path.exists() {
            return Ok(path);
        }
    }

    // Fallback: scan folder for any file with an audio extension
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                    return Ok(path);
                }
            }
        }
    }

    Err(anyhow!("No audio file found in: {}", folder.display()))
}

struct RetranscriptionSource {
    path: PathBuf,
    label: &'static str,
    source_id: &'static str,
    speaker_hint: Option<&'static str>,
    positive_threshold: f32,
    negative_threshold: f32,
}

pub(crate) fn recording_metadata(folder: &Path) -> Result<serde_json::Value> {
    match std::fs::read(folder.join("metadata.json")) {
        Ok(bytes) => {
            let metadata: serde_json::Value = serde_json::from_slice(&bytes)?;
            if !metadata.is_object() { return Err(anyhow!("Recording metadata is not an object")); }
            Ok(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn recording_in_person(folder: &Path) -> Result<bool> {
    match recording_metadata(folder)?.get("in_person") {
        Some(value) => value.as_bool().ok_or_else(|| anyhow!("Invalid in_person recording provenance")),
        None => Ok(false),
    }
}

fn find_retranscription_sources(folder: &Path, fallback: &Path) -> Result<Vec<RetranscriptionSource>> {
    let in_person = recording_in_person(folder)?;
    let mic_path = folder.join("mic.mp4");
    let system_path = folder.join("system.mp4");
    let mut sources = Vec::new();

    if in_person {
        return Ok(vec![RetranscriptionSource {
            path: if mic_path.is_file() { mic_path } else { fallback.to_path_buf() },
            label: "room microphone",
            source_id: "room",
            speaker_hint: Some("Microphone"),
            positive_threshold: 0.20,
            negative_threshold: 0.10,
        }]);
    }
    let metadata = recording_metadata(folder)?;
    let retained_call_sources = ["mic_audio_file", "system_audio_file"].iter()
        .any(|key| metadata.get(key).is_some_and(|value| !value.is_null()));
    if retained_call_sources && !(mic_path.is_file() && system_path.is_file()) {
        return Err(anyhow!("A retained call source track is missing. Restore both microphone and system tracks before refinement."));
    }
    if mic_path.is_file() != system_path.is_file() {
        return Err(anyhow!("A retained call source track is missing. Restore both microphone and system tracks before refinement; mixed fallback could lose a speaker."));
    }
    if mic_path.is_file() && system_path.is_file() {
        sources.push(RetranscriptionSource {
            path: mic_path,
            label: "microphone",
            source_id: "microphone",
            speaker_hint: Some("You"),
            positive_threshold: 0.20,
            negative_threshold: 0.10,
        });
        sources.push(RetranscriptionSource {
            path: system_path,
            label: "system audio",
            source_id: "system",
            speaker_hint: Some("Guest"),
            positive_threshold: 0.50,
            negative_threshold: 0.35,
        });
    }
    if sources.is_empty() {
        sources.push(RetranscriptionSource {
            path: fallback.to_path_buf(),
            label: "mixed audio",
            source_id: "mixed",
            speaker_hint: None,
            positive_threshold: 0.50,
            negative_threshold: 0.35,
        });
    }

    Ok(sources)
}

pub(crate) fn saved_sources(folder: &Path) -> Result<()> {
    let fallback = find_audio_file(folder)?;
    let sources = find_retranscription_sources(folder, &fallback)?;
    if sources.iter().all(|s| s.path.is_file()) { Ok(()) } else { Err(anyhow!("Saved audio is unavailable")) }
}

fn create_source_labeled_segments(
    transcripts: &[(String, f64, f64)],
    speaker_hints: &[Option<&str>],
    recording_started_at: DateTime<Utc>,
) -> Result<Vec<crate::api::TranscriptSegment>> {
    debug_assert_eq!(transcripts.len(), speaker_hints.len());
    let mut segments = create_transcript_segments(transcripts, recording_started_at)?;
    for (segment, speaker_hint) in segments.iter_mut().zip(speaker_hints) {
        segment.speaker = speaker_hint.map(str::to_string);
    }
    Ok(segments)
}

/// Internal function to run retranscription
async fn run_retranscription<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    initial_prompt: Option<String>,
    version_id: String,
) -> Result<RetranscriptionResult> {
    if super::recording_commands::is_recording_active() || super::recording_commands::is_recording_stopping() {
        return Err(anyhow!("Recording has priority. Retry post-meeting refinement after recording stops."));
    }
    let folder_path = PathBuf::from(&meeting_folder_path);
    let audio_path = find_audio_file(&folder_path)?;
    let sources = find_retranscription_sources(&folder_path, &audio_path)?;

    // Determine which provider to use (default to whisper)
    let use_parakeet = provider.as_deref() == Some("parakeet");

    info!(
        "Starting retranscription for meeting {} with language {:?}, model {:?}, provider {:?}",
        meeting_id, language, model, provider
    );

    let source_count = sources.len();
    let mut duration_seconds = 0.0f64;
    let mut speech_segments = Vec::new();

    // Retained source tracks prevent one speaker from masking the other. Older
    // recordings fall back to the mixed playback file.
    for (source_index, source) in sources.into_iter().enumerate() {
        if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Retranscription cancelled"));
        }

        emit_progress(
            &app,
            &meeting_id,
            "decoding",
            (5.0 + (source_index as f32 / source_count as f32) * 20.0) as u32,
            &format!("Decoding {}...", source.label),
        );

        let path_for_decode = source.path.clone();
        let decoded = tokio::task::spawn_blocking(move || decode_audio_file(&path_for_decode))
            .await
            .map_err(|e| anyhow!("Decode task panicked: {}", e))??;
        duration_seconds = duration_seconds.max(decoded.duration_seconds);
        info!(
            "Decoded {}: {:.2}s, {}Hz, {} channels",
            source.label, decoded.duration_seconds, decoded.sample_rate, decoded.channels
        );

        let audio_samples = tokio::task::spawn_blocking(move || decoded.to_whisper_format())
            .await
            .map_err(|e| anyhow!("Resample task panicked: {}", e))?;
        let app_for_vad = app.clone();
        let meeting_id_for_vad = meeting_id.clone();
        let source_label = source.label;
        let speaker_hint = source.speaker_hint;
        let source_id = source.source_id;
        let positive_threshold = source.positive_threshold;
        let negative_threshold = source.negative_threshold;
        let source_progress_span = 20.0 / source_count as f32;
        let source_progress_start = 5.0
            + source_index as f32 * source_progress_span
            + source_progress_span / 2.0;
        let vad_progress_span = source_progress_span / 2.0;

        let mut source_segments = tokio::task::spawn_blocking(move || {
            get_speech_chunks_with_thresholds_and_progress(
                &audio_samples,
                VAD_REDEMPTION_TIME_MS,
                positive_threshold,
                negative_threshold,
                |vad_progress, segments_found| {
                    emit_progress(
                        &app_for_vad,
                        &meeting_id_for_vad,
                        "vad",
                        (source_progress_start
                            + vad_progress_span * vad_progress as f32 / 100.0)
                            as u32,
                        &format!(
                            "Detecting {} speech... {}% ({} found)",
                            source_label, vad_progress, segments_found
                        ),
                    );
                    !RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst)
                },
            )
        })
        .await
        .map_err(|e| anyhow!("VAD task panicked: {}", e))?
        .map_err(|e| anyhow!("VAD processing failed for {}: {}", source_label, e))?;

        info!("VAD detected {} {} segments", source_segments.len(), source_label);
        speech_segments.extend(
            source_segments
                .drain(..)
                .map(|segment| (segment, speaker_hint, source_id)),
        );
    }

    speech_segments.sort_by(|a, b| {
        a.0.start_timestamp_ms
            .partial_cmp(&b.0.start_timestamp_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_segments = speech_segments.len();
    info!("VAD detected {} speech segments (redemption_time={}ms)", total_segments, VAD_REDEMPTION_TIME_MS);

    // Diagnostic: log segment duration distribution
    if !speech_segments.is_empty() {
        let durations_ms: Vec<f64> = speech_segments.iter()
            .map(|(s, _, _)| s.end_timestamp_ms - s.start_timestamp_ms)
            .collect();
        let total_speech_ms: f64 = durations_ms.iter().sum();
        let avg_duration = total_speech_ms / durations_ms.len() as f64;
        let min_duration = durations_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_duration = durations_ms.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        info!(
            "VAD segment stats: avg={:.0}ms, min={:.0}ms, max={:.0}ms, total_speech={:.1}s/{:.1}s ({:.0}%)",
            avg_duration, min_duration, max_duration,
            total_speech_ms / 1000.0, duration_seconds,
            (total_speech_ms / 1000.0 / duration_seconds) * 100.0
        );
        // Log first 10 segments for detailed inspection
        for (i, (seg, speaker_hint, _)) in speech_segments.iter().take(10).enumerate() {
            let dur = seg.end_timestamp_ms - seg.start_timestamp_ms;
            debug!("  Segment {}: {:.0}ms-{:.0}ms ({:.0}ms, {} samples, {:?})",
                i, seg.start_timestamp_ms, seg.end_timestamp_ms, dur, seg.samples.len(), speaker_hint);
        }
        if total_segments > 10 {
            debug!("  ... and {} more segments", total_segments - 10);
        }
    }

    if total_segments == 0 {
        warn!("No speech detected in audio");
        return Err(anyhow!("No speech detected in audio file"));
    }

    emit_progress(&app, &meeting_id, "transcribing", 25, "Loading transcription engine...");

    // Initialize the appropriate engine once (not per-segment)
    let whisper_engine = if !use_parakeet {
        Some(get_or_init_whisper(&app, model.as_deref()).await?)
    } else {
        None
    };
    let parakeet_engine = if use_parakeet {
        Some(get_or_init_parakeet(&app, model.as_deref()).await?)
    } else {
        None
    };

    // Split very long segments at silence boundaries for better transcription quality.
    // Hard cuts at arbitrary sample positions lose words at boundaries. Instead, scan
    // for the lowest-energy window near the target split point and cut there.
    const MAX_SEGMENT_SAMPLES: usize = 25 * 16000; // 25 seconds at 16kHz

    let mut processable_segments: Vec<(crate::audio::vad::SpeechSegment, Option<&'static str>, &'static str)> = Vec::new();
    for (segment, speaker_hint, source_id) in &speech_segments {
        if segment.samples.len() > MAX_SEGMENT_SAMPLES {
            debug!(
                "Splitting large segment ({:.0}ms, {} samples) at silence boundaries",
                segment.end_timestamp_ms - segment.start_timestamp_ms,
                segment.samples.len()
            );

            let sub_segments = split_segment_at_silence(segment, MAX_SEGMENT_SAMPLES);
            debug!("Split into {} sub-segments", sub_segments.len());
            processable_segments.extend(
                sub_segments
                    .into_iter()
                    .map(|segment| (segment, *speaker_hint, *source_id)),
            );
        } else {
            processable_segments.push((segment.clone(), *speaker_hint, *source_id));
        }
    }

    let processable_count = processable_segments.len();
    info!("Processing {} segments (after splitting)", processable_count);

    // Process each speech segment with progress updates
    let mut all_transcripts: Vec<(String, f64, f64)> = Vec::new(); // (text, start_ms, end_ms)
    let mut speaker_hints: Vec<Option<&'static str>> = Vec::new();
    let mut transcript_sources: Vec<&'static str> = Vec::new();
    let mut total_confidence = 0.0f32;

    for (i, (segment, speaker_hint, source_id)) in processable_segments.iter().enumerate() {
        // Check for cancellation before each segment
        if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
            return Err(anyhow!("Retranscription cancelled"));
        }

        // Calculate progress (25% to 80% range for transcription)
        let progress = 25 + ((i as f32 / processable_count as f32) * 55.0) as u32;
        let segment_duration_sec = (segment.end_timestamp_ms - segment.start_timestamp_ms) / 1000.0;
        emit_progress(
            &app,
            &meeting_id,
            "transcribing",
            progress,
            &format!(
                "Transcribing segment {} of {} ({:.1}s)...",
                i + 1,
                processable_count,
                segment_duration_sec
            ),
        );

        // Skip very short segments (< 100ms of audio = 1600 samples at 16kHz)
        if segment.samples.len() < 1600 {
            debug!("Skipping short segment {} with {} samples", i, segment.samples.len());
            continue;
        }

        // Skip silent segments (RMS < 0.005 and peak < 0.01) to prevent Whisper silence hallucinations (e.g. "you", "thank you")
        let sum_sq: f32 = segment.samples.iter().map(|&x| x * x).sum();
        let rms = (sum_sq / segment.samples.len() as f32).sqrt();
        let peak = segment.samples.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        if rms < 0.005 && peak < 0.01 {
            debug!("Skipping silent segment {} with RMS {:.5}, peak {:.5}", i, rms, peak);
            continue;
        }

        // Transcribe this segment
        let (text, conf) = if use_parakeet {
            let engine = parakeet_engine.as_ref().unwrap();
            let text = engine
                .transcribe_audio(segment.samples.clone())
                .await
                .map_err(|e| anyhow!("Parakeet transcription failed on segment {}: {}", i, e))?;
            (text, 0.9f32)
        } else {
            let engine = whisper_engine.as_ref().unwrap();
            let (text, conf, _) = engine
                .transcribe_audio_with_confidence(
                    segment.samples.clone(),
                    language.clone(),
                    initial_prompt.as_deref(),
                )
                .await
                .map_err(|e| anyhow!("Whisper transcription failed on segment {}: {}", i, e))?;
            (text, conf)
        };

        // Skip empty transcripts
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            // Filter out known Whisper silence hallucinations if energy is low
            let lower = trimmed.to_lowercase();
            let is_hallucination = (lower == "you" || lower == "you." || lower == "thank you." || lower == "thank you" || lower == "thanks." || lower == "bye." || lower == "bye") && rms < 0.02;
            if is_hallucination {
                warn!("Dropping suspected silence hallucination on segment {}: '{}' (rms={:.5}, conf={:.2})", i + 1, trimmed, rms, conf);
                continue;
            }

            debug!(
                "Segment {}/{}: {:.1}s, conf={:.2}, text='{}'",
                i + 1, processable_count, segment_duration_sec, conf,
                if trimmed.len() > 80 { let mut end = 80; while !trimmed.is_char_boundary(end) { end -= 1; } &trimmed[..end] } else { trimmed }
            );
            all_transcripts.push((text, segment.start_timestamp_ms, segment.end_timestamp_ms));
            speaker_hints.push(*speaker_hint);
            transcript_sources.push(*source_id);
            total_confidence += conf;
        } else {
            debug!("Segment {}/{}: {:.1}s — empty transcription", i + 1, processable_count, segment_duration_sec);
        }
    }

    let transcribed_count = all_transcripts.len();
    let avg_confidence = if transcribed_count > 0 {
        total_confidence / transcribed_count as f32
    } else {
        0.0
    };

    info!(
        "Transcription complete: {} segments transcribed out of {}, avg confidence: {:.2}",
        transcribed_count, processable_count, avg_confidence
    );

    // Check for cancellation
    if RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst) {
        return Err(anyhow!("Retranscription cancelled"));
    }

    emit_progress(&app, &meeting_id, "saving", 80, "Saving transcripts...");

    // Save to database
    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| anyhow!("App state not available"))?;

    // Candidate JSON is independent from selected rows and recording recovery files.
    let pool = app_state.db_manager.pool();
    let stored_recording_start: DateTimeUtc =
        sqlx::query_scalar("SELECT created_at FROM meetings WHERE id = ?")
            .bind(&meeting_id)
            .fetch_one(pool)
            .await
            .map_err(|e| anyhow!("Failed to load meeting recording start: {}", e))?;
    let recording_started_at = crate::api::recording_started_at_from_folder(&meeting_folder_path)
        .unwrap_or(stored_recording_start.0);

    let segments = create_source_labeled_segments(&all_transcripts, &speaker_hints, recording_started_at)?;
    if segments.is_empty() { return Err(anyhow!("No transcript produced; previous version retained")); }
    let source_map = segments.iter().zip(transcript_sources).map(|(segment, source)| (segment.id.clone(), source.to_string())).collect();
    crate::database::transcript_versions::finish_with_sources(pool, &version_id, &segments, &source_map).await.map_err(|e| anyhow!(e))?;

    emit_progress(&app, &meeting_id, "saving", 85, "ASR candidate saved; speaker refinement is next");

    Ok(RetranscriptionResult {
        meeting_id,
        segments_count: segments.len(),
        duration_seconds,
        language,
        version_id,
    })
}

/// Emit progress event
fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    meeting_id: &str,
    stage: &str,
    progress: u32,
    message: &str,
) {
    let _ = app.emit(
        "retranscription-progress",
        RetranscriptionProgress {
            meeting_id: meeting_id.to_string(),
            stage: stage.to_string(),
            progress_percentage: progress,
            message: message.to_string(),
        },
    );
}

/// Get or initialize the Whisper engine, auto-loading the model if needed
/// If `requested_model` is provided, ensures that specific model is loaded
async fn get_or_init_whisper<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<WhisperEngine>> {
    use crate::whisper_engine::commands::WHISPER_ENGINE;

    let engine = {
        let guard = WHISPER_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            // Determine which model to use
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_whisper_model(app).await?,
            };

            // Check if the correct model is already loaded
            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Whisper model '{}' (current: {:?})",
                    target_model, current_model
                );

                // Discover available models first (populates the internal cache)
                info!("Discovering available Whisper models...");
                if let Err(discover_err) = e.discover_models().await {
                    warn!("Error during model discovery (continuing anyway): {}", discover_err);
                }

                match e.load_model(&target_model).await {
                    Ok(_) => {
                        info!("Whisper model '{}' loaded successfully", target_model);
                        Ok(e)
                    }
                    Err(load_err) => {
                        error!("Failed to load Whisper model '{}': {}", target_model, load_err);
                        Err(anyhow!("Failed to load Whisper model '{}': {}", target_model, load_err))
                    }
                }
            } else {
                info!("Whisper model '{}' already loaded", target_model);
                Ok(e)
            }
        }
        None => Err(anyhow!("Whisper engine not initialized")),
    }
}

/// Get the configured Whisper model name from the database
async fn get_configured_whisper_model<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    debug!("Getting configured Whisper model from database...");

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| {
            error!("App state not available");
            anyhow!("App state not available")
        })?;

    debug!("Querying transcript_settings table...");

    // Query the transcript settings from the database - get both provider and model
    let result: Option<(String, String)> = sqlx::query_as(
        "SELECT provider, model FROM transcript_settings WHERE id = '1'"
    )
    .fetch_optional(app_state.db_manager.pool())
    .await
    .map_err(|e| {
        error!("Failed to query transcript config: {}", e);
        anyhow!("Failed to query transcript config: {}", e)
    })?;

    match result {
        Some((provider, model)) => {
            info!("Found transcript config: provider={}, model={}", provider, model);

            // Check if provider is Whisper-based
            if provider == "localWhisper" || provider == "whisper" {
                Ok(model)
            } else {
                error!("Retranscription requires Whisper provider, but configured provider is: {}", provider);
                Err(anyhow!("Retranscription requires Whisper. Current provider '{}' does not support retranscription with language selection.", provider))
            }
        },
        None => {
            // Default to configured Whisper model if no config exists
            warn!("No transcript config found, using default model '{}'", DEFAULT_WHISPER_MODEL);
            Ok(DEFAULT_WHISPER_MODEL.to_string())
        }
    }
}

/// Get or initialize the Parakeet engine, auto-loading the model if needed
async fn get_or_init_parakeet<R: Runtime>(
    app: &AppHandle<R>,
    requested_model: Option<&str>,
) -> Result<Arc<ParakeetEngine>> {
    use crate::parakeet_engine::commands::PARAKEET_ENGINE;

    let engine = {
        let guard = PARAKEET_ENGINE.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().cloned()
    };

    match engine {
        Some(e) => {
            // Determine which model to use
            let target_model = match requested_model {
                Some(model) => model.to_string(),
                None => get_configured_parakeet_model(app).await?,
            };

            // Check if the correct model is already loaded
            let current_model = e.get_current_model().await;
            let needs_load = match &current_model {
                Some(loaded) => loaded != &target_model,
                None => true,
            };

            if needs_load {
                info!(
                    "Loading Parakeet model '{}' (current: {:?})",
                    target_model, current_model
                );

                // Discover available models first
                info!("Discovering available Parakeet models...");
                if let Err(discover_err) = e.discover_models().await {
                    warn!("Error during Parakeet model discovery (continuing anyway): {}", discover_err);
                }

                match e.load_model(&target_model).await {
                    Ok(_) => {
                        info!("Parakeet model '{}' loaded successfully", target_model);
                        Ok(e)
                    }
                    Err(load_err) => {
                        error!("Failed to load Parakeet model '{}': {}", target_model, load_err);
                        Err(anyhow!("Failed to load Parakeet model '{}': {}", target_model, load_err))
                    }
                }
            } else {
                info!("Parakeet model '{}' already loaded", target_model);
                Ok(e)
            }
        }
        None => Err(anyhow!("Parakeet engine not initialized")),
    }
}

/// Get the configured Parakeet model name from the database
async fn get_configured_parakeet_model<R: Runtime>(app: &AppHandle<R>) -> Result<String> {
    debug!("Getting configured Parakeet model from database...");

    let app_state = app
        .try_state::<AppState>()
        .ok_or_else(|| {
            error!("App state not available");
            anyhow!("App state not available")
        })?;

    // Query the transcript settings from the database
    let result: Option<(String, String)> = sqlx::query_as(
        "SELECT provider, model FROM transcript_settings WHERE id = '1'"
    )
    .fetch_optional(app_state.db_manager.pool())
    .await
    .map_err(|e| {
        error!("Failed to query transcript config: {}", e);
        anyhow!("Failed to query transcript config: {}", e)
    })?;

    match result {
        Some((provider, model)) => {
            info!("Found transcript config: provider={}, model={}", provider, model);

            if provider == "parakeet" {
                Ok(model)
            } else {
                // Default to configured Parakeet model
                warn!("Configured provider is not Parakeet, using default model");
                Ok(DEFAULT_PARAKEET_MODEL.to_string())
            }
        },
        None => {
            // Default to configured Parakeet model if no config exists
            warn!("No transcript config found, using default Parakeet model");
            Ok(DEFAULT_PARAKEET_MODEL.to_string())
        }
    }
}

// Tauri commands

/// Response when retranscription is started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetranscriptionStarted {
    pub version_id: String,
    pub meeting_id: String,
    pub message: String,
}

// Normal meeting-details entrypoint; only local engines and owned saved audio.
#[tauri::command]
pub async fn start_retranscription_command<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
    meeting_folder_path: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    vocabulary_terms: Option<String>,
    vocabulary_scope: Option<String>,
) -> Result<RetranscriptionStarted, String> {

    if !matches!(provider.as_deref(), None | Some("whisper") | Some("parakeet") | Some("localWhisper")) {
        return Err("Post-meeting ASR supports only local Whisper or Parakeet".into());
    }
    let state = app.try_state::<AppState>().ok_or("Database not initialized")?;
    let stored_folder: Option<String> = sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(&meeting_id).fetch_one(state.db_manager.pool()).await.map_err(|e| e.to_string())?;
    if stored_folder.as_deref() != Some(meeting_folder_path.as_str()) { return Err("Recording folder does not belong to this meeting".into()); }
    saved_sources(Path::new(&meeting_folder_path)).map_err(|e| e.to_string())?;
    if super::recording_commands::is_recording_active() || super::recording_commands::is_recording_stopping() {
        return Err("Finish the current recording before post-meeting refinement".into());
    }
    if !recording_in_person(Path::new(&meeting_folder_path)).map_err(|e| e.to_string())? && !crate::diarization::diarization_models_available().await? { return Err("Download the selected local diarization model before refinement".into()); }

    // Reserve before returning so duplicate requests and cancellation also see
    // jobs queued behind an offline diarization pass.
    let guard = RetranscriptionGuard::acquire()?;
    RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);

    let use_parakeet = provider.as_deref() == Some("parakeet");
    let initial_prompt = if use_parakeet {
        if vocabulary_terms
            .as_deref()
            .is_some_and(|terms| !terms.trim().is_empty())
        {
            return Err("Vocabulary hints are only supported by Whisper".to_string());
        }
        None
    } else {
        let state = app
            .try_state::<AppState>()
            .ok_or_else(|| "Database not initialized".to_string())?;
        let pool = state.db_manager.pool();
        if let Some(terms) = vocabulary_terms
            .as_deref()
            .filter(|terms| !terms.trim().is_empty())
        {
            match vocabulary_scope.as_deref().unwrap_or("meeting") {
                "meeting" => {
                    VocabularyRepository::add_meeting(pool, &meeting_id, terms).await?;
                }
                "global" => {
                    VocabularyRepository::add_global(pool, terms).await?;
                }
                _ => return Err("Invalid vocabulary scope".to_string()),
            }
        }
        VocabularyRepository::get_effective(pool, Some(&meeting_id))
            .await
            .map_err(|error| error.to_string())?
    };

    let model = Some(match model {
        Some(model) => model,
        None if use_parakeet => get_configured_parakeet_model(&app).await.map_err(|e| e.to_string())?,
        None => get_configured_whisper_model(&app).await.map_err(|e| e.to_string())?,
    });
    let recording_metadata = recording_metadata(Path::new(&meeting_folder_path)).map_err(|e| e.to_string())?;
    let mut diarization_config = crate::diarization::get_diarization_config();
    // Candidate auto-detection uses the supported eight-speaker capacity, never
    // a stale count preference which could suppress a remote track.
    diarization_config.nemotron_max_speakers = 8;
    let version_id = crate::database::transcript_versions::begin(state.db_manager.pool(), &meeting_id,
        &serde_json::json!({"provider": provider, "model": model, "language": language, "vocabulary_prompt": initial_prompt, "recording_metadata": recording_metadata, "label_origin": "generated_asr", "diarization_engine": diarization_config.engine, "diarization_config": diarization_config})).await?;
    *ACTIVE_RETRANSCRIPTION.lock().unwrap_or_else(|e| e.into_inner()) = Some((meeting_id.clone(), version_id.clone()));
    let response_version = version_id.clone();
    drop(state);

    // Clone values for the spawned task
    let meeting_id_clone = meeting_id.clone();

    // Spawn the retranscription in a background task
    tauri::async_runtime::spawn(async move {
        let _operation_guard = crate::diarization::operation_guard().await;
        let monitor_app = app.clone();
        let monitor_meeting = meeting_id_clone.clone();
        let monitor_version = version_id.clone();
        let result = tauri::async_runtime::spawn(start_retranscription(
            guard, app, meeting_id_clone, meeting_folder_path, language,
            model, provider, initial_prompt, version_id, false,
        )).await;
        let failure = match result {
            Err(join_error) => Some(format!("Local refinement worker stopped: {join_error}")),
            Ok(Err(error)) => Some(error.to_string()),
            Ok(Ok(_)) => None,
        };
        if let Some(error) = failure {
            error!("Retranscription failed: {}", error);
            let state = monitor_app.state::<AppState>();
            let status = if error.contains("cancelled") { "cancelled" } else { "failed" };
            // A DB write failure must still terminate the UI wait. If storage
            // remains unavailable, restart recovery will mark this interrupted.
            let _ = crate::database::transcript_versions::set_status(state.db_manager.pool(), &monitor_version, status, Some(error.clone())).await;
            let _ = monitor_app.emit("retranscription-error", RetranscriptionError { meeting_id: monitor_meeting, error });
        }
    });

    Ok(RetranscriptionStarted {
        version_id: response_version,
        meeting_id,
        message: "Retranscription started".to_string(),
    })
}

/// Awaited saved-meeting operation for the durable queue. No live-session DTO,
/// generic provider, detached JS promise or candidate selection is involved.
/// Caller owns recording-priority admission until this returns, including drain.
pub(crate) async fn run_queued_refinement<R: Runtime>(
    app: AppHandle<R>, meeting_id: String,
    token: &tokio_util::sync::CancellationToken,
) -> Result<String, String> {
    use crate::database::{transcript_versions as versions, repositories::setting::SettingsRepository};
    let state = app.state::<AppState>();
    let pool = state.db_manager.pool();
    let id = format!("post-call-refined:{meeting_id}");
    let existing: Option<(String, String)> = sqlx::query_as("SELECT status,settings_json FROM transcript_versions WHERE id=? AND meeting_id=?")
        .bind(&id).bind(&meeting_id).fetch_optional(pool).await.map_err(|e|e.to_string())?;
    if existing.as_ref().is_some_and(|(status,_)| status=="ready") { return Ok(id); }
    if token.is_cancelled() { return Err("Post-call work yielded to recording".into()); }
    let guard = RetranscriptionGuard::acquire()?;
    RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);
    let _operation_guard = tokio::select! {
        guard = crate::diarization::operation_guard() => guard,
        _ = token.cancelled() => return Err("Post-call work yielded before offline operation admission".into()),
    };
    if token.is_cancelled() { return Err("Post-call work yielded to recording".into()); }
    let folder: Option<String> = sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id=?")
        .bind(&meeting_id).fetch_one(pool).await.map_err(|e|e.to_string())?;
    let folder=folder.ok_or("Saved recording folder is unavailable")?;
    saved_sources(Path::new(&folder)).map_err(|e|e.to_string())?;
    let settings = if let Some((_,json))=existing {
        serde_json::from_str::<serde_json::Value>(&json).map_err(|e|e.to_string())?
    } else {
        let configured=SettingsRepository::get_post_call_transcript_config(pool).await.map_err(|e|e.to_string())?;
        let (provider,model)=match configured {
            Some((p,m)) if p!="live" => (p,m),
            _ => sqlx::query_as::<_,(String,String)>("SELECT provider,model FROM transcript_settings WHERE id='1'").fetch_optional(pool).await.map_err(|e|e.to_string())?
                .ok_or("Choose a downloaded local post-call ASR model in Settings")?,
        };
        if !matches!(provider.as_str(),"whisper"|"localWhisper"|"parakeet") {return Err("Automatic post-call ASR requires a local Whisper or Parakeet model; no provider fallback is allowed".into());}
        let vocabulary=if provider=="parakeet" {None} else {VocabularyRepository::get_effective(pool,Some(&meeting_id)).await.map_err(|e|e.to_string())?};
        let mut config=crate::diarization::get_diarization_config();config.nemotron_max_speakers=8;
        serde_json::json!({"provider":provider,"model":model,"language":null,"vocabulary_prompt":vocabulary,
            "recording_metadata":recording_metadata(Path::new(&folder)).map_err(|e|e.to_string())?,
            "label_origin":"generated_asr","diarization_engine":config.engine,"diarization_config":config,"post_call_job":meeting_id})
    };
    if !recording_in_person(Path::new(&folder)).map_err(|e|e.to_string())? && !crate::diarization::diarization_models_available().await? {
        return Err("Download the selected local diarization model before post-call processing".into());
    }
    let version=versions::begin_automatic(pool,&meeting_id,&settings).await?;
    *ACTIVE_RETRANSCRIPTION.lock().unwrap_or_else(|e|e.into_inner())=Some((meeting_id.clone(),version.clone()));
    let string=|key:&str|settings.get(key).and_then(|v|v.as_str()).map(str::to_owned);
    let run=start_retranscription(guard,app.clone(),meeting_id,folder,string("language"),string("model"),string("provider"),string("vocabulary_prompt"),version.clone(),true);
    tokio::pin!(run);
    let result=tokio::select! {
        result=&mut run => result,
        _=token.cancelled()=> { cancel_retranscription(); run.await }
    };
    if let Err(error) = &result {
        // Also cover early checkpoint/storage failures before the shared worker
        // reaches its ordinary terminal-status block. Never leave a failed
        // awaited queue operation looking like a live native inference.
        let status = if token.is_cancelled() || refinement_cancelled() { "cancelled" } else { "failed" };
        let _ = versions::set_status(pool, &version, status, Some(error.to_string())).await;
    }
    result.map(|_|version).map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn cancel_retranscription_command(meeting_id: Option<String>, version_id: Option<String>) -> Result<(), String> {
    if !is_retranscription_in_progress() {
        return Err("No retranscription in progress".to_string());
    }
    let active = ACTIVE_RETRANSCRIPTION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((meeting, version)) = active.as_ref() {
        if meeting_id.as_ref().is_some_and(|id| id != meeting) || version_id.as_ref().is_some_and(|id| id != version) {
            return Err("This refinement is no longer the active job; refresh its saved status".into());
        }
    } else { return Err("Refinement is still being reserved; retry cancellation".into()); }
    cancel_retranscription();
    Ok(())
}

#[tauri::command]
pub async fn is_retranscription_in_progress_command() -> bool {
    is_retranscription_in_progress()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_recording_start() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn test_create_transcript_segments_empty() {
        let transcripts: Vec<(String, f64, f64)> = vec![];
        let segments = create_transcript_segments(&transcripts, test_recording_start()).unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn test_create_transcript_segments_single() {
        let transcripts = vec![
            ("Hello world".to_string(), 0.0, 1500.0), // 0-1.5 seconds
        ];
        let segments = create_transcript_segments(&transcripts, test_recording_start()).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[0].audio_end_time, Some(1.5));
        assert_eq!(segments[0].duration, Some(1.5));
        assert_eq!(segments[0].timestamp, "2026-08-30T12:00:00.000Z");
    }

    #[test]
    fn test_create_transcript_segments_multiple() {
        let transcripts = vec![
            ("First segment".to_string(), 0.0, 2000.0),      // 0-2 seconds
            ("Second segment".to_string(), 3000.0, 5000.0),  // 3-5 seconds
            ("Third segment".to_string(), 6500.0, 8000.0),   // 6.5-8 seconds
        ];
        let segments = create_transcript_segments(&transcripts, test_recording_start()).unwrap();

        assert_eq!(segments.len(), 3);

        // First segment
        assert_eq!(segments[0].text, "First segment");
        assert_eq!(segments[0].audio_start_time, Some(0.0));
        assert_eq!(segments[0].audio_end_time, Some(2.0));
        assert_eq!(segments[0].duration, Some(2.0));

        // Second segment
        assert_eq!(segments[1].text, "Second segment");
        assert_eq!(segments[1].audio_start_time, Some(3.0));
        assert_eq!(segments[1].audio_end_time, Some(5.0));
        assert_eq!(segments[1].duration, Some(2.0));

        // Third segment
        assert_eq!(segments[2].text, "Third segment");
        assert_eq!(segments[2].audio_start_time, Some(6.5));
        assert_eq!(segments[2].audio_end_time, Some(8.0));
        assert_eq!(segments[2].duration, Some(1.5));
        assert_eq!(segments[2].timestamp, "2026-08-30T12:00:06.500Z");
    }

    #[test]
    fn test_create_transcript_segments_trims_whitespace() {
        let transcripts = vec![
            ("  Hello with spaces  ".to_string(), 0.0, 1000.0),
        ];
        let segments = create_transcript_segments(&transcripts, test_recording_start()).unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello with spaces");
    }

    #[test]
    fn test_create_transcript_segments_generates_unique_ids() {
        let transcripts = vec![
            ("Segment one".to_string(), 0.0, 1000.0),
            ("Segment two".to_string(), 1000.0, 2000.0),
        ];
        let segments = create_transcript_segments(&transcripts, test_recording_start()).unwrap();

        assert_eq!(segments.len(), 2);
        assert_ne!(segments[0].id, segments[1].id);
        assert!(segments[0].id.starts_with("transcript-"));
        assert!(segments[1].id.starts_with("transcript-"));
    }

    #[test]
    fn test_cancellation_flag() {
        // Reset flag to known state
        RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);
        RETRANSCRIPTION_IN_PROGRESS.store(false, Ordering::SeqCst);

        assert!(!is_retranscription_in_progress());

        // Test cancellation
        cancel_retranscription();
        assert!(RETRANSCRIPTION_CANCELLED.load(Ordering::SeqCst));

        // Reset for other tests
        RETRANSCRIPTION_CANCELLED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_vad_redemption_time_constant() {
        // Batch processing uses 2000ms to bridge natural pauses in full-file VAD
        assert_eq!(VAD_REDEMPTION_TIME_MS, 2000);
    }

    #[test]
    fn test_find_audio_file_common_candidates() {
        let dir = tempfile::tempdir().unwrap();

        // No audio file → error
        assert!(find_audio_file(dir.path()).is_err());

        // Create audio.mp4 — should be found first
        std::fs::write(dir.path().join("audio.mp4"), b"fake").unwrap();
        let found = find_audio_file(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "audio.mp4");
    }

    #[test]
    fn dual_track_sources_preserve_deterministic_speaker_hints() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mic.mp4"), b"mic").unwrap();
        std::fs::write(dir.path().join("system.mp4"), b"system").unwrap();
        let fallback = dir.path().join("audio.mp4");
        let sources = find_retranscription_sources(dir.path(), &fallback).unwrap();

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].speaker_hint, Some("You"));
        assert_eq!(sources[1].speaker_hint, Some("Guest"));
    }

    #[test]
    fn mixed_source_has_no_speaker_hint() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("audio.mp4");
        let sources = find_retranscription_sources(dir.path(), &fallback).unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].speaker_hint, None);
    }

    #[test]
    fn source_hints_stay_aligned_with_successful_transcripts() {
        let transcripts = vec![
            ("Hello one two three".to_string(), 14_970.0, 18_880.0),
            ("Remote speech".to_string(), 16_770.0, 26_980.0),
        ];
        let segments = create_source_labeled_segments(
            &transcripts,
            &[Some("You"), Some("Guest")],
            test_recording_start(),
        )
        .unwrap();

        assert_eq!(segments[0].speaker.as_deref(), Some("You"));
        assert_eq!(segments[1].speaker.as_deref(), Some("Guest"));
        assert_eq!(segments[0].audio_start_time, Some(14.97));
        assert_eq!(segments[0].timestamp, "2026-08-30T12:00:14.970Z");
    }

    #[test]
    fn retained_tracks_are_transcribed_separately() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("audio.mp4");
        std::fs::write(&fallback, b"mixed").unwrap();
        std::fs::write(dir.path().join("mic.mp4"), b"mic").unwrap();
        std::fs::write(dir.path().join("system.mp4"), b"system").unwrap();

        let sources = find_retranscription_sources(dir.path(), &fallback).unwrap();

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].label, "microphone");
        assert_eq!(sources[0].positive_threshold, 0.20);
        assert_eq!(sources[0].negative_threshold, 0.10);
        assert_eq!(sources[1].label, "system audio");
    }

    #[test]
    fn old_recordings_fall_back_to_mixed_audio() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("audio.mp4");
        std::fs::write(&fallback, b"mixed").unwrap();

        let sources = find_retranscription_sources(dir.path(), &fallback).unwrap();

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].label, "mixed audio");
        assert_eq!(sources[0].path, fallback);
    }

    #[test]
    fn incomplete_retained_tracks_reject_unsafe_mixed_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("audio.mp4");
        std::fs::write(&fallback, b"mixed").unwrap();
        std::fs::write(dir.path().join("mic.mp4"), b"mic").unwrap();
        assert!(find_retranscription_sources(dir.path(), &fallback).is_err());
    }

    #[test]
    fn test_find_audio_file_non_mp4_extensions() {
        let dir = tempfile::tempdir().unwrap();

        // Create audio.wav (imported as .wav, not .mp4)
        std::fs::write(dir.path().join("audio.wav"), b"fake").unwrap();
        let found = find_audio_file(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "audio.wav");
    }

    #[test]
    fn test_find_audio_file_fallback_scan() {
        let dir = tempfile::tempdir().unwrap();

        // Create a file with an audio extension but non-standard name
        std::fs::write(dir.path().join("my_recording.flac"), b"fake").unwrap();
        // Also add a non-audio file that should be ignored
        std::fs::write(dir.path().join("notes.txt"), b"text").unwrap();

        let found = find_audio_file(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "my_recording.flac");
    }

    #[test]
    fn test_find_audio_file_priority_order() {
        let dir = tempfile::tempdir().unwrap();

        // Create both audio.m4a and audio.mp4 — mp4 should win (listed first in candidates)
        std::fs::write(dir.path().join("audio.m4a"), b"fake").unwrap();
        std::fs::write(dir.path().join("audio.mp4"), b"fake").unwrap();
        let found = find_audio_file(dir.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "audio.mp4");
    }

    #[test]
    fn test_find_audio_file_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_audio_file(dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No audio file found"));
    }

    #[test]
    fn test_find_audio_file_nonexistent_folder() {
        let result = find_audio_file(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
    }

    #[test]
    fn test_audio_extensions_constant() {
        // Verify all expected formats are covered
        assert!(AUDIO_EXTENSIONS.contains(&"mp4"));
        assert!(AUDIO_EXTENSIONS.contains(&"m4a"));
        assert!(AUDIO_EXTENSIONS.contains(&"wav"));
        assert!(AUDIO_EXTENSIONS.contains(&"mp3"));
        assert!(AUDIO_EXTENSIONS.contains(&"flac"));
        assert!(AUDIO_EXTENSIONS.contains(&"ogg"));
        assert!(AUDIO_EXTENSIONS.contains(&"aac"));
        // FFmpeg-backed formats
        assert!(AUDIO_EXTENSIONS.contains(&"mkv"));
        assert!(AUDIO_EXTENSIONS.contains(&"webm"));
        assert!(AUDIO_EXTENSIONS.contains(&"wma"));
        // Non-audio formats
        assert!(!AUDIO_EXTENSIONS.contains(&"txt"));
        assert!(!AUDIO_EXTENSIONS.contains(&"pdf"));
    }
}

#[cfg(test)]
mod issue9_room_tests {
    use super::*;

    #[test]
    fn enhance_room_recording_does_not_turn_microphone_into_you() {
        let dir = tempfile::tempdir().unwrap();
        let mixed = dir.path().join("audio.mp4");
        for (name, bytes) in [("mic.mp4", b"room".as_slice()), ("system.mp4", b"silent"), ("audio.mp4", b"room mix")] {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        std::fs::write(dir.path().join("metadata.json"), br#"{"in_person":true}"#).unwrap();
        let sources = find_retranscription_sources(dir.path(), &mixed).unwrap();
        assert_eq!(sources.len(), 1, "a silent padded system track is not a second room source");
        assert_eq!(sources[0].path, dir.path().join("mic.mp4"));
        assert_eq!(sources[0].speaker_hint, Some("Microphone"));
    }


}


#[cfg(test)]
mod issue9_label_retry_tests {
    use super::*;
    #[test]
    fn legacy_call_sources_keep_their_hints_but_room_fallback_never_uses_you() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("audio.mp4");
        for name in ["mic.mp4", "system.mp4", "audio.mp4"] { std::fs::write(dir.path().join(name), b"fixture").unwrap(); }
        assert_eq!(find_retranscription_sources(dir.path(), &fallback).unwrap()[0].speaker_hint, Some("You"));
        std::fs::write(dir.path().join("metadata.json"), br#"{"in_person":true}"#).unwrap();
        std::fs::remove_file(dir.path().join("mic.mp4")).unwrap();
        let sources = find_retranscription_sources(dir.path(), &fallback).unwrap();
        assert_eq!(sources[0].path, fallback);
        assert_eq!(sources[0].speaker_hint, Some("Microphone"));
        std::fs::write(dir.path().join("metadata.json"), b"truncated").unwrap();
        assert!(find_retranscription_sources(dir.path(), &fallback).is_err());
    }
}

#[cfg(test)]
mod issue14_source_tests {
    use super::*;
    #[test]
    fn missing_call_track_never_falls_back_to_mixed_audio() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("mic.mp4"), b"fixture").unwrap();
        std::fs::write(folder.path().join("audio.mp4"), b"fixture").unwrap();
        assert!(saved_sources(folder.path()).unwrap_err().to_string().contains("source track is missing"));
    }
    #[test]
    fn deleted_audio_is_explicitly_unavailable() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("transcripts.json"), b"[]").unwrap();
        assert!(saved_sources(folder.path()).is_err());
    }
    #[test]
    fn r1_manifest_requires_both_retained_call_tracks_even_when_both_are_missing() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("audio.mp4"), []).unwrap();
        std::fs::write(folder.path().join("metadata.json"), r#"{"mic_audio_file":"mic.mp4","system_audio_file":"system.mp4","in_person":false}"#).unwrap();
        assert!(saved_sources(folder.path()).is_err());
        std::fs::write(folder.path().join("mic.mp4"), []).unwrap();
        assert!(saved_sources(folder.path()).is_err());
        std::fs::write(folder.path().join("system.mp4"), []).unwrap();
        assert!(saved_sources(folder.path()).is_ok());
    }
    #[test]
    fn r1_legacy_mixed_only_audio_still_refines_anonymously() {
        let folder = tempfile::tempdir().unwrap();
        let mixed = folder.path().join("audio.mp4");
        std::fs::write(&mixed, []).unwrap();
        let sources = find_retranscription_sources(folder.path(),&mixed).unwrap();
        assert_eq!(sources[0].source_id,"mixed");
        assert_eq!(sources[0].speaker_hint,None);
    }

}
