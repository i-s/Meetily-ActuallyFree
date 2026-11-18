use crate::commands::*;
use crate::database::repositories::library;
use crate::state::{AppState, Db};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

async fn db() -> SqlitePool {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    for migration in crate::migrations::MIGRATIONS { sqlx::raw_sql(migration).execute(&pool).await.unwrap(); }
    pool
}
async fn meeting(pool: &SqlitePool, id: &str, title: &str) {
    sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at) VALUES(?,?, '2026-11-18T10:00:00Z','2026-11-18T10:00:00Z')").bind(id).bind(title).execute(pool).await.unwrap();
}
async fn notes(pool: &SqlitePool, id: &str, text: &str, revision: i64) {
    sqlx::query("INSERT INTO meeting_notes(meeting_id,notes_markdown,revision,created_at,updated_at) VALUES(?,?,?,'now','now')").bind(id).bind(text).bind(revision).execute(pool).await.unwrap();
}
async fn phrase(pool: &SqlitePool, id: &str, meeting: &str, text: &str, time: f64) {
    sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,timestamp,speaker,audio_start_time) VALUES(?,?,?,'00:00','You',?)").bind(id).bind(meeting).bind(text).bind(time).execute(pool).await.unwrap();
}

#[tokio::test]
async fn normal_search_command_matches_cyrillic_case_notes_and_late_pages_literally() {
    let pool = db().await;
    meeting(&pool, "old", "СТАРАЯ ВСТРЕЧА").await;
    notes(&pool,"old", "Моя МыСлЬ, не обещание. 50%_off\\today", 4).await;
    for i in 0..770 { phrase(&pool,&format!("line-{i:04}"),"old",if i == 769 { "Поздняя РЕПЛИКА" } else { "nothing" },i as f64).await; }
    let global = crate::database::repositories::person::PeopleRepository::global_search(&pool,"мысль",Some(40)).await.unwrap();
    assert_eq!(global.len(),1); assert_eq!(global[0].kind,"notes"); assert_eq!(global[0].meeting_id.as_deref(),Some("old"));
    let global = crate::database::repositories::person::PeopleRepository::global_search(&pool,"реПлика",Some(40)).await.unwrap();
    assert_eq!(global[0].transcript_id.as_deref(),Some("line-0769"));
    let state = AppState { db_manager: Db(pool.clone()) };
    let hit = api_search_library(&state,"репЛИка".into()).await.unwrap();
    assert_eq!(hit.len(),1); assert_eq!(hit[0].transcript_id.as_deref(),Some("line-0769"));
    let hit = api_search_library(&state,"мысль".into()).await.unwrap();
    assert_eq!(hit[0].kind,"notes"); assert!(hit[0].snippet.contains("МыСлЬ"));
    assert_eq!(api_search_library(&state,"50%_off\\today".into()).await.unwrap().len(),1);
    assert!(api_search_library(&state,"50XXoff".into()).await.unwrap().is_empty());
    assert_eq!(api_search_library(&state,"старая встреча".into()).await.unwrap()[0].kind,"meeting");
    assert!(api_search_library(&state," ".into()).await.unwrap().is_empty());
}

