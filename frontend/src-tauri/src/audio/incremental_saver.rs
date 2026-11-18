use std::path::PathBuf;
use anyhow::{Result, anyhow};
use log::{info, warn};
use super::encode::encode_single_audio;
use super::recording_state::AudioChunk;
use serde::{Serialize, Deserialize};


#[cfg(test)]
fn ffconcat_file_line(path: &std::path::Path) -> String {
    super::track_commit::concat_line(path)
}

/// Audio data for one track
#[derive(Clone)]
struct AudioData {
    data: Vec<f32>,
}

/// Incremental audio saver that writes checkpoints every 30 seconds
/// to minimize memory usage and enable crash recovery.
///
/// `track` selects the output basename (`audio`, `mic`, `system`) so a meeting
/// can keep separate source tracks for offline diarization plus a mixed file
/// for playback.
pub struct IncrementalAudioSaver {
    checkpoint_buffer: Vec<AudioData>,
    checkpoint_interval_samples: usize,  // 30s at 48kHz = 1,440,000 samples
    checkpoint_count: u32,
    checkpoints_dir: PathBuf,
    meeting_folder: PathBuf,
    sample_rate: u32,
    /// Basename without extension: "audio" | "mic" | "system"
    track: String,
}

impl IncrementalAudioSaver {
    /// Create a new incremental saver for the mixed playback track (`audio.mp4`).
    pub fn new(meeting_folder: PathBuf, sample_rate: u32) -> Result<Self> {
        Self::new_track(meeting_folder, sample_rate, "audio")
    }

    /// Create a saver for a named track (`audio`, `mic`, or `system`).
    pub fn new_track(meeting_folder: PathBuf, sample_rate: u32, track: &str) -> Result<Self> {
        let track = track.trim().to_ascii_lowercase();
        if track.is_empty() {
            return Err(anyhow!("track name must not be empty"));
        }
        let checkpoints_dir = meeting_folder.join(".checkpoints").join(&track);

        std::fs::create_dir_all(&checkpoints_dir).map_err(|e| {
            anyhow!(
                "Failed to create checkpoints dir {}: {}",
                checkpoints_dir.display(),
                e
            )
        })?;

        Ok(Self {
            checkpoint_buffer: Vec::new(),
            checkpoint_interval_samples: sample_rate as usize * 30, // 30 seconds
            checkpoint_count: 0,
            checkpoints_dir,
            meeting_folder,
            sample_rate,
            track,
        })
    }

    /// Add an audio chunk to the buffer
    /// Automatically saves a checkpoint when buffer reaches 30 seconds
    pub fn add_chunk(&mut self, chunk: AudioChunk) -> Result<()> {
        let audio_data = AudioData {
            data: chunk.data,
            // sample_rate: chunk.sample_rate,
        };

        self.checkpoint_buffer.push(audio_data);

        // Calculate total samples in buffer
        let total_samples: usize = self.checkpoint_buffer
            .iter()
            .map(|c| c.data.len())
            .sum();

        // Save checkpoint when buffer reaches threshold (30 seconds)
        if total_samples >= self.checkpoint_interval_samples {
            self.save_checkpoint()?;
            self.checkpoint_buffer.clear();
        }

        Ok(())
    }

    /// Save current buffer as a checkpoint file
    fn save_checkpoint(&mut self) -> Result<()> {
        // Concatenate all chunks in buffer
        let audio_data: Vec<f32> = self.checkpoint_buffer
            .iter()
            .flat_map(|c| &c.data)
            .cloned()
            .collect();

        if audio_data.is_empty() {
            warn!("Attempted to save empty checkpoint, skipping");
            return Ok(());
        }

        // Generate checkpoint filename
        let checkpoint_path = self.checkpoints_dir
            .join(format!("{}_chunk_{:03}.mp4", self.track, self.checkpoint_count));

        // Encode and save checkpoint
        // A failed encode must not leave a corrupt .mp4 that recovery mistakes
        // for a committed checkpoint, or block retry with FFmpeg's overwrite prompt.
        let pending_path = checkpoint_path.with_extension("pending");
        let _ = std::fs::remove_file(&pending_path);
        encode_single_audio(
            bytemuck::cast_slice(&audio_data),
            self.sample_rate,
            1,  // mono
            &pending_path
        )?;
        std::fs::File::open(&pending_path)?.sync_all()?;
        std::fs::rename(&pending_path, &checkpoint_path)?;

        let duration_seconds = audio_data.len() as f32 / self.sample_rate as f32;
        self.checkpoint_count += 1;

        info!(
            "Saved {} checkpoint {}: {:.2}s ({} samples)",
            self.track,
            self.checkpoint_count,
            duration_seconds,
            audio_data.len()
        );

        Ok(())
    }

