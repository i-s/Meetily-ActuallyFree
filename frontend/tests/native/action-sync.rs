//! Real SQLite interleavings with production guarded extraction and version SQL.
use crate::database::repositories::{action_sync as sync, action_item::{ActionItemsRepository as Tasks, ActionItemDraft, ActionItemFilter}, library, notes_document};
use crate::transcript_versions as versions;
use serde_json::{json, Value};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

fn full() -> Value { json!({"markdown":"## Overview\nOld summary\n\n## Action items\n- Send API report"}) }
fn trimmed() -> Value { json!({"markdown":"## Overview\nOld summary","original_markdown":full()["markdown"]}) }
fn draft(id: &str) -> ActionItemDraft { ActionItemDraft { text:"Send API report".into(),owner_label:None,due_text:None,audio_time:Some(2.0),transcript_id:Some(id.into()) } }
async fn db() -> SqlitePool {
    let pool=SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    for migration in crate::migrations::MIGRATIONS { sqlx::raw_sql(migration).execute(&pool).await.unwrap(); }
    sqlx::raw_sql("INSERT INTO meetings(id,title,created_at,updated_at) VALUES('m','Meeting','2026','2026'),('other','Other','2026','2026'); INSERT INTO transcripts(id,meeting_id,transcript,timestamp,audio_start_time,audio_end_time,speaker) VALUES('line','m','Send API report','00:02',2,3,'You'),('foreign-line','other','Send API report','00:02',2,3,'You');").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','completed','same','same',?)").bind(full().to_string()).execute(&pool).await.unwrap();
    notes_document::write(&pool,"meeting:m","Private Notes",Some(json!([{"type":"paragraph","content":"Private Notes"}])),0).await.unwrap();
    Tasks::sync_ai(&pool,"m",vec![draft("line")],"before").await.unwrap();
    let items=Tasks::list(&pool,&ActionItemFilter{meeting_id:Some("m".into()),..Default::default()}).await.unwrap();
    Tasks::update(&pool,&items[0].id,"User-curated API task",None,None,None,true).await.unwrap();
    pool
}
async fn state(pool: &SqlitePool) -> Value {
    let items=Tasks::list(pool,&ActionItemFilter{meeting_id:Some("m".into()),..Default::default()}).await.unwrap();
    let summary: Option<String>=sqlx::query_scalar("SELECT result FROM summary_processes WHERE meeting_id='m'").fetch_optional(pool).await.unwrap().flatten();
    let marker: Option<String>=sqlx::query_scalar("SELECT action_items_source FROM meetings WHERE id='m'").fetch_one(pool).await.unwrap();
    let anchors:Vec<(String,String,Option<String>,Option<f64>)>=sqlx::query_as("SELECT action_id,version_id,transcript_id,audio_time FROM action_item_transcript_sources ORDER BY action_id").fetch_all(pool).await.unwrap();
    json!({"items":items,"summary":summary,"marker":marker,"anchors":anchors})
}

#[tokio::test]
async fn delayed_action_sync_after_select_and_correct_is_a_database_and_export_noop() {
    for operation in ["select","correct"] {
        let pool=db().await;
        let context=sync::context(&pool,"m").await.unwrap().unwrap();
        let selected=versions::selected(&pool,"m").await.unwrap();
        if operation=="select" {
            let candidate=versions::begin(&pool,"m",&json!({})).await.unwrap();
            let turn=crate::api::TranscriptSegment{id:"new-line".into(),text:"New projection".into(),timestamp:"00:02".into(),audio_start_time:Some(2.0),audio_end_time:Some(3.0),duration:Some(1.0),speaker:Some("You".into())};
            versions::finish(&pool,&candidate,&[turn]).await.unwrap();versions::set_status(&pool,&candidate,"ready",None).await.unwrap();
            versions::select(&pool,"m",&candidate,selected.revision).await.unwrap();
        } else { versions::correct_text(&pool,"m","line","Corrected projection",selected.revision).await.unwrap(); }
        let before=state(&pool).await;
        assert!(sync::commit(&pool,&context.guard,vec![draft("line")],"late",Some(trimmed())).await.unwrap().is_none());
        assert_eq!(state(&pool).await,before);
        let exported=library::export_snapshot(&pool,"m",1).await.unwrap();
        assert!(exported.summary.is_none());assert_eq!(exported.notes.markdown,"Private Notes");
        assert_eq!(exported.notes.revision,1);assert!(exported.transcripts[0].text.contains("projection"));
    }
}

