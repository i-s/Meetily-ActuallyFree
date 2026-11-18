//! Combined #12/#14 qualification: real migrations, repositories and SQLite.
//! Audio/model/device/Tauri transport are not exercised by this fixture.
use crate::{api::TranscriptSegment, database::transcript_versions as versions, notes_document, repository::TranscriptsRepository};
use sqlx::SqlitePool;

async fn migrate(pool: &SqlitePool) {
    migrate_with_legacy_notes(pool, false).await;
}
async fn migrate_with_legacy_notes(pool: &SqlitePool, legacy: bool) {
    let folder = std::path::Path::new(file!()).parent().unwrap().join("../../src-tauri/migrations");
    let mut files: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql")).collect();
    files.sort();
    for file in files {
        if legacy && file.file_name().unwrap() == "20261118001200_recording_notes.sql" {
            // Populate the old schema before its revision column exists.
            sqlx::raw_sql("INSERT INTO meetings(id,title,created_at,updated_at) VALUES('legacy-notes','Legacy','2025','2025'); INSERT INTO meeting_notes(meeting_id,notes_markdown,notes_json,created_at,updated_at) VALUES('legacy-notes','Старые Notes','[{\"type\":\"paragraph\",\"content\":\"exact old blocks\"}]','2025','2025');").execute(pool).await.unwrap();
        }
        sqlx::raw_sql(&std::fs::read_to_string(file).unwrap()).execute(pool).await.unwrap();
    }
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(pool).await.unwrap().is_empty());
}
fn turn(id: &str, text: &str, speaker: &str) -> TranscriptSegment {
    TranscriptSegment { id: id.into(), text: text.into(), speaker: Some(speaker.into()), timestamp: "2026-11-18T10:00:01Z".into(), audio_start_time: Some(1.0), audio_end_time: Some(2.0), duration: Some(1.0) }
}
fn blocks() -> serde_json::Value {
    serde_json::json!([{ "id":"heading", "type":"heading", "props":{"level":2}, "content":[{"type":"text","text":"Заметки RU/EN","styles":{"bold":true}}], "children":[] }, {"id":"task","type":"bulletListItem","content":[{"type":"text","text":"API follow-up","styles":{"italic":true}}],"children":[]}])
}