#[tokio::test]
async fn search_is_bounded_and_searches_only_visible_summary_text() {
    let pool = db().await;
    for i in 0..210 {
        let id = format!("m-{i:03}"); meeting(&pool,&id,"Meeting").await;
        notes(&pool,&id,"ОбЩаЯ заметка",0).await;
    }
    assert_eq!(library::search(&pool,"общая",200,true).await.unwrap().len(),200);
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m-000','completed','now','now',?)")
        .bind(r#"{"markdown":"Видимый ВыВоД","english_cache":"hidden-secret","summary_json":[{"id":"hidden-id"}]}"#).execute(&pool).await.unwrap();
    assert_eq!(library::search(&pool,"вывод",10,false).await.unwrap()[0].kind,"summary");
    assert!(library::search(&pool,"hidden-secret",10,false).await.unwrap().is_empty());
    assert!(library::search(&pool,"hidden-id",10,false).await.unwrap().is_empty());
}

#[tokio::test]
async fn normal_snapshot_and_material_commands_keep_notes_only_and_empty_legacy_meetings() {
    let pool = db().await;
    meeting(&pool,"notes","Notes only").await; notes(&pool,"notes","Идея, не речь",3).await;
    meeting(&pool,"empty","Legacy").await;
    let state = AppState { db_manager: Db(pool.clone()) };
    let rows = api_library_materials(&state).await.unwrap();
    let row = rows.iter().find(|r|r.id=="notes").unwrap();
    assert!(row.has_notes); assert!(!row.audio_available); assert!(!row.has_summary); assert_eq!(row.transcript_count,0);
    let snapshot = api_export_meeting_snapshot(&state,"notes".into(),3).await.unwrap();
    assert_eq!(snapshot.notes.source,"user_notes"); assert_eq!(snapshot.notes.markdown,"Идея, не речь");
    assert!(snapshot.transcripts.is_empty()); assert!(snapshot.summary.is_none()); assert!(!snapshot.audio_available);
    assert!(api_export_meeting_snapshot(&state,"empty".into(),0).await.is_ok());
    assert!(api_export_meeting_snapshot(&state,"gone".into(),0).await.is_err());
    assert!(api_export_meeting_snapshot(&state,"notes".into(),2).await.unwrap_err().contains("Notes changed"));
}

#[tokio::test]
async fn snapshot_preserves_all_selected_rows_and_audio_removal_does_not_remove_text() {
    let pool = db().await; meeting(&pool,"m","Real title").await;
    let audio = tempfile::tempdir().unwrap(); let path = audio.path().join("audio.WAV"); std::fs::write(&path,b"synthetic fixture, not playable audio").unwrap();
    sqlx::query("UPDATE meetings SET folder_path=? WHERE id='m'").bind(audio.path().to_str().unwrap()).execute(&pool).await.unwrap();
    for i in 0..1205 { phrase(&pool,&format!("t-{i:04}"),"m",&format!("Строка {i}"),i as f64).await; }
    let before = library::export_snapshot(&pool,"m",0).await.unwrap();
    assert!(before.audio_available); assert_eq!(before.transcripts.len(),1205); assert_eq!(before.transcripts[1204].text,"Строка 1204");
    std::fs::remove_file(path).unwrap();
    let after = library::export_snapshot(&pool,"m",0).await.unwrap();
    assert!(!after.audio_available); assert_eq!(after.transcripts.len(),1205);
    assert_eq!(library::search(&pool,"строка 1204",10,false).await.unwrap()[0].transcript_id.as_deref(),Some("t-1204"));
}

#[tokio::test]
async fn wal_snapshot_never_combines_old_transcript_and_new_summary_during_projection_change() {
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new().filename(dir.path().join("fixture.sqlite")).create_if_missing(true).journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new().max_connections(3).connect_with(options).await.unwrap();
    for migration in crate::migrations::MIGRATIONS { sqlx::raw_sql(migration).execute(&pool).await.unwrap(); }
    meeting(&pool,"m","old title").await; notes(&pool,"m","notes",1).await; phrase(&pool,"line","m","old speech",0.0).await;
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','completed','now','now','old summary')").execute(&pool).await.unwrap();
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE transcripts SET transcript='corrected speech' WHERE meeting_id='m'").execute(&mut *writer).await.unwrap();
    sqlx::query("UPDATE summary_processes SET result=NULL,status='idle' WHERE meeting_id='m'").execute(&mut *writer).await.unwrap();
    sqlx::query("UPDATE meetings SET title='new title' WHERE id='m'").execute(&mut *writer).await.unwrap();
    let before = library::export_snapshot(&pool,"m",1).await.unwrap();
    assert_eq!(before.title,"old title"); assert_eq!(before.transcripts[0].text,"old speech"); assert_eq!(before.summary.as_deref(),Some("old summary"));
    writer.commit().await.unwrap();
    let after = library::export_snapshot(&pool,"m",1).await.unwrap();
    assert_eq!(after.title,"new title"); assert_eq!(after.transcripts[0].text,"corrected speech"); assert!(after.summary.is_none());
    assert_eq!(after.notes.markdown,before.notes.markdown);
    assert!(library::search(&pool,"old speech",10,false).await.unwrap().is_empty());
    assert_eq!(library::search(&pool,"corrected speech",10,false).await.unwrap().len(),1);
}

#[cfg(feature = "versions")]
#[tokio::test]
async fn selected_versions_compatibility_uses_actual_selection_and_correction_without_archived_summaries() {
    use crate::transcript_versions as versions;
    let pool = db().await;
    meeting(&pool, "m", "Meeting").await;
    notes(&pool, "m", "Моя идея не речь", 7).await;
    phrase(&pool, "live-row", "m", "Live words", 2.0).await;
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','completed','now','now','Former summary')").execute(&pool).await.unwrap();
    let baseline = versions::selected(&pool, "m").await.unwrap();
    let candidate = versions::begin(&pool, "m", &serde_json::json!({"model":"synthetic"})).await.unwrap();
    versions::finish(&pool, &candidate, &[crate::api::TranscriptSegment { id:"refined-row".into(),text:"Уточнённая РЕПЛИКА".into(),timestamp:"00:02".into(),audio_start_time:Some(2.0),audio_end_time:Some(3.0),duration:Some(1.0),speaker:Some("You".into()) }]).await.unwrap();
    versions::set_status(&pool, &candidate, "ready", None).await.unwrap();
    let before = library::export_snapshot(&pool, "m", 7).await.unwrap();
    assert_eq!(before.transcripts[0].text, "Live words");
    assert!(library::search(&pool, "уточнённая", 10, false).await.unwrap().is_empty());
    versions::select(&pool, "m", &candidate, baseline.revision).await.unwrap();
    let selected = versions::selected(&pool, "m").await.unwrap();
    let after = library::export_snapshot(&pool, "m", 7).await.unwrap();
    assert_eq!(after.transcripts[0].text, selected.segments[0].text);
    assert_eq!(after.notes.markdown, before.notes.markdown);
    assert!(after.summary.is_none());
    assert!(library::search(&pool, "Former summary", 10, false).await.unwrap().is_empty());
    assert_eq!(library::search(&pool, "реплика", 10, false).await.unwrap()[0].transcript_id.as_deref(), Some("refined-row"));
    versions::correct_text(&pool, "m", "refined-row", "Исправленный API", selected.revision).await.unwrap();
    let corrected = library::export_snapshot(&pool, "m", 7).await.unwrap();
    assert_eq!(corrected.transcripts[0].text, "Исправленный API");
    assert_eq!(corrected.notes.revision, 7);
    assert!(library::search(&pool, "уточнённая", 10, false).await.unwrap().is_empty());
    assert_eq!(library::search(&pool, "исправленный", 10, false).await.unwrap().len(), 1);
}

#[tokio::test]
async fn legacy_rich_notes_without_markdown_are_not_silently_empty_or_searched_by_editor_ids() {
    let pool = db().await; meeting(&pool,"legacy-rich","Legacy rich Notes").await;
    let json = r#"[{"id":"private-editor-id","type":"table","content":{"rows":[{"cells":[[{"type":"text","text":"Табличная МыСлЬ"}]]}]}}]"#;
    sqlx::query("INSERT INTO meeting_notes(meeting_id,notes_json,created_at,updated_at) VALUES('legacy-rich',?,'now','now')").bind(json).execute(&pool).await.unwrap();
    assert!(library::materials(&pool).await.unwrap()[0].has_notes);
    assert_eq!(library::search(&pool,"мысль",10,false).await.unwrap()[0].kind,"notes");
    assert!(library::search(&pool,"private-editor-id",10,false).await.unwrap().is_empty());
    let snapshot = serde_json::to_value(library::export_snapshot(&pool,"legacy-rich",0).await.unwrap()).unwrap();
    assert_eq!(snapshot["notes"]["json"],serde_json::from_str::<serde_json::Value>(json).unwrap());
}
