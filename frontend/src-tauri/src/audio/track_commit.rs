//! Durable per-track promotion proof. A filename is not evidence of a commit.
//! The record is synced BEFORE exclusive output promotion, so interrupted Stop
//! and recovery share the same resumable protocol without overwriting audio.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Fingerprint {
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrackCommit {
    version: u32,
    inputs: Vec<Fingerprint>,
    output: Fingerprint,
    staged_file: String,
    duration_seconds: f64,
}

pub(super) struct ValidatedTrack {
    pub checkpoint_count: usize,
    pub duration_seconds: f64,
}

#[cfg(test)]
static HASH_WORK: std::sync::Mutex<Vec<(PathBuf, u64)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
struct HashWork(PathBuf);

#[cfg(test)]
impl HashWork {
    fn start(folder: &Path) -> Self {
        HASH_WORK.lock().unwrap().push((folder.to_path_buf(), 0));
        Self(folder.to_path_buf())
    }
    fn bytes(&self) -> u64 {
        HASH_WORK
            .lock()
            .unwrap()
            .iter()
            .find(|(folder, _)| folder == &self.0)
            .unwrap()
            .1
    }
}

#[cfg(test)]
impl Drop for HashWork {
    fn drop(&mut self) {
        HASH_WORK
            .lock()
            .unwrap()
            .retain(|(folder, _)| folder != &self.0);
    }
}

fn fingerprint(path: &Path) -> Result<Fingerprint, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(format!(
            "Not a nonempty regular audio file: {}",
            path.display()
        ));
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    fingerprint_file(path, &mut file, metadata.len())
}

fn fingerprint_file(
    path: &Path,
    file: &mut std::fs::File,
    bytes: u64,
) -> Result<Fingerprint, String> {
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        #[cfg(test)]
        for (folder, bytes) in HASH_WORK.lock().unwrap().iter_mut() {
            if path.starts_with(folder) {
                *bytes += count as u64;
            }
        }
        digest.update(&buffer[..count]);
    }
    Ok(Fingerprint {
        name: path
            .file_name()
            .ok_or("Missing audio basename")?
            .to_string_lossy()
            .into_owned(),
        bytes,
        sha256: format!("{:x}", digest.finalize()),
    })
}

fn same_content(a: &Fingerprint, b: &Fingerprint) -> bool {
    a.bytes == b.bytes && a.sha256 == b.sha256
}

pub(super) fn checkpoint_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    match std::fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err(format!(
                "Unsafe checkpoint directory: {}",
                directory.display()
            ))
        }
        _ => {}
    }
    let mut files = std::fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|p| p.extension().and_then(|s| s.to_str()) == Some("mp4"));
    files.sort();
    Ok(files)
}

fn inputs(directory: &Path) -> Result<Vec<Fingerprint>, String> {
    checkpoint_files(directory)?
        .iter()
        .map(|p| fingerprint(p))
        .collect()
}

fn record_path(output: &Path) -> Result<PathBuf, String> {
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid track name")?;
    if !["audio", "mic", "system"].contains(&stem) {
        return Err("Unsupported recording track".into());
    }
    Ok(output
        .parent()
        .ok_or("Missing recording folder")?
        .join(".audio-commits")
        .join(format!("{stem}.json")))
}

fn read_record(output: &Path) -> Result<Option<TrackCommit>, String> {
    let path = record_path(output)?;
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(m) if !m.file_type().is_file() => return Err("Unsafe track commit record".into()),
        _ => {}
    }
    let record: TrackCommit =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid track commit record: {e}"))?;
    if record.version != 1
        || record.output.name != output.file_name().unwrap().to_string_lossy()
        || !record.duration_seconds.is_finite()
        || record.duration_seconds <= 0.0
        || !record.staged_file.starts_with(".meetily-track-")
        || Path::new(&record.staged_file)
            .file_name()
            .and_then(|s| s.to_str())
            != Some(record.staged_file.as_str())
    {
        return Err("Invalid track commit provenance".into());
    }
    Ok(Some(record))
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    std::fs::File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    let _ = path;
    Ok(())
}