#[tokio::test]
async fn combined_notes_versions_save_recovery_selection_correction_and_disk_reopen() {
    let folder = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(folder.path().join("synthetic.sqlite")).create_if_missing(true).foreign_keys(true);
    let pool = SqlitePool::connect_with(options.clone()).await.unwrap();
    migrate(&pool).await;
    let alias = "recording:/synthetic/combined";
    let markdown = "## **Заметки RU/EN**\n\n- *API follow-up*";
    notes_document::write(&pool,alias,markdown,Some(blocks()),0).await.unwrap();
    notes_document::write(&pool,"recording:/synthetic/unrelated","Other draft",None,0).await.unwrap();
    let meeting = TranscriptsRepository::save_transcript(&pool,"Meeting",&[turn("ignored","Live RU/EN speech","You")],Some("/synthetic/combined".into()),None).await.unwrap();
    let target = format!("meeting:{meeting}");
    let live = versions::selected(&pool,&meeting).await.unwrap();
    let candidate = versions::begin(&pool,&meeting,&serde_json::json!({"recording_metadata":{"in_person":false,"future_policy":"opaque"},"label_origin":"generated_asr"})).await.unwrap();
    let candidate_rows = [turn("mic","Candidate RU/EN speech","You"),turn("remote","Remote API reply","Guest")];
    let sources = std::collections::HashMap::from([("mic".into(),"microphone".into()),("remote".into(),"system".into())]);
    versions::finish_with_sources(&pool,&candidate,&candidate_rows,&sources).await.unwrap();
    versions::set_status(&pool,&candidate,"ready",None).await.unwrap();
    let note_before = notes_document::read(&pool,&target).await.unwrap().unwrap();
    // A late recording alias is still the exact same native Notes document.
    notes_document::write(&pool,alias,markdown,Some(blocks()),note_before.revision).await.unwrap();
    let note_before = notes_document::read(&pool,&target).await.unwrap().unwrap();
    versions::select(&pool,&meeting,&candidate,live.revision).await.unwrap();
    let selected = versions::selected(&pool,&meeting).await.unwrap();
    assert_eq!(selected.version_id,candidate);
    assert_eq!(versions::version_sources(&pool,&meeting,&candidate).await.unwrap(),sources);
    let settings: serde_json::Value = serde_json::from_str(&selected.settings_json).unwrap();
    assert_eq!(settings["recording_metadata"]["future_policy"],"opaque");
    assert!(!selected.segments.iter().any(|row| row.text.contains("Заметки")));
    versions::correct_text(&pool,&meeting,"mic","Corrected speech, not Notes",selected.revision).await.unwrap();
    let after = notes_document::read(&pool,&target).await.unwrap().unwrap();
    assert_eq!(after.revision,note_before.revision); assert_eq!(after.json,Some(blocks()));
    assert_eq!(after.markdown.as_deref(),Some(markdown)); assert_eq!(after.source,"user_notes");
    // Old normal-save and crash-recovery callers must not restore old speech or Notes.
    let retry = TranscriptsRepository::save_transcript(&pool,"Retry",&[turn("ignored","old speech","Guest")],Some("/synthetic/combined".into()),None).await.unwrap();
    let recovered = TranscriptsRepository::save_recovered_transcript(&pool,"Recovered",&[],Some("/synthetic/combined".into()),None,"synthetic-recovery").await.unwrap();
    assert_eq!(retry,meeting); assert_eq!(recovered,meeting);
    assert!(versions::selected(&pool,&meeting).await.unwrap().segments.iter().any(|row| row.text=="Corrected speech, not Notes"));
    assert_eq!(notes_document::read(&pool,"recording:/synthetic/unrelated").await.unwrap().unwrap().markdown.as_deref(),Some("Other draft"));
    let final_revision = versions::selected(&pool,&meeting).await.unwrap().revision;
    pool.close().await;
    let reopened = SqlitePool::connect_with(options).await.unwrap();
    let restored = versions::selected(&reopened,&meeting).await.unwrap();
    assert_eq!(restored.version_id,candidate); assert_eq!(restored.revision,final_revision);
    assert_eq!(notes_document::read(&reopened,&target).await.unwrap().unwrap().json,Some(blocks()));
    assert_eq!(notes_document::read(&reopened,alias).await.unwrap().unwrap().revision,note_before.revision);
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&reopened).await.unwrap().is_empty());
}

#[tokio::test]
async fn combined_pending_summary_and_notes_competing_writer_are_separate_revision_domains() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap(); migrate(&pool).await;
    let meeting = TranscriptsRepository::save_transcript(&pool,"Meeting",&[turn("ignored","Live speech","You")],None,None).await.unwrap();
    let target = format!("meeting:{meeting}");
    notes_document::write(&pool,&target,"Local Notes",Some(blocks()),0).await.unwrap();
    let speech_before = versions::selected(&pool,&meeting).await.unwrap();
    // Writer B commits; stale A cannot overwrite B, even around a version switch.
    notes_document::write(&pool,&target,"Competing writer",Some(blocks()),1).await.unwrap();
    assert!(notes_document::write(&pool,&target,"Stale local",None,1).await.is_err());
    assert_eq!(versions::selected(&pool,&meeting).await.unwrap().revision,speech_before.revision);
    let candidate = versions::begin(&pool,&meeting,&serde_json::json!({})).await.unwrap();
    versions::finish(&pool,&candidate,&[turn("candidate","New speech","You")]).await.unwrap();
    versions::set_status(&pool,&candidate,"ready",None).await.unwrap();
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES(?,'pending','2026','2026','pending summary')").bind(&meeting).execute(&pool).await.unwrap();
    assert!(versions::select(&pool,&meeting,&candidate,speech_before.revision).await.is_err());
    let line = &speech_before.segments[0].id;
    assert!(versions::correct_text(&pool,&meeting,line,"must not apply",speech_before.revision).await.is_err());
    assert_eq!(versions::selected(&pool,&meeting).await.unwrap().revision,speech_before.revision);
    assert_eq!(notes_document::read(&pool,&target).await.unwrap().unwrap().markdown.as_deref(),Some("Competing writer"));
    sqlx::query("UPDATE summary_processes SET status='completed' WHERE meeting_id=?").bind(&meeting).execute(&pool).await.unwrap();
    versions::select(&pool,&meeting,&candidate,speech_before.revision).await.unwrap();
    let archived: Option<String> = sqlx::query_scalar("SELECT summary_json FROM transcript_versions WHERE meeting_id=? AND kind='corrections'").bind(&meeting).fetch_one(&pool).await.unwrap();
    assert!(archived.unwrap().contains("pending summary"));
    assert_eq!(notes_document::read(&pool,&target).await.unwrap().unwrap().revision,2);
    assert_eq!(notes_document::read(&pool,&target).await.unwrap().unwrap().json,Some(blocks()));
}