#[tokio::test]
async fn summary_ownership_rejects_same_row_identical_regeneration_and_delete_reinsert_and_edits() {
    for writer in ["upsert","reinsert","edit","new-content"] {
        let pool=db().await;
        let context=sync::context(&pool,"m").await.unwrap().unwrap();
        if writer=="upsert" {
            // The real summary writer resets an existing PK via UPSERT, keeping
            // result until completion. Even identical final bytes are a new job.
            sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','PENDING','same','same',NULL) ON CONFLICT(meeting_id) DO UPDATE SET status='PENDING',result_backup=result,result=result,error=NULL").execute(&pool).await.unwrap();
            assert!(sync::context(&pool,"m").await.unwrap().is_none());
            sqlx::query("UPDATE summary_processes SET status='completed',result=?,result_backup=NULL,user_edited=0 WHERE meeting_id='m'").bind(full().to_string()).execute(&pool).await.unwrap();
        } else if writer=="reinsert" {
            sqlx::query("DELETE FROM summary_processes WHERE meeting_id='m'").execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result) VALUES('m','completed','same','same',?)").bind(full().to_string()).execute(&pool).await.unwrap();
        } else {
            sqlx::query("UPDATE summary_processes SET result=?,user_edited=1 WHERE meeting_id='m'").bind(if writer=="edit"{full()}else{json!({"markdown":"New summary"})}.to_string()).execute(&pool).await.unwrap();
        }
        let current=versions::selected(&pool,"m").await.unwrap();assert_eq!(current.revision,context.guard.revision);
        let before=state(&pool).await;
        assert!(sync::commit(&pool,&context.guard,vec![draft("line")],"late",Some(trimmed())).await.unwrap().is_none());
        // The same precondition also guards no-action-section backfill marker
        // and task reconciliation; no independent meeting-only fallback exists.
        assert!(sync::commit(&pool,&context.guard,vec![],"late-empty",None).await.unwrap().is_none());
        assert_eq!(state(&pool).await,before);
        let exported=library::export_snapshot(&pool,"m",1).await.unwrap();
        assert_eq!(exported.summary,before["summary"].as_str().map(str::to_string));
    }
}

#[tokio::test]
async fn ordinary_guarded_sync_is_atomic_keeps_curated_tasks_notes_and_current_anchors() {
    let pool=db().await;
    let app_state = crate::state::AppState { db_manager: crate::state::Db(pool.clone()) };
    let context=sync::api_action_sync_context(&app_state,"m".into()).await.unwrap().unwrap();
    let kept=self::state(&pool).await["items"][0].clone();
    let items=sync::api_commit_action_sync(&app_state,context.guard.clone(),vec![draft("line")],"synced".into(),Some(trimmed())).await.unwrap().unwrap();
    assert_eq!(items.len(),2);assert_eq!(serde_json::to_value(&items[0]).unwrap(),kept);
    assert_eq!(items[1].transcript_id.as_deref(),Some("line"));assert_eq!(items[1].audio_time,Some(2.0));
    let exported=library::export_snapshot(&pool,"m",1).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&exported.summary.unwrap()).unwrap(),trimmed());
    assert_eq!(exported.notes.markdown,"Private Notes");assert_eq!(exported.actions.len(),2);
    assert!(sync::commit(&pool,&context.guard,vec![],"duplicate",Some(trimmed())).await.unwrap().is_none());
    let next=sync::context(&pool,"m").await.unwrap().unwrap();
    assert!(sync::commit(&pool,&next.guard,vec![],"read-empty",None).await.unwrap().is_some());
    assert_eq!(state(&pool).await["items"].as_array().unwrap().len(),2); // a no-section read retains every task
    assert_eq!(state(&pool).await["marker"],"read-empty");
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn bad_anchor_or_summary_failure_rolls_back_tasks_marker_and_summary() {
    let pool=db().await;
    let context=sync::context(&pool,"m").await.unwrap().unwrap();
    let before=state(&pool).await;
    assert!(sync::commit(&pool,&context.guard,vec![draft("foreign-line")],"bad",Some(trimmed())).await.is_err());
    assert_eq!(state(&pool).await,before);
    sqlx::raw_sql("CREATE TRIGGER fail_trim BEFORE UPDATE OF result ON summary_processes BEGIN SELECT RAISE(ABORT,'synthetic trim failure'); END;").execute(&pool).await.unwrap();
    assert!(sync::commit(&pool,&context.guard,vec![draft("line")],"fail",Some(trimmed())).await.is_err());
    assert_eq!(state(&pool).await,before);

}


#[tokio::test]
async fn transcript_edit_with_unchanged_summary_ownership_rejects_stale_context() {
    let pool=db().await;
    let context=sync::context(&pool,"m").await.unwrap().unwrap();
    sqlx::query("UPDATE transcripts SET transcript='New words' WHERE meeting_id='m'").execute(&pool).await.unwrap();
    let next=sync::context(&pool,"m").await.unwrap().unwrap();
    assert_eq!(next.guard.summary_token,context.guard.summary_token);
    assert_ne!(next.guard.revision,context.guard.revision);
    let before=state(&pool).await;
    assert!(sync::commit(&pool,&context.guard,vec![draft("line")],"stale",Some(trimmed())).await.unwrap().is_none());
    assert_eq!(state(&pool).await,before);
}