fn nonce() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn write_record(output: &Path, record: &TrackCommit) -> Result<TrackCommit, String> {
    let path = record_path(output)?;
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    sync_directory(parent.parent().ok_or("Missing recording folder")?)?;
    if !std::fs::symlink_metadata(parent)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_dir()
    {
        return Err("Unsafe track commit directory".into());
    }
    let temporary = parent.join(format!(".commit-{}.pending", nonce()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(record).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    // Hard-link creation is atomic and cannot overwrite another writer's proof.
    let promoted = std::fs::hard_link(&temporary, &path);
    let _ = std::fs::remove_file(&temporary);
    match promoted {
        Ok(()) => {
            sync_directory(parent)?;
            Ok(record.clone())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_record(output)?.ok_or("Track commit disappeared")?;
            if existing.inputs != record.inputs || !same_content(&existing.output, &record.output) {
                return Err(
                    "Existing track commit does not match; all audio/checkpoints were preserved"
                        .into(),
                );
            }
            Ok(existing)
        }
        Err(e) => Err(format!("Could not commit track proof: {e}")),
    }
}

/// Decode before declaring an unproven file valid. Progress must contain actual
/// audio duration; a successful process with no decoded samples is not audio.
pub(super) fn validate_decodable_audio(path: &Path) -> Result<f64, String> {
    fingerprint(path)?;
    let ffmpeg =
        super::ffmpeg::find_ffmpeg_path().ok_or("FFmpeg not found for audio validation")?;
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-progress",
            "pipe:1",
            "-nostats",
            "-f",
            "null",
            "-",
        ]);
    hide_console(&mut command);
    let result = command.output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Audio validation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    let duration = String::from_utf8_lossy(&result.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("out_time_us=")?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .fold(0.0f64, f64::max)
        / 1_000_000.0;
    if duration <= 0.0 {
        return Err("Audio contains no decoded samples".into());
    }
    Ok(duration)
}

fn hide_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let _ = command;
}

pub(super) fn validate_committed_track(
    directory: Option<&Path>,
    output: &Path,
) -> Result<Option<ValidatedTrack>, String> {
    let Some(record) = read_record(output)? else {
        return Ok(None);
    };
    if !output.exists() {
        return Ok(None);
    }
    if !same_content(&fingerprint(output)?, &record.output) {
        return Err(format!(
            "Existing recording was not replaced: {} has changed since commit",
            output.display()
        ));
    }
    if let Some(directory) = directory {
        // Cleanup can itself be interrupted after deleting a subset. Missing
        // inputs are safe ONLY when the entire promoted output still matches;
        // unknown/modified inputs are always a conflict.
        if inputs(directory)?
            .iter()
            .any(|input| !record.inputs.contains(input))
        {
            return Err(
                "Checkpoint inputs changed since track commit; preserved for review".into(),
            );
        }
    }
    Ok(Some(ValidatedTrack {
        checkpoint_count: record.inputs.len(),
        duration_seconds: record.duration_seconds,
    }))
}

pub(super) fn concat_line(path: &Path) -> String {
    format!("file '{}'\n", path.to_string_lossy().replace('\'', "'\\''"))
}