#[tokio::test]
async fn compatibility_first_notes_insert_rejects_absent_document_cas() {
    use crate::database::repositories::meeting_notes::api_save_meeting_notes;
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    migrate(&pool).await;
    let meeting = TranscriptsRepository::save_transcript(&pool, "Meeting", &[], None, None).await.unwrap();
    let target = format!("meeting:{meeting}");
    // Editor A loads absence at revision zero, then compatibility writer B saves.
    assert!(notes_document::read(&pool, &target).await.unwrap().is_none());
    let state = crate::state::AppState { db_manager: crate::state::DbManager(pool.clone()) };
    let markdown = "## **Заметки B / Notes**\n\n- *API follow-up*";
    api_save_meeting_notes(crate::State(&state), meeting.clone(), Some(markdown.into()), Some(blocks())).await.unwrap();
    let stale = notes_document::write(&pool, &target, "A stale draft", Some(serde_json::json!([])), 0).await;
    assert!(stale.is_err(), "absent-document CAS must not overwrite the first compatibility save");
    let saved = notes_document::read(&pool, &target).await.unwrap().unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.markdown.as_deref(), Some(markdown));
    assert_eq!(saved.json, Some(blocks()));
    assert_eq!(saved.source, "user_notes");
    assert_eq!(notes_document::write(&pool, &target, "Reviewed A", Some(blocks()), saved.revision).await.unwrap(), 2);
    for revision in [3, 4] {
        api_save_meeting_notes(crate::State(&state), meeting.clone(), Some(markdown.into()), Some(blocks())).await.unwrap();
        let saved = notes_document::read(&pool, &target).await.unwrap().unwrap();
        assert_eq!(saved.revision, revision);
        assert_eq!(saved.markdown.as_deref(), Some(markdown));
        assert_eq!(saved.json, Some(blocks()));
    }
}

#[tokio::test]
async fn compatibility_revision_fix_preserves_preexisting_migrated_zero_notes() {
    use crate::database::repositories::meeting_notes::api_save_meeting_notes;
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    migrate_with_legacy_notes(&pool, true).await;
    let target = "meeting:legacy-notes";
    let saved = notes_document::read(&pool, target).await.unwrap().unwrap();
    assert_eq!(saved.revision, 0);
    assert_eq!(saved.markdown.as_deref(), Some("Старые Notes"));
    assert_eq!(saved.json, Some(serde_json::json!([{"type":"paragraph","content":"exact old blocks"}])));
    assert_eq!(notes_document::write(&pool, target, "Reviewed migrated Notes", Some(blocks()), 0).await.unwrap(), 1);
    let state = crate::state::AppState { db_manager: crate::state::DbManager(pool.clone()) };
    api_save_meeting_notes(crate::State(&state), "legacy-notes".into(), Some("Legacy update".into()), Some(blocks())).await.unwrap();
    let saved = notes_document::read(&pool, target).await.unwrap().unwrap();
    assert_eq!(saved.revision, 2);
    assert_eq!(saved.markdown.as_deref(), Some("Legacy update"));
    assert_eq!(saved.json, Some(blocks()));
}