    pub fn has_audio(&self) -> bool {
        self.checkpoint_count > 0 || self.checkpoint_buffer.iter().any(|chunk| !chunk.data.is_empty())
    }

    /// Finalize the recording: save final checkpoint, merge all checkpoints, cleanup
    ///
    /// Returns the path to the final merged `{track}.mp4` file
    pub async fn finalize(&mut self) -> Result<PathBuf> {
        info!("Finalizing incremental recording track '{}'...", self.track);

        // Save final buffer if not empty
        if !self.checkpoint_buffer.is_empty() {
            info!(
                "Saving final {} checkpoint with remaining {} chunks",
                self.track,
                self.checkpoint_buffer.len()
            );
            self.save_checkpoint()?;
            self.checkpoint_buffer.clear();
        }

        if self.checkpoint_count == 0 {
            return Err(anyhow!(
                "No audio checkpoints to merge for track '{}' - recording may have failed",
                self.track
            ));
        }

        // Merge all checkpoints using FFmpeg concat
        let final_audio_path = self.meeting_folder.join(format!("{}.mp4", self.track));
        self.merge_checkpoints(&final_audio_path).await?;

        // RecordingSaver owns meeting-level commit and cleanup. A different
        // source or metadata write can still fail after this track succeeds.

        info!("Finalized track: {}", final_audio_path.display());

        Ok(final_audio_path)
    }

    /// Merge all checkpoint files into final audio.mp4 using FFmpeg concat
    /// Uses concat demuxer for fast merging without re-encoding
    async fn merge_checkpoints(&self, output: &PathBuf) -> Result<()> {
        let directory = self.checkpoints_dir.clone();
        let output = output.clone();
        let count = self.checkpoint_count;
        tokio::task::spawn_blocking(move || recover_checkpoint_track(&directory, &output, Some(count)))
            .await.map_err(|e| anyhow!("Track finalization task failed: {e}"))?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }

    /// Get the meeting folder path
    pub fn get_meeting_folder(&self) -> &PathBuf {
        &self.meeting_folder
    }

    /// Get current checkpoint count
    pub fn get_checkpoint_count(&self) -> u32 {
        self.checkpoint_count
    }
}

/// Audio recovery status for transcript recovery feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRecoveryStatus {
    pub status: String, // "success" | "partial" | "failed" | "none"
    pub chunk_count: u32,
    pub estimated_duration_seconds: f64,
    pub audio_file_path: Option<String>,
    pub message: String,
}

/// Recover audio from checkpoint files
/// This is called by the transcript recovery system to merge audio chunks after a crash
fn recover_checkpoint_track(
    checkpoints_dir: &PathBuf,
    output_path: &PathBuf,
    expected_count: Option<u32>,
) -> Result<u32, String> {
    let count = super::track_commit::commit_checkpoint_track(checkpoints_dir, output_path, expected_count)?;
    // Informational only. Cleanup always validates the durable hash proof again.
    std::fs::write(checkpoints_dir.join(".recovered"), b"track-commit-v1")
        .map_err(|e| format!("Could not mark committed track: {e}"))?;
    Ok(count)
}