pub(super) fn commit_checkpoint_track(
    directory: &Path,
    output: &Path,
    expected_count: Option<u32>,
) -> Result<u32, String> {
    let files = checkpoint_files(directory)?;
    if files.is_empty() {
        return Err(format!("No checkpoints found in {}", directory.display()));
    }
    if expected_count.is_some_and(|n| files.len() < n as usize) {
        return Err("Source has fewer checkpoints than mixed audio; retained for review".into());
    }
    let source_inputs = inputs(directory)?;
    if let Some(valid) = validate_committed_track(Some(directory), output)? {
        return Ok(valid.checkpoint_count as u32);
    }
    let folder = output.parent().ok_or("Missing meeting folder")?;
    let mut record = read_record(output)?;
    let staged = if let Some(proof) = &record {
        if proof.inputs != source_inputs {
            return Err(
                "Pending track inputs changed; audio and checkpoints were preserved".into(),
            );
        }
        let path = folder.join(&proof.staged_file);
        if path.exists() {
            if !same_content(&fingerprint(&path)?, &proof.output) {
                return Err("Pending track output was modified; preserved for review".into());
            }
            path
        } else {
            return Err("Committed track promotion was interrupted and its staged file is missing; checkpoints preserved".into());
        }
    } else {
        let stage = folder.join(format!(".meetily-track-{}.recovering", nonce()));
        let list = directory.join(format!(".concat-{}.txt", nonce()));
        let lines = files
            .iter()
            .map(|p| {
                p.canonicalize()
                    .map(|p| concat_line(&p))
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<String, _>>()?;
        let mut list_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&list)
            .map_err(|e| e.to_string())?;
        list_file
            .write_all(lines.as_bytes())
            .map_err(|e| e.to_string())?;
        let ffmpeg =
            super::ffmpeg::find_ffmpeg_path().ok_or("FFmpeg not found for track recovery")?;
        let mut command = Command::new(ffmpeg);
        command
            .args([
                "-v", "error", "-nostdin", "-f", "concat", "-safe", "0", "-i",
            ])
            .arg(&list)
            .args(["-c", "copy", "-f", "mp4", "-n"])
            .arg(&stage);
        hide_console(&mut command);
        let encoded = command.output().map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&list);
        let encoded = encoded?;
        if !encoded.status.success() {
            return Err(format!(
                "FFmpeg failed: {}",
                String::from_utf8_lossy(&encoded.stderr)
            ));
        }
        let duration_seconds = validate_decodable_audio(&stage)?;
        std::fs::File::open(&stage)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        sync_directory(folder)?;
        let mut output_fingerprint = fingerprint(&stage)?;
        output_fingerprint.name = output.file_name().unwrap().to_string_lossy().into_owned();
        if output.exists() && !same_content(&fingerprint(output)?, &output_fingerprint) {
            // This attempt owns the uncommitted candidate, not the existing
            // recording. Avoid accumulating another full recording per retry.
            if same_content(&fingerprint(&stage)?, &output_fingerprint) {
                let _ = std::fs::remove_file(&stage);
            }
            return Err(format!(
                "Existing recording was not replaced: {} does not match checkpoint output",
                output.display()
            ));
        }
        let proposed = TrackCommit {
            version: 1,
            inputs: source_inputs,
            output: output_fingerprint,
            staged_file: stage.file_name().unwrap().to_string_lossy().into_owned(),
            duration_seconds,
        };
        record = Some(write_record(output, &proposed)?);
        stage
    };
    let proof = record.ok_or("Missing track commit proof")?;
    // Recheck inputs immediately before promotion; concurrent mutation is a
    // conflict, not permission to overwrite the recording or remove evidence.
    if inputs(directory)? != proof.inputs {
        return Err("Checkpoints changed during recovery".into());
    }
    match std::fs::hard_link(&staged, output) {
        Ok(()) => sync_directory(folder)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("Failed to promote audio without overwrite: {e}")),
    }
    if !same_content(&fingerprint(output)?, &proof.output) {
        return Err(
            "Existing recording was not replaced; output conflicts with commit proof".into(),
        );
    }
    sync_directory(folder)?;
    // Only remove the exact stage whose bytes we proved; unknown files stay put.
    if staged.exists() && same_content(&fingerprint(&staged)?, &proof.output) {
        let _ = std::fs::remove_file(&staged);
    }
    Ok(proof.inputs.len() as u32)
}

// Metadata is a change guard, not content proof. Hash once through an open
// handle, bracket that read with identity/version checks, then retain the stamp
// for constant-size checks before unlink. Ordinary external writes/replacements
// invalidate the stamp; hostile writers racing the final check are not excluded
// by filesystem metadata (there is no portable compare-and-unlink operation).
#[derive(Debug, PartialEq)]
struct FileStamp {
    bytes: u64,
    modified: std::time::SystemTime,
    identity: (u64, u64),
    changed: (i64, i64),
}

