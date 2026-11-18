use crate::{command, state::{AppState, DbManager}, State};
use std::sync::atomic::Ordering;

async fn run_case(flagged: bool, engine: &str) -> (Vec<(String, String, f64, f64, String)>, usize, usize) {
    let dir = tempfile::tempdir().unwrap();
    let recording = dir.path().join("audio.wav");
    std::fs::write(&recording, b"synthetic model boundary, not an audio fixture").unwrap();
    if flagged {
        std::fs::write(dir.path().join("mic.mp4"), b"synthetic mic source").unwrap();
        std::fs::write(dir.path().join("system.mp4"), b"synthetic remote source").unwrap();
    }
    let enrollment = dir.path().join("legacy-voiceprint.json");
    let polluted = br#"{"source":"legacy mic pickup of remote voice","embedding":[1,0]}"#;
    std::fs::write(&enrollment, polluted).unwrap();
    std::fs::write(dir.path().join("metadata.json"), format!("{{\"microphone_identity_unverified\":{flagged}}}")).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    crate::test_migrate(&pool).await;
    sqlx::query("INSERT INTO meetings(id,folder_path,title,created_at,updated_at) VALUES('m',?,'Synthetic Zoom','2026','2026')").bind(dir.path().to_str().unwrap()).execute(&pool).await.unwrap();
    for (id, source, start, end) in [("remote", "Guest", 0., 2.), ("mic", "Microphone", 0., 2.), ("system", "System audio + Guest", 0., 2.), ("stale", "You", 0., 2.), ("manual", "Alice", 2., 4.), ("other_turn", "Guest", 2., 4.), ("manual_composite", "Alice + You", 2., 4.), ("stale_gap", "You", 5., 6.), ("guest_gap", "You + Guest", 5., 6.), ("speaker_gap", "You+Speaker 2", 5., 6.), ("manual_gap", "Alice+You", 5., 6.)] {
        sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,timestamp,audio_start_time,audio_end_time,duration,speaker) VALUES(?,'m',?,'00:00',?,?,?,?)").bind(id).bind(format!("Keep {id}: текст!"))
            .bind(start).bind(end).bind(end-start).bind(source).execute(&pool).await.unwrap();
    }
    let before: Vec<(String,String,f64,f64)> = sqlx::query_as("SELECT id,transcript,audio_start_time,audio_end_time FROM transcripts ORDER BY id").fetch_all(&pool).await.unwrap();
    command::voiceprint::READS.store(0, Ordering::SeqCst);
    command::voiceprint::WRITES.store(0, Ordering::SeqCst);
    let state = AppState { db_manager: DbManager(pool.clone()) };
    let result = command::diarize_meeting(State(&state), "m".into(), Some(recording.to_str().unwrap().into()), None, None, Some(engine.into())).await.unwrap();
    assert_eq!(result.labeled, 11);
    let after: Vec<(String,String,f64,f64)> = sqlx::query_as("SELECT id,transcript,audio_start_time,audio_end_time FROM transcripts ORDER BY id").fetch_all(&pool).await.unwrap();
    assert_eq!(before, after, "identity changes must preserve text/row/timestamps");
    assert_eq!(std::fs::read(&enrollment).unwrap(), polluted);
    let rows = sqlx::query_as("SELECT id,transcript,audio_start_time,audio_end_time,speaker FROM transcripts ORDER BY id").fetch_all(&pool).await.unwrap();
    (rows, command::voiceprint::READS.load(Ordering::SeqCst), command::voiceprint::WRITES.load(Ordering::SeqCst))
}

#[tokio::test]
async fn actual_call_path_keeps_remote_guest_and_polluted_enrollment_out_of_you() {
    for engine in ["pyannote", "nemotron"] {
        let (rows, reads, writes) = run_case(true, engine).await;
        assert_eq!(reads, 0, "flagged call read legacy user voiceprint");
        assert_eq!(writes, 0, "flagged call enrolled unqualified mic audio");
        for (id, _, _, _, speaker) in rows {
            if id == "manual" || id == "other_turn" || id == "manual_composite" || id == "manual_gap" { assert_eq!(speaker, "Alice"); }
            else if id == "guest_gap" { assert_eq!(speaker, "Guest"); }
            else if id == "speaker_gap" { assert_eq!(speaker, "Speaker 2"); }
            else if id == "stale_gap" { assert_eq!(speaker, "Unknown"); }
            else { assert_eq!(speaker, "Speaker 1", "{id}: source should refine without becoming You or a manual name"); }
        }
    }
}
#[tokio::test]
async fn unrelated_legacy_call_keeps_existing_behavior_and_manual_names() {
    let (rows, reads, _) = run_case(false, "pyannote").await;
    assert!(reads > 0);
    assert_eq!(rows.iter().find(|r| r.0 == "remote").unwrap().4, "You");
    assert_eq!(rows.iter().find(|r| r.0 == "manual").unwrap().4, "Alice");
}
