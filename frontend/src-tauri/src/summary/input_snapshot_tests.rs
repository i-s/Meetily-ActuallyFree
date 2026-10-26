use super::*;
use crate::database::repositories::summary::SummaryProcessesRepository;

async fn database() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m', 'Meeting', datetime('now'), datetime('now'))")
        .execute(&pool).await.unwrap();
    pool
}

async fn notes(pool: &SqlitePool, text: &str) {
    sqlx::query("INSERT INTO meeting_notes (meeting_id, notes_markdown, created_at, updated_at) VALUES ('m', ?, datetime('now'), datetime('now')) ON CONFLICT(meeting_id) DO UPDATE SET notes_markdown = excluded.notes_markdown")
        .bind(text).execute(pool).await.unwrap();
}

#[tokio::test]
async fn snapshot_keeps_full_transcript_and_notes_while_edits_and_chat_do_not_leak() {
    let pool = database().await;
    let complete_notes = format!("Decision differs from transcript. {} END OF NOTES", "追加 ".repeat(20_000));
    notes(&pool, &complete_notes).await;
    for i in 0..130 {
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, speaker) VALUES (?, 'm', ?, '00:00', ?, 'You')")
            .bind(format!("t{i}")).bind(format!("turn {i}")).bind(i as f64)
            .execute(&pool).await.unwrap();
    }
    // An unrelated persisted assistant answer must never be used as source material.
    sqlx::query("INSERT INTO meeting_chat_turns (id, meeting_id, question, answer, status, created_at, context_json) VALUES ('chat', 'm', 'Question', 'CHAT SECRET', 'completed', datetime('now'), '{}')").execute(&pool).await.unwrap();
    let snapshot = capture_and_start(&pool, "m").await.unwrap();
    assert!(snapshot.transcript.contains("turn 0\n"));
    assert!(snapshot.transcript.ends_with("turn 129"));
    assert_eq!(snapshot.notes_markdown.as_deref(), Some(complete_notes.as_str()));
    let prompt = snapshot.context_prompt("Focus on decisions");
    assert!(prompt.contains(&complete_notes));
    assert!(prompt.contains("<meeting_notes>"));
    assert!(!prompt.contains("CHAT SECRET"));
    notes(&pool, "Edited while generating").await;
    sqlx::query("UPDATE transcripts SET transcript = 'Edited transcript'").execute(&pool).await.unwrap();
    assert_eq!(snapshot.notes_markdown.as_deref(), Some(complete_notes.as_str()));
    assert!(!snapshot.transcript.contains("Edited transcript"));
    assert!(saved_snapshot(&pool, "m").await.unwrap().is_none());
    SummaryProcessesRepository::update_process_completed(&pool, "m", serde_json::json!({"markdown":"First result"}), 1, 1.0).await.unwrap();
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown, snapshot.notes_markdown);
    capture_and_start(&pool, "m").await.unwrap();
    SummaryProcessesRepository::update_process_failed(&pool, "m", "provider error").await.unwrap();
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown, snapshot.notes_markdown);
    assert!(SummaryProcessesRepository::get_summary_data(&pool, "m").await.unwrap().unwrap().result.unwrap().contains("First result"));
    sqlx::query("DELETE FROM meetings WHERE id = 'm'").execute(&pool).await.unwrap();
    assert!(saved_snapshot(&pool, "m").await.unwrap().is_none());
    assert!(capture_and_start(&pool, "m").await.is_err());
}

#[tokio::test]
async fn notes_only_input_is_valid_and_empty_input_does_not_replace_prior_state() {
    let pool = database().await;
    notes(&pool, "Notes without speech").await;
    let snapshot = capture_and_start(&pool, "m").await.unwrap();
    assert!(snapshot.transcript.is_empty());
    SummaryProcessesRepository::update_process_completed(&pool, "m", serde_json::json!({"markdown":"From notes"}), 1, 1.0).await.unwrap();
    notes(&pool, "").await;
    assert!(capture_and_start(&pool, "m").await.is_err());
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown.as_deref(), Some("Notes without speech"));
}

#[tokio::test]
async fn reopened_database_keeps_snapshot_and_cancelled_run_cannot_replace_it() {
    let directory = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("snapshots.sqlite")).create_if_missing(true).foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options.clone()).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m', 'Meeting', datetime('now'), datetime('now'))").execute(&pool).await.unwrap();
    notes(&pool, "Original notes").await;
    capture_and_start(&pool, "m").await.unwrap();
    SummaryProcessesRepository::update_process_completed(&pool, "m", serde_json::json!({"markdown":"Original"}), 1, 1.0).await.unwrap();
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown.as_deref(), Some("Original notes"));
    notes(&pool, "Changed notes").await;
    capture_and_start(&pool, "m").await.unwrap();
    SummaryProcessesRepository::update_process_cancelled(&pool, "m").await.unwrap();
    assert!(!SummaryProcessesRepository::update_process_completed(&pool, "m", serde_json::json!({"markdown":"Late completion"}), 1, 1.0).await.unwrap());
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown.as_deref(), Some("Original notes"));
    capture_and_start(&pool, "m").await.unwrap();
    SummaryProcessesRepository::update_process_completed(&pool, "m", serde_json::json!({"markdown":"New result"}), 1, 1.0).await.unwrap();
    assert_eq!(saved_snapshot(&pool, "m").await.unwrap().unwrap().notes_markdown.as_deref(), Some("Changed notes"));
}