impl FileStamp {
    fn from_file(file: &std::fs::File) -> Result<Self, String> {
        Self::from_metadata(&file.metadata().map_err(|e| e.to_string())?, file)
    }

    fn from_metadata(metadata: &std::fs::Metadata, _file: &std::fs::File) -> Result<Self, String> {
        if !metadata.file_type().is_file() || metadata.len() == 0 {
            return Err("Cleanup requires a nonempty regular file".into());
        }
        #[cfg(unix)]
        let (identity, changed) = {
            use std::os::unix::fs::MetadataExt;
            (
                (metadata.dev(), metadata.ino()),
                (metadata.ctime(), metadata.ctime_nsec()),
            )
        };
        #[cfg(windows)]
        let (identity, changed) = {
            use std::os::windows::io::AsRawHandle;
            // Stable Win32 handle identity; std's file-index metadata API is
            // not stable on the project's Rust baseline. Open handles exclude
            // concurrent writers on Windows until their validation finishes.
            #[repr(C)]
            #[derive(Default)]
            struct FileInformation {
                attributes: u32,
                created: [u32; 2],
                accessed: [u32; 2],
                written: [u32; 2],
                volume: u32,
                size_high: u32,
                size_low: u32,
                links: u32,
                index_high: u32,
                index_low: u32,
            }
            #[link(name = "kernel32")]
            extern "system" {
                fn GetFileInformationByHandle(
                    handle: *mut std::ffi::c_void,
                    info: *mut FileInformation,
                ) -> i32;
            }
            let mut info = FileInformation::default();
            // The live File owns this handle; repr(C) matches the Win32
            // BY_HANDLE_FILE_INFORMATION layout and info is writable here.
            if unsafe { GetFileInformationByHandle(_file.as_raw_handle(), &mut info) } == 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            (
                (
                    info.volume as u64,
                    ((info.index_high as u64) << 32) | info.index_low as u64,
                ),
                (info.written[1] as i64, info.written[0] as i64),
            )
        };
        #[cfg(not(any(unix, windows)))]
        return Err("Stable cleanup file identity is unavailable on this platform".into());
        #[cfg(any(unix, windows))]
        Ok(Self {
            bytes: metadata.len(),
            modified: metadata.modified().map_err(|e| e.to_string())?,
            identity,
            changed,
        })
    }

    fn at(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.file_type().is_file() {
            return Err("Cleanup path is not a regular file".into());
        }
        let file = open_cleanup_file(path)?;
        let stamp = Self::from_file(&file)?;
        #[cfg(unix)]
        if stamp != Self::from_metadata(&metadata, &file)? {
            return Err("Cleanup path changed while opening".into());
        }
        Ok(stamp)
    }

    fn check(&self, path: &Path) -> Result<(), String> {
        if *self != Self::at(path)? {
            return Err(
                "Audio identity or metadata changed; remaining checkpoints preserved".into(),
            );
        }
        Ok(())
    }
}

fn open_cleanup_file(path: &Path) -> Result<std::fs::File, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: no writes/replacement while held.
    }
    options.open(path).map_err(|e| e.to_string())
}

fn validate_cleanup_file(
    path: &Path,
    expected: &Fingerprint,
) -> Result<(std::fs::File, FileStamp), String> {
    let mut file = open_cleanup_file(path)?;
    let before = FileStamp::from_file(&file)?;
    before.check(path)?;
    if !same_content(&fingerprint_file(path, &mut file, before.bytes)?, expected) {
        return Err("Audio content changed; remaining checkpoints preserved".into());
    }
    if before != FileStamp::from_file(&file)? {
        return Err("Audio changed during validation; remaining checkpoints preserved".into());
    }
    before.check(path)?;
    Ok((file, before))
}

#[cfg(test)]
thread_local! {
    static BEFORE_CLEANUP_DELETE: std::cell::RefCell<Option<Box<dyn FnMut(usize)>>> = const { std::cell::RefCell::new(None) };
}

