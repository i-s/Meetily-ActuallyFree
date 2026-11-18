//! Portable caller tests: actual native command bodies + real SQLite/FFmpeg.
//! The harness supplies only desktop transport/state wrappers.
use crate::audio::incremental_saver::{
    cleanup_checkpoints, recover_audio_from_checkpoints, IncrementalAudioSaver,
};
use crate::audio::recording_state::{AudioChunk, DeviceType};
use crate::state::{AppState, DbManager};
use crate::{api, AppHandle, State};

async fn state() -> AppState {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT, created_at TEXT, updated_at TEXT, folder_path TEXT, title_is_manual INTEGER); CREATE TABLE person_speakers(person_id TEXT, meeting_id TEXT, speaker_label TEXT, UNIQUE(meeting_id,speaker_label)); CREATE TABLE transcripts (id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, timestamp TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL, speaker TEXT);")
        .execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../../src-tauri/migrations/20261118001400_transcript_versions.sql")).execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../../src-tauri/migrations/20261118001401_transcript_version_references.sql")).execute(&pool).await.unwrap();
    crate::notes_document::tests::create_notes_schema(&pool).await;
    sqlx::raw_sql(include_str!("../../src-tauri/migrations/20261118001500_post_call_queue.sql")).execute(&pool).await.unwrap();
    AppState {
        db_manager: DbManager(pool),
    }
}

async fn save(state: &AppState, folder: &str, key: &str) -> Result<serde_json::Value, String> {
    api::api_save_transcript(
        AppHandle::<()>(std::marker::PhantomData),
        State(state),
        "Audio-only take".into(),
        vec![],
        Some(folder.into()),
        Some("2026-11-18T12:00:00Z".into()),
        None,
        Some(key.into()),
    )
    .await
}

async fn finalized_audio(folder: &std::path::Path) {
    let mut saver = IncrementalAudioSaver::new(folder.to_path_buf(), 48_000).unwrap();
    saver
        .add_chunk(AudioChunk {
            data: vec![0.2; 48_000],
            sample_rate: 48_000,
            timestamp: 1.0,
            chunk_id: 0,
            device_type: DeviceType::Mixed,
        })
        .unwrap();
    saver.finalize().await.unwrap();
}

#[tokio::test]
async fn recovery_native_save_rejects_unvalidated_empty_meetings() {
    let state = state().await;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_str().unwrap();
    assert!(save(&state, folder, "draft-42").await.is_err());
    std::fs::write(dir.path().join("audio.mp4"), b"not audio").unwrap();
    std::fs::write(
        dir.path().join("metadata.json"),
        br#"{"status":"completed","audio_file":"audio.mp4"}"#,
    )
    .unwrap();
    assert!(save(&state, folder, "draft-42").await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM meetings")
            .fetch_one(state.db_manager.pool())
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn recovery_native_finalized_audio_without_checkpoints_saves_once() {
    let state = state().await;
    let dir = tempfile::tempdir().unwrap();
    finalized_audio(dir.path()).await;
    let folder = dir.path().to_str().unwrap();
    cleanup_checkpoints(folder.into()).await.unwrap();
    assert!(!dir.path().join(".checkpoints").exists());
    let first = save(&state, folder, "draft-42").await.unwrap();
    let second = save(&state, folder, "draft-42").await.unwrap();
    assert_eq!(first["meeting_id"], second["meeting_id"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM meetings")
            .fetch_one(state.db_manager.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn recovery_command_then_real_database_failure_and_retry_keeps_audio_and_references() {
    let state = state().await;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_str().unwrap();
    finalized_audio(dir.path()).await; // Partial normal finalization still has committed checkpoints.
    let audio_before = std::fs::read(dir.path().join("audio.mp4")).unwrap();
    assert_eq!(
        recover_audio_from_checkpoints(folder.into(), 48_000)
            .await
            .unwrap()
            .status,
        "success"
    );
    sqlx::query("CREATE TRIGGER synthetic_disk_failure BEFORE INSERT ON meetings BEGIN SELECT RAISE(FAIL, 'synthetic database full'); END;")
        .execute(state.db_manager.pool()).await.unwrap();
    assert!(save(&state, folder, "draft-42")
        .await
        .unwrap_err()
        .contains("synthetic database full"));
    assert!(dir
        .path()
        .join(".checkpoints/audio/audio_chunk_000.mp4")
        .is_file());
    sqlx::query("DROP TRIGGER synthetic_disk_failure")
        .execute(state.db_manager.pool())
        .await
        .unwrap();
    assert_eq!(
        recover_audio_from_checkpoints(folder.into(), 48_000)
            .await
            .unwrap()
            .status,
        "success"
    );
    let saved = save(&state, folder, "draft-42").await.unwrap();
    assert_eq!(
        save(&state, folder, "draft-42").await.unwrap()["meeting_id"],
        saved["meeting_id"]
    );
    assert_eq!(
        std::fs::read(dir.path().join("audio.mp4")).unwrap(),
        audio_before
    );
}

#[tokio::test]
async fn notes_commands_survive_native_stop_save_and_recovery_retry() {
    let state = state().await;
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_str().unwrap();
    finalized_audio(dir.path()).await;
    let target = format!("recording:{folder}");
    let json = Some(serde_json::json!([{ "type": "paragraph", "content": "My own idea" }]));
    assert_eq!(crate::notes_document::api_write_notes_document(State(&state), target.clone(), "My own idea".into(), json.clone(), 0).await.unwrap(), 1);
    let saved = save(&state, folder, "notes-command-recovery").await.unwrap();
    let meeting_id = saved["meeting_id"].as_str().unwrap().to_string();
    assert_eq!(crate::notes_document::api_notes_recording_target(State(&state), meeting_id.clone()).await.unwrap(), Some(target.clone()));
    let notes = crate::notes_document::api_read_notes_document(State(&state), format!("meeting:{meeting_id}")).await.unwrap().unwrap();
    assert_eq!(notes.markdown.as_deref(), Some("My own idea"));
    assert_eq!(notes.json, json);
    crate::notes_document::api_write_notes_document(State(&state), target, "After Stop".into(), None, 1).await.unwrap();
    save(&state, folder, "notes-command-recovery").await.unwrap();
    assert_eq!(crate::notes_document::api_read_notes_document(State(&state), format!("meeting:{meeting_id}")).await.unwrap().unwrap().markdown.as_deref(), Some("After Stop"));
}