fn finalized_mixed_audio(folder: &std::path::Path) -> Result<Option<super::track_commit::ValidatedTrack>, String> {
    let output = folder.join("audio.mp4");
    if let Some(valid) = super::track_commit::validate_committed_track(None, &output)? { return Ok(Some(valid)); }
    if !output.is_file() { return Ok(None); }
    // Compatibility for completed recordings predating per-track proofs. They
    // may be saved to the library, but are never proof to delete checkpoints.
    let metadata = match std::fs::read(folder.join("metadata.json")) {
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    if metadata.get("status").and_then(|v| v.as_str()) != Some("completed")
        || metadata.get("audio_file").and_then(|v| v.as_str()) != Some("audio.mp4") { return Ok(None); }
    let duration_seconds = super::track_commit::validate_decodable_audio(&output)?;
    Ok(Some(super::track_commit::ValidatedTrack { checkpoint_count: 0, duration_seconds }))
}

pub(crate) fn validated_final_audio_available(folder: &std::path::Path) -> Result<bool, String> {
    Ok(finalized_mixed_audio(folder)?.is_some())
}

fn recover_recording_audio(folder: &std::path::Path) -> Result<AudioRecoveryStatus, String> {
    let root = folder.join(".checkpoints");
    let mut mixed = None;
    let mut failures = Vec::new();
    let mut source_counts = Vec::new();
    let mut any_inputs = false;
    // Attempt each track independently even when another output already exists
    // or conflicts. A failed source is not permission to replace good playback.
    for track in ["audio", "mic", "system"] {
        let output = folder.join(format!("{track}.mp4"));
        let directory = if track == "audio" && !root.join("audio").is_dir() { root.clone() } else { root.join(track) };
        let files = match super::track_commit::checkpoint_files(&directory) {
            Ok(files) => files,
            Err(error) => {
                failures.push(format!("{track}: {error}"));
                if track == "audio" { mixed = finalized_mixed_audio(folder).ok().flatten(); }
                continue;
            }
        };
        if directory.is_dir() {
            match std::fs::read_dir(&directory) {
                Ok(entries) => {
                    if entries.filter_map(Result::ok).any(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("pending")) {
                        failures.push(format!("{track}: uncommitted pending audio was retained for review"));
                    }
                },
                Err(error) => failures.push(format!("{track}: could not inspect pending audio: {error}")),
            }
        }
        let result = if !files.is_empty() {
            any_inputs = true;
            recover_checkpoint_track(&directory, &output, None)
                .and_then(|_| super::track_commit::validate_committed_track(Some(&directory), &output))
        } else if track == "audio" { finalized_mixed_audio(folder) }
        else { super::track_commit::validate_committed_track(None, &output) };
        match result {
            Ok(valid) if track == "audio" => mixed = valid,
            Ok(Some(valid)) => source_counts.push((track, valid.checkpoint_count)),
            Ok(None) => {},
            Err(error) => {
                failures.push(format!("{track}: {error}"));
                // A conflicting input must stay untouched, but a previously
                // committed, byte-validated playback track is still usable.
                if track == "audio" { mixed = finalized_mixed_audio(folder).ok().flatten(); }
            },
        }
    }
    if let Some(valid) = &mixed {
        for (track, count) in source_counts {
            if count < valid.checkpoint_count {
                failures.push(format!("{track}: recovered {count} checkpoints versus {} playback checkpoints; source tail needs review", valid.checkpoint_count));
            }
        }
    }
    let (status, chunk_count, duration, path) = match mixed {
        Some(valid) => (if failures.is_empty() { "success" } else { "partial" }, valid.checkpoint_count as u32,
            valid.duration_seconds, Some(folder.join("audio.mp4").to_string_lossy().into_owned())),
        None => (if failures.is_empty() && !any_inputs { "none" } else { "failed" }, 0, 0.0, None),
    };
    Ok(AudioRecoveryStatus { status: status.into(), chunk_count, estimated_duration_seconds: duration,
        audio_file_path: path,
        message: if failures.is_empty() { if status == "none" { "No validated audio or checkpoints found".into() } else { "Validated recorded audio and available source tracks".into() } }
            else { format!("Recovery requires review; existing audio and checkpoints were preserved ({})", failures.join("; ")) },
    })
}

#[tauri::command]
pub async fn recover_audio_from_checkpoints(meeting_folder: String, _sample_rate: u32) -> Result<AudioRecoveryStatus, String> {
    // Recovery can read finalized tracks too. Share offline admission with
    // cleanup until the blocking recovery worker has actually drained.
    let operation = crate::diarization::operation_guard().await;
    tokio::task::spawn_blocking(move || {
        let _operation = operation;
        recover_recording_audio(std::path::Path::new(&meeting_folder))
    }).await.map_err(|e| e.to_string())?
}

/// Startup detection includes validated finalized audio, not just checkpoints.
#[tauri::command]
pub async fn has_recoverable_audio(meeting_folder: String) -> Result<bool, String> {
    let operation = crate::diarization::operation_guard().await;
    tokio::task::spawn_blocking(move || {
        let _operation = operation;
        let folder = PathBuf::from(meeting_folder);
        if finalized_mixed_audio(&folder)?.is_some() { return Ok(true); }
        let root = folder.join(".checkpoints");
        let directory = if root.join("audio").is_dir() { root.join("audio") } else { root };
        Ok(!super::track_commit::checkpoint_files(&directory)?.is_empty())
    }).await.map_err(|e| e.to_string())?
}

/// Clean up checkpoint files after successful recording or recovery
/// This command is called by the frontend after successful save to clean up checkpoint files
#[tauri::command]
pub async fn cleanup_checkpoints(meeting_folder: String) -> Result<(), String> {
    let operation = crate::diarization::operation_guard().await;
    tokio::task::spawn_blocking(move || {
        let _operation = operation;
        let folder = PathBuf::from(meeting_folder);
        let root = folder.join(".checkpoints");
        for track in ["audio", "mic", "system"] {
            let directory = if track == "audio" && !root.join("audio").is_dir() { root.clone() } else { root.join(track) };
            if !directory.is_dir() { continue; }
            match super::track_commit::validate_committed_track(Some(&directory), &folder.join(format!("{track}.mp4"))) {
                Ok(Some(_)) => super::track_commit::cleanup_checkpoint_track(&directory, &folder.join(format!("{track}.mp4")))?,
                Ok(None) => {}, // Unknown/legacy audio is never authority to delete input.
                Err(error) => warn!("Keeping checkpoint track {}: {}", track, error),
            }
        }
        let _ = std::fs::remove_dir(&root);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// Check if a meeting folder has audio checkpoint files
/// Returns true if .checkpoints/ directory exists and contains .mp4 files
#[tauri::command]
pub async fn has_audio_checkpoints(meeting_folder: String) -> Result<bool, String> {
    let folder_path = PathBuf::from(&meeting_folder);
    let checkpoints_root = folder_path.join(".checkpoints");
    let checkpoints_dir = if checkpoints_root.join("audio").is_dir() {
        checkpoints_root.join("audio")
    } else {
        checkpoints_root
    };

    // Check if checkpoints directory exists
    if !checkpoints_dir.exists() {
        return Ok(false);
    }

    // Scan for .mp4 checkpoint files
    let has_mp4_files = std::fs::read_dir(&checkpoints_dir)
        .map_err(|e| format!("Failed to read checkpoints directory: {}", e))?
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry.path().extension().and_then(|s| s.to_str()) == Some("mp4")
        });

    Ok(has_mp4_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use super::super::recording_state::DeviceType;

    #[test]
    fn concat_paths_escape_apostrophes() {
        let line = ffconcat_file_line(std::path::Path::new("/Users/O'Brien/audio.mp4"));
        assert_eq!(line, "file '/Users/O'\\''Brien/audio.mp4'\n");
    }

    #[tokio::test]
    async fn test_checkpoint_creation() {
        // Create temp meeting folder
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Test_Meeting");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(
            meeting_folder.clone(),
            48000
        ).unwrap();

        // Add 60 seconds worth of audio (should create 2 checkpoints)
        for i in 0..120 {  // 120 chunks of 0.5s each
            let chunk = AudioChunk {
                data: vec![0.5f32; 24000],  // 0.5s at 48kHz
                sample_rate: 48000,
                timestamp: i as f64 * 0.5,  // timestamp in seconds
                chunk_id: i as u64,
                device_type: DeviceType::Microphone,
            };
            saver.add_chunk(chunk).unwrap();
        }

        // Verify 2 checkpoints created
        assert_eq!(saver.checkpoint_count, 2);

        // Finalize and verify merge
        let final_path = saver.finalize().await.unwrap();
        assert!(final_path.exists());

        // A track finalization alone must not erase crash-recovery evidence.
        assert!(meeting_folder.join(".checkpoints/audio").exists());
    }

    #[tokio::test]
    async fn test_empty_recording() {
        let temp_dir = tempdir().unwrap();
        let meeting_folder = temp_dir.path().join("Empty_Test");
        std::fs::create_dir_all(&meeting_folder).unwrap();
        std::fs::create_dir_all(meeting_folder.join(".checkpoints")).unwrap();

        let mut saver = IncrementalAudioSaver::new(
            meeting_folder.clone(),
            48000
        ).unwrap();

        // Try to finalize without adding any chunks
        let result = saver.finalize().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No audio checkpoints"));
    }
}


#[cfg(test)]
mod recovery_tests {
    use super::*;
    use super::super::recording_state::DeviceType;

    fn chunk() -> AudioChunk {
        AudioChunk { data: vec![0.2; 48_000], sample_rate: 48_000,
            timestamp: 1.0, chunk_id: 0, device_type: DeviceType::Microphone }
    }

    #[test]
    fn recovers_committed_checkpoint_and_ignores_incomplete_output() {
        let dir = tempfile::tempdir().unwrap();
        let mut saver = IncrementalAudioSaver::new_track(dir.path().to_path_buf(), 48_000, "mic").unwrap();
        saver.add_chunk(chunk()).unwrap();
        saver.save_checkpoint().unwrap();
        std::fs::write(saver.checkpoints_dir.join("mic_chunk_001.pending"), b"incomplete").unwrap();
        let output = dir.path().join("mic.mp4");
        assert_eq!(recover_checkpoint_track(&saver.checkpoints_dir, &output, None).unwrap(), 1);
        assert!(output.metadata().unwrap().len() > 0);
        assert!(saver.checkpoints_dir.join("mic_chunk_000.mp4").exists(), "recovery retains evidence until explicit cleanup");
    }

    #[test]
    fn recovery_never_replaces_existing_user_audio() {
        let dir = tempfile::tempdir().unwrap();
        let mut saver = IncrementalAudioSaver::new(dir.path().to_path_buf(), 48_000).unwrap();
        saver.add_chunk(chunk()).unwrap();
        saver.save_checkpoint().unwrap();
        let output = dir.path().join("audio.mp4");
        std::fs::write(&output, b"existing user audio").unwrap();
        assert!(recover_checkpoint_track(&saver.checkpoints_dir, &output, None).unwrap_err().contains("not replaced"));
        assert_eq!(std::fs::read(output).unwrap(), b"existing user audio");
    }

    #[test]
    fn checkpoint_failure_keeps_buffer_and_does_not_commit_partial_mp4() {
        let dir = tempfile::tempdir().unwrap();
        let mut saver = IncrementalAudioSaver::new(dir.path().to_path_buf(), 48_000).unwrap();
        saver.checkpoint_interval_samples = 1;
        // Deterministic storage failure, even under an elevated test account.
        std::fs::remove_dir(&saver.checkpoints_dir).unwrap();
        std::fs::write(&saver.checkpoints_dir, b"not a directory").unwrap();
        assert!(saver.add_chunk(chunk()).is_err());
        assert_eq!(saver.checkpoint_count, 0);
        assert_eq!(saver.checkpoint_buffer[0].data.len(), 48_000);
    }
}

#[cfg(test)]
mod issue9_retry_tests {
    use super::*;
    use super::super::recording_state::DeviceType;

    fn checkpoint(folder: &std::path::Path, track: &str) -> IncrementalAudioSaver {
        let mut saver = IncrementalAudioSaver::new_track(folder.to_path_buf(), 48_000, track).unwrap();
        saver.add_chunk(AudioChunk { data: vec![0.2; 48_000], sample_rate: 48_000, timestamp: 1.0,
            chunk_id: 0, device_type: DeviceType::Mixed }).unwrap();
        saver.save_checkpoint().unwrap();
        saver.checkpoint_buffer.clear();
        saver
    }

    #[tokio::test]
    async fn retry_after_mixed_promotion_and_source_failure_resumes_each_track() {
        let dir = tempfile::tempdir().unwrap();
        let _mixed = checkpoint(dir.path(), "audio");
        let mic = checkpoint(dir.path(), "mic");
        let _system = checkpoint(dir.path(), "system");
        let input = mic.checkpoints_dir.join("mic_chunk_000.mp4");
        let bytes = std::fs::read(&input).unwrap();
        std::fs::write(&input, b"interrupted source checkpoint").unwrap();
        let first = recover_audio_from_checkpoints(dir.path().to_string_lossy().into_owned(), 48_000).await.unwrap();
        assert_eq!(first.status, "partial");
        let mixed = std::fs::read(dir.path().join("audio.mp4")).unwrap();
        std::fs::write(input, bytes).unwrap();
        let retry = recover_audio_from_checkpoints(dir.path().to_string_lossy().into_owned(), 48_000).await.unwrap();
        assert_eq!(retry.status, "success", "retry must not stop at already promoted mixed audio: {}", retry.message);
        assert_eq!(std::fs::read(dir.path().join("audio.mp4")).unwrap(), mixed);
        assert!(dir.path().join("mic.mp4").is_file());
        assert!(dir.path().join("system.mp4").is_file());
    }

    #[tokio::test]
    async fn normal_partial_finalize_then_recovery_and_repeated_db_retry_are_safe() {
        let dir = tempfile::tempdir().unwrap();
        let mut mixed = checkpoint(dir.path(), "audio");
        let _mic = checkpoint(dir.path(), "mic");
        mixed.finalize().await.unwrap(); // Normal Stop promoted playback before another track/DB failed.
        let bytes = std::fs::read(dir.path().join("audio.mp4")).unwrap();
        for _ in 0..2 {
            let result = recover_audio_from_checkpoints(dir.path().to_string_lossy().into_owned(), 48_000).await.unwrap();
            assert_eq!(result.status, "success", "{}", result.message);
        }
        assert_eq!(std::fs::read(dir.path().join("audio.mp4")).unwrap(), bytes);
        assert!(dir.path().join(".checkpoints/audio").is_dir());
    }

    #[tokio::test]
    async fn interruption_after_promotion_before_cleanup_marker_is_resumable() {
        let dir = tempfile::tempdir().unwrap();
        let mixed = checkpoint(dir.path(), "audio");
        recover_checkpoint_track(&mixed.checkpoints_dir, &dir.path().join("audio.mp4"), None).unwrap();
        std::fs::remove_file(mixed.checkpoints_dir.join(".recovered")).unwrap();
        let retry = recover_audio_from_checkpoints(dir.path().to_string_lossy().into_owned(), 48_000).await.unwrap();
        assert_eq!(retry.status, "success", "{}", retry.message);
    }
}


#[cfg(test)]
mod issue9_finalized_tests {
    use super::*;
    #[tokio::test]
    async fn finalized_audio_without_checkpoints_is_available_after_db_failure() {
        let dir = tempfile::tempdir().unwrap();
        let mut saver = IncrementalAudioSaver::new(dir.path().to_path_buf(), 48_000).unwrap();
        saver.add_chunk(super::super::recording_state::AudioChunk { data: vec![0.2; 48_000], sample_rate: 48_000, timestamp: 1.0,
            chunk_id: 0, device_type: super::super::recording_state::DeviceType::Mixed }).unwrap();
        saver.finalize().await.unwrap();
        let folder = dir.path().to_string_lossy().into_owned();
        cleanup_checkpoints(folder.clone()).await.unwrap();
        assert!(!dir.path().join(".checkpoints").exists());
        assert!(has_recoverable_audio(folder.clone()).await.unwrap());
        let result = recover_audio_from_checkpoints(folder, 48_000).await.unwrap();
        assert_eq!(result.status, "success");
        assert!(result.audio_file_path.is_some());
    }
    #[tokio::test]
    async fn legacy_completed_audio_requires_metadata_and_a_successful_decode() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("audio.mp4");
        let samples = vec![0.2f32; 48_000];
        encode_single_audio(bytemuck::cast_slice(&samples), 48_000, 1, &output).unwrap();
        let folder = dir.path().to_string_lossy().into_owned();
        assert!(!has_recoverable_audio(folder.clone()).await.unwrap()); // Unknown file alone is not proof.
        std::fs::write(dir.path().join("metadata.json"), br#"{"status":"completed","audio_file":"audio.mp4"}"#).unwrap();
        assert!(has_recoverable_audio(folder.clone()).await.unwrap());
        std::fs::write(output, b"not audio").unwrap();
        assert!(has_recoverable_audio(folder).await.is_err());
    }
}


#[cfg(test)]
mod issue9_independent_track_tests {
    use super::*;
    #[tokio::test]
    async fn valid_playback_survives_changed_checkpoint_without_authorizing_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let samples = vec![0.2f32; 48_000];
        let input_dir = dir.path().join(".checkpoints/audio");
        std::fs::create_dir_all(&input_dir).unwrap();
        let input = input_dir.join("audio_chunk_000.mp4");
        encode_single_audio(bytemuck::cast_slice(&samples), 48_000, 1, &input).unwrap();
        let folder = dir.path().to_string_lossy().into_owned();
        assert_eq!(recover_audio_from_checkpoints(folder.clone(), 48_000).await.unwrap().status, "success");
        std::fs::write(&input, b"modified checkpoint").unwrap();
        let result = recover_audio_from_checkpoints(folder.clone(), 48_000).await.unwrap();
        assert_eq!(result.status, "partial");
        assert!(result.audio_file_path.is_some());
        cleanup_checkpoints(folder).await.unwrap();
        assert_eq!(std::fs::read(input).unwrap(), b"modified checkpoint");
    }
    #[tokio::test]
    async fn fewer_source_checkpoints_are_recovered_but_reported_as_partial_not_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let samples = vec![0.2f32; 48_000];
        for track in ["audio", "mic"] {
            let inputs = dir.path().join(".checkpoints").join(track);
            std::fs::create_dir_all(&inputs).unwrap();
            let first = inputs.join(format!("{track}_chunk_000.mp4"));
            encode_single_audio(bytemuck::cast_slice(&samples), 48_000, 1, &first).unwrap();
            if track == "audio" { std::fs::copy(&first, inputs.join("audio_chunk_001.mp4")).unwrap(); }
        }
        let result = recover_audio_from_checkpoints(dir.path().to_string_lossy().into_owned(), 48_000).await.unwrap();
        assert_eq!(result.status, "partial");
        assert!(result.audio_file_path.is_some());
        assert!(dir.path().join("mic.mp4").exists());
        assert!(dir.path().join(".checkpoints/mic/mic_chunk_000.mp4").exists());
    }
}


#[cfg(test)]
mod issue9_pending_tests {
    use super::*;
    #[tokio::test]
    async fn pending_tail_stays_visible_as_partial_and_survives_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let inputs = dir.path().join(".checkpoints/audio");
        std::fs::create_dir_all(&inputs).unwrap();
        let samples = vec![0.2f32; 48_000];
        encode_single_audio(bytemuck::cast_slice(&samples), 48_000, 1, &inputs.join("audio_chunk_000.mp4")).unwrap();
        std::fs::write(inputs.join("audio_chunk_001.pending"), b"uncommitted tail").unwrap();
        let folder = dir.path().to_string_lossy().into_owned();
        let result = recover_audio_from_checkpoints(folder.clone(), 48_000).await.unwrap();
        assert_eq!(result.status, "partial");
        assert!(result.audio_file_path.is_some());
        cleanup_checkpoints(folder).await.unwrap();
        assert_eq!(std::fs::read(inputs.join("audio_chunk_001.pending")).unwrap(), b"uncommitted tail");
    }
}