/// Remove only proven inputs. Uncommitted .pending files and unknown artifacts
/// are not covered by the output proof and must survive automatic cleanup.
pub(super) fn cleanup_checkpoint_track(directory: &Path, output: &Path) -> Result<(), String> {
    let Some(proof) = read_record(output)? else {
        return Ok(());
    };
    if !output.exists() {
        return Ok(());
    }
    // Full content work is O(output bytes + remaining checkpoint bytes), once
    // per call, not once per deletion. Validate ALL inputs before deleting any.
    // Keep output open to pin its identity throughout cleanup; checkpoint
    // handles are short-lived so a long take cannot exhaust file descriptors.
    let (_output_file, output_stamp) = validate_cleanup_file(output, &proof.output)?;
    let expected: std::collections::HashMap<_, _> =
        proof.inputs.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut validated = Vec::new();
    for path in checkpoint_files(directory)? {
        let input = path
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|name| expected.get(name))
            .ok_or("Unproven checkpoint; remaining inputs preserved")?;
        let (_, stamp) = validate_cleanup_file(&path, input)?;
        validated.push((path, stamp));
    }
    for (_index, (path, stamp)) in validated.into_iter().enumerate() {
        #[cfg(test)]
        BEFORE_CLEANUP_DELETE.with(|hook| {
            if let Some(callback) = hook.borrow_mut().as_mut() {
                callback(_index);
            }
        });
        output_stamp.check(output)?;
        stamp.check(&path)?;
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    let marker = directory.join(".recovered");
    if std::fs::read(&marker).ok().as_deref() == Some(b"track-commit-v1") {
        let _ = std::fs::remove_file(marker);
    }
    let _ = std::fs::remove_dir(directory);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let checkpoints = dir.path().join(".checkpoints/audio");
        std::fs::create_dir_all(&checkpoints).unwrap();
        let samples = vec![0.2f32; 48_000];
        super::super::encode::encode_single_audio(
            bytemuck::cast_slice(&samples),
            48_000,
            1,
            &checkpoints.join("audio_chunk_000.mp4"),
        )
        .unwrap();
        let output = dir.path().join("audio.mp4");
        (dir, checkpoints, output)
    }
    #[test]
    fn proof_before_promotion_resumes_after_interruption() {
        let (_dir, inputs, output) = fixture();
        commit_checkpoint_track(&inputs, &output, None).unwrap();
        let proof = read_record(&output).unwrap().unwrap();
        let staged = output.parent().unwrap().join(&proof.staged_file);
        std::fs::copy(&output, &staged).unwrap();
        std::fs::remove_file(&output).unwrap(); // Exact state: durable proof + stage, promotion not yet done.
        assert_eq!(commit_checkpoint_track(&inputs, &output, None).unwrap(), 1);
        assert!(validate_committed_track(Some(&inputs), &output)
            .unwrap()
            .is_some());
    }
    #[test]
    fn modified_output_or_input_never_authorizes_cleanup_or_overwrite() {
        let (_dir, inputs, output) = fixture();
        commit_checkpoint_track(&inputs, &output, None).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        std::fs::write(&output, b"user modified recording").unwrap();
        assert!(commit_checkpoint_track(&inputs, &output, None).is_err());
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        assert!(inputs.join("audio_chunk_000.mp4").exists());
        assert_eq!(std::fs::read(&output).unwrap(), b"user modified recording");
        std::fs::write(&output, bytes).unwrap();
        std::fs::write(inputs.join("audio_chunk_000.mp4"), b"changed checkpoint").unwrap();
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        assert!(inputs.join("audio_chunk_000.mp4").exists());
    }
    #[test]
    fn validated_output_allows_cleanup_retry_but_preserves_uncommitted_artifacts() {
        let (_dir, inputs, output) = fixture();
        commit_checkpoint_track(&inputs, &output, None).unwrap();
        std::fs::write(inputs.join("audio_chunk_001.pending"), b"uncommitted tail").unwrap();
        std::fs::remove_file(inputs.join("audio_chunk_000.mp4")).unwrap(); // Cleanup interrupted before directory removal.
        assert!(validate_committed_track(Some(&inputs), &output)
            .unwrap()
            .is_some());
        cleanup_checkpoint_track(&inputs, &output).unwrap();
        assert_eq!(
            std::fs::read(inputs.join("audio_chunk_001.pending")).unwrap(),
            b"uncommitted tail"
        );
    }
    #[test]
    fn legacy_partial_output_can_only_be_adopted_if_byte_identical_to_checkpoints() {
        let (_dir, inputs, output) = fixture();
        commit_checkpoint_track(&inputs, &output, None).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        std::fs::remove_file(record_path(&output).unwrap()).unwrap();
        assert_eq!(commit_checkpoint_track(&inputs, &output, None).unwrap(), 1);
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
    }
    fn many_checkpoints(count: usize) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let (dir, inputs, output) = fixture();
        for index in 1..count {
            std::fs::copy(
                inputs.join("audio_chunk_000.mp4"),
                inputs.join(format!("audio_chunk_{index:03}.mp4")),
            )
            .unwrap();
        }
        commit_checkpoint_track(&inputs, &output, None).unwrap();
        (dir, inputs, output)
    }

    #[test]
    fn cleanup_hash_work_is_linear_in_output_and_checkpoint_bytes() {
        let mut measurements = Vec::new();
        for count in [1, 4, 12, 24] {
            let (_dir, inputs, output) = many_checkpoints(count);
            let output_bytes = std::fs::metadata(&output).unwrap().len();
            let input_bytes: u64 = checkpoint_files(&inputs)
                .unwrap()
                .iter()
                .map(|p| std::fs::metadata(p).unwrap().len())
                .sum();
            let work = HashWork::start(_dir.path());
            cleanup_checkpoint_track(&inputs, &output).unwrap();
            let hashed = work.bytes();
            println!("cleanup checkpoints={count} output_bytes={output_bytes} checkpoint_bytes={input_bytes} hashed_bytes={hashed} limit={}", output_bytes + input_bytes);
            measurements.push((count, hashed, output_bytes + input_bytes));
        }
        for (count, hashed, limit) in measurements {
            assert!(
                hashed <= limit,
                "{count} checkpoints: {hashed} hashed bytes exceed linear limit {limit}"
            );
        }
    }

    struct CleanupHook;
    impl Drop for CleanupHook {
        fn drop(&mut self) {
            BEFORE_CLEANUP_DELETE.with(|hook| *hook.borrow_mut() = None);
        }
    }
    fn before_delete(callback: impl FnMut(usize) + 'static) -> CleanupHook {
        BEFORE_CLEANUP_DELETE.with(|hook| *hook.borrow_mut() = Some(Box::new(callback)));
        CleanupHook
    }

    #[test]
    fn cleanup_validates_every_input_before_deleting_any() {
        let (_dir, inputs, output) = many_checkpoints(4);
        std::fs::write(
            inputs.join("audio_chunk_003.mp4"),
            b"changed last checkpoint",
        )
        .unwrap();
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        assert_eq!(checkpoint_files(&inputs).unwrap().len(), 4);
    }

    #[test]
    fn cleanup_never_deletes_without_a_commit_proof() {
        let (_dir, inputs, output) = many_checkpoints(4);
        std::fs::remove_file(record_path(&output).unwrap()).unwrap();
        let work = HashWork::start(_dir.path());
        cleanup_checkpoint_track(&inputs, &output).unwrap();
        assert_eq!(work.bytes(), 0);
        assert_eq!(checkpoint_files(&inputs).unwrap().len(), 4);
    }

    // Windows validation holds a deny-write/delete output handle; these Unix
    // controls exercise writes/replacements that are allowed while it is open.
    #[cfg(unix)]
    #[test]
    fn changed_output_stops_cleanup_and_partial_retry_hashes_only_remaining_inputs() {
        let (_dir, inputs, output) = many_checkpoints(4);
        let original = std::fs::read(&output).unwrap();
        let mutate = output.clone();
        let hook = before_delete(move |index| {
            if index == 1 {
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&mutate)
                    .unwrap()
                    .write_all(b"changed")
                    .unwrap();
            }
        });
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        drop(hook);
        assert!(!inputs.join("audio_chunk_000.mp4").exists());
        assert_eq!(checkpoint_files(&inputs).unwrap().len(), 3);
        std::fs::write(&output, &original).unwrap();
        let remaining_bytes: u64 = checkpoint_files(&inputs)
            .unwrap()
            .iter()
            .map(|p| std::fs::metadata(p).unwrap().len())
            .sum();
        let work = HashWork::start(_dir.path());
        cleanup_checkpoint_track(&inputs, &output).unwrap();
        let hashed = work.bytes();
        assert_eq!(hashed, original.len() as u64 + remaining_bytes);
        println!(
            "cleanup partial_retry remaining=3 hashed_bytes={hashed} limit={}",
            original.len() as u64 + remaining_bytes
        );
        assert!(!inputs.exists());
    }

    #[cfg(unix)]
    #[test]
    fn identical_replacement_after_validation_is_not_authority_to_delete() {
        for replace_output in [true, false] {
            let (_dir, inputs, output) = many_checkpoints(4);
            let target = if replace_output {
                output.clone()
            } else {
                inputs.join("audio_chunk_000.mp4")
            };
            let replacement = target.with_extension("replacement");
            std::fs::copy(&target, &replacement).unwrap();
            let hook = before_delete(move |index| {
                if index == 0 {
                    std::fs::rename(&replacement, &target).unwrap();
                }
            });
            assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
            drop(hook);
            assert_eq!(checkpoint_files(&inputs).unwrap().len(), 4);
            // A fresh call can prove byte-identical replacement, never trust
            // stale validation from before its inode changed.
            cleanup_checkpoint_track(&inputs, &output).unwrap();
            assert!(!inputs.exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn same_length_input_mutation_with_restored_mtime_stops_remaining_deletions() {
        let (_dir, inputs, output) = many_checkpoints(4);
        let target = inputs.join("audio_chunk_001.mp4");
        let before = std::fs::metadata(&target).unwrap().modified().unwrap();
        let mut bytes = std::fs::read(&target).unwrap();
        bytes[0] ^= 1;
        let hook = before_delete(move |index| {
            if index == 1 {
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&target)
                    .unwrap();
                file.write_all(&bytes).unwrap();
                file.set_modified(before).unwrap();
            }
        });
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        drop(hook);
        assert_eq!(checkpoint_files(&inputs).unwrap().len(), 3);
        assert!(inputs.join("audio_chunk_001.mp4").exists());
        assert!(cleanup_checkpoint_track(&inputs, &output).is_err());
        assert_eq!(checkpoint_files(&inputs).unwrap().len(), 3);
    }

    #[tokio::test]
    async fn cleanup_command_hash_work_is_linear_across_blocking_worker() {
        let mut measurements = Vec::new();
        for count in [1, 4, 12, 24] {
            let (dir, inputs, output) = many_checkpoints(count);
            let output_bytes = std::fs::metadata(&output).unwrap().len();
            let input_bytes: u64 = checkpoint_files(&inputs)
                .unwrap()
                .iter()
                .map(|p| std::fs::metadata(p).unwrap().len())
                .sum();
            // Folder-scoped actual read accounting crosses spawn_blocking, but
            // excludes other tests/fixtures and the preceding commit work.
            let work = HashWork::start(dir.path());
            super::super::incremental_saver::cleanup_checkpoints(
                dir.path().to_string_lossy().into_owned(),
            )
            .await
            .unwrap();
            let hashed = work.bytes();
            let limit = 2 * (output_bytes + input_bytes);
            println!("cleanup command checkpoints={count} output_bytes={output_bytes} checkpoint_bytes={input_bytes} hashed_bytes={hashed} limit={limit}");
            measurements.push((count, hashed, limit));
        }
        // The command's existing preflight performs one additional full pass;
        // neither pass may multiply output reads by the checkpoint count.
        for (count, hashed, limit) in measurements {
            assert!(
                hashed <= limit,
                "command {count} checkpoints: {hashed} hashed bytes exceed {limit}"
            );
        }
    }
}
