use crate::audio_cleanup::*;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{fs, path::PathBuf};

struct Fixture {
    pool: SqlitePool,
    root: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
async fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "meetily-cleanup-generated-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(root.join("fixture.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY,title TEXT,created_at TEXT,folder_path TEXT); CREATE TABLE transcript_versions(meeting_id TEXT,status TEXT); CREATE TABLE post_call_jobs(meeting_id TEXT PRIMARY KEY,status TEXT,stage TEXT); CREATE TABLE retained(kind TEXT,content TEXT);").execute(&pool).await.unwrap();
    for kind in [
        "transcripts",
        "corrections",
        "Notes",
        "summary",
        "tasks",
        "queue_results",
    ] {
        sqlx::query("INSERT INTO retained VALUES(?,?)")
            .bind(kind)
            .bind(format!("generated {kind}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    Fixture { pool, root }
}
async fn meeting(f: &Fixture, id: &str) -> PathBuf {
    let dir = f.root.join(id);
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("metadata.json"), br#"{"status":"completed","audio_file":"audio.mp4","mic_audio_file":"mic.mp4","system_audio_file":"system.mp4"}"#).unwrap();
    for name in ["audio.mp4", "mic.mp4", "system.mp4"] {
        fs::write(dir.join(name), b"GENERATED-NOT-REAL-AUDIO").unwrap();
    }
    fs::write(dir.join("transcripts.json"), b"generated transcript backup").unwrap();
    sqlx::query("INSERT INTO meetings VALUES(?,?,'2026-01-01T00:00:00Z',?)")
        .bind(id)
        .bind(format!("Meeting {id}"))
        .bind(dir.to_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    dir
}
#[tokio::test]
async fn consent_is_scoped_single_use_and_preserves_every_non_audio_material() {
    let f = fixture().await;
    let a = meeting(&f, "a").await;
    let b = meeting(&f, "b").await;
    let store = Plans::default();
    let inv = inventory(&f.pool).await.unwrap();
    assert_eq!(inv.total_bytes, 144);
    assert!(inv.recordings.iter().all(|r| r.blocked.is_none()));
    let plan = prepare(&f.pool, &store, vec!["a".into()]).await.unwrap();
    assert_eq!(plan.recordings.len(), 1);
    assert!(a.join("audio.mp4").exists());
    assert!(execute(&f.pool, &store, "invented").await.is_err());
    let result = execute(&f.pool, &store, &plan.token).await.unwrap();
    assert_eq!(result.removed_bytes, 72);
    assert_eq!(result.files.len(), 3);
    assert!(execute(&f.pool, &store, &plan.token).await.is_err());
    assert!(!a.join("audio.mp4").exists());
    assert!(b.join("audio.mp4").exists());
    assert!(a.join("metadata.json").exists());
    assert!(a.join("transcripts.json").exists());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM retained")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        6
    );
}
#[tokio::test]
async fn job_and_version_changes_after_review_block_deletion_and_keep_queue_rows() {
    let f = fixture().await;
    let dir = meeting(&f, "a").await;
    let plans = Plans::default();
    let p = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    sqlx::query("INSERT INTO post_call_jobs VALUES('a','running','local')")
        .execute(&f.pool)
        .await
        .unwrap();
    let r = execute(&f.pool, &plans, &p.token).await.unwrap();
    assert_eq!(r.removed_bytes, 0);
    assert!(r.files.iter().all(|f| f.status == "blocked"));
    assert!(dir.join("audio.mp4").exists());
    for status in ["queued", "waiting", "deferred", "running"] {
        sqlx::query("UPDATE post_call_jobs SET status=?")
            .bind(status)
            .execute(&f.pool)
            .await
            .unwrap();
        assert!(prepare(&f.pool, &plans, vec!["a".into()]).await.is_err());
    }
    sqlx::query("UPDATE post_call_jobs SET status='cancelled'")
        .execute(&f.pool)
        .await
        .unwrap();
    let p = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    sqlx::query("INSERT INTO transcript_versions VALUES('a','running')")
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(
        execute(&f.pool, &plans, &p.token)
            .await
            .unwrap()
            .removed_bytes,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM post_call_jobs")
            .fetch_one(&f.pool)
            .await
            .unwrap(),
        "cancelled"
    );
}
#[tokio::test]
async fn recovery_unknown_audio_aliases_and_links_fail_closed() {
    let f = fixture().await;
    let a = meeting(&f, "a").await;
    let plans = Plans::default();
    fs::create_dir(a.join(".checkpoints")).unwrap();
    assert!(prepare(&f.pool, &plans, vec!["a".into()]).await.is_err());
    fs::remove_dir(a.join(".checkpoints")).unwrap();
    fs::write(a.join("unrelated.wav"), b"generated unrelated").unwrap();
    assert!(prepare(&f.pool, &plans, vec!["a".into()]).await.is_err());
    fs::remove_file(a.join("unrelated.wav")).unwrap();
    sqlx::query("INSERT INTO meetings VALUES('alias','alias','now',?)")
        .bind(a.join(".").to_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(prepare(&f.pool, &plans, vec!["a".into()]).await.is_err());
    sqlx::query("DELETE FROM meetings WHERE id='alias'")
        .execute(&f.pool)
        .await
        .unwrap();
    #[cfg(unix)]
    {
        fs::remove_file(a.join("mic.mp4")).unwrap();
        std::os::unix::fs::symlink(a.join("audio.mp4"), a.join("mic.mp4")).unwrap();
        assert!(prepare(&f.pool, &plans, vec!["a".into()]).await.is_err());
    }
}
#[tokio::test]
async fn changed_file_rejects_scope_without_claiming_freed_bytes_and_retry_is_fresh() {
    let f = fixture().await;
    let dir = meeting(&f, "a").await;
    let plans = Plans::default();
    let p = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    fs::write(dir.join("audio.mp4"), b"replacement").unwrap();
    let r = execute(&f.pool, &plans, &p.token).await.unwrap();
    assert_eq!(r.removed_bytes, 0);
    assert!(dir.join("mic.mp4").exists());
    let retry = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    let r = execute(&f.pool, &plans, &retry.token).await.unwrap();
    assert_eq!(r.removed_bytes, 59);
}
#[tokio::test]
async fn one_changed_meeting_does_not_hide_success_for_another() {
    let f = fixture().await;
    let a = meeting(&f, "a").await;
    let b = meeting(&f, "b").await;
    let plans = Plans::default();
    let p = prepare(&f.pool, &plans, vec!["a".into(), "b".into()])
        .await
        .unwrap();
    fs::write(
        a.join("metadata.json"),
        br#"{"status":"recording","audio_file":"audio.mp4"}"#,
    )
    .unwrap();
    let r = execute(&f.pool, &plans, &p.token).await.unwrap();
    assert_eq!(r.removed_bytes, 72);
    assert!(a.join("audio.mp4").exists());
    assert!(!b.join("audio.mp4").exists());
    assert!(r.files.iter().any(|f| f.status == "blocked"));
}
#[tokio::test]
async fn protected_recovery_bytes_are_visible_and_shared_folders_are_not_double_counted() {
    let f = fixture().await;
    let dir = meeting(&f, "a").await;
    fs::create_dir_all(dir.join(".checkpoints/mic")).unwrap();
    fs::write(dir.join(".checkpoints/mic/mic_chunk_000.mp4"), b"DATA").unwrap();
    let inv = inventory(&f.pool).await.unwrap();
    assert_eq!(inv.total_bytes, 76);
    assert!(inv.recordings[0].blocked.is_some());
    sqlx::query("INSERT INTO meetings VALUES('alias','alias','now',?)")
        .bind(dir.to_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(inventory(&f.pool).await.unwrap().total_bytes, 76);
    sqlx::query("INSERT INTO meetings VALUES('nested','nested','now',?)")
        .bind(dir.join(".checkpoints/mic").to_str().unwrap())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_eq!(inventory(&f.pool).await.unwrap().total_bytes, 76);
}
#[cfg(unix)]
#[tokio::test]
async fn actual_unlink_failure_reports_only_successful_bytes_and_fresh_retry() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture().await;
    let a = meeting(&f, "a").await;
    let b = meeting(&f, "b").await;
    let plans = Plans::default();
    let p = prepare(&f.pool, &plans, vec!["a".into(), "b".into()])
        .await
        .unwrap();
    fs::set_permissions(&a, fs::Permissions::from_mode(0o555)).unwrap();
    let r = execute(&f.pool, &plans, &p.token).await.unwrap();
    fs::set_permissions(&a, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(r.removed_bytes, 72);
    assert_eq!(r.files.iter().filter(|f| f.status == "error").count(), 3);
    assert!(a.join("audio.mp4").exists());
    assert!(!b.join("audio.mp4").exists());
    let p = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    assert_eq!(
        execute(&f.pool, &plans, &p.token)
            .await
            .unwrap()
            .removed_bytes,
        72
    );
}
#[tokio::test]
async fn normal_command_rejects_active_capture_and_draining_native_jobs() {
    use std::sync::atomic::Ordering;
    let f = fixture().await;
    let dir = meeting(&f, "a").await;
    let state = crate::state::AppState {
        db_manager: crate::state::Db(f.pool.clone()),
    };
    let plan = crate::commands::api_prepare_audio_cleanup(&state, vec!["a".into()])
        .await
        .unwrap();
    crate::audio::recording_commands::ACTIVE.store(true, Ordering::SeqCst);
    assert!(crate::commands::api_audio_storage(&state)
        .await
        .unwrap()
        .recordings[0]
        .blocked
        .is_some());
    let app = || crate::tauri::AppHandle::<crate::tauri::TestRuntime>::default();
    assert!(
        crate::commands::api_confirm_audio_cleanup(app(), &state, plan.token.clone())
            .await
            .is_err()
    );
    crate::audio::recording_commands::ACTIVE.store(false, Ordering::SeqCst);
    let reader = crate::diarization::operation_guard().await;
    assert!(
        crate::commands::api_confirm_audio_cleanup(app(), &state, plan.token.clone())
            .await
            .is_err()
    );
    drop(reader);
    let lifecycle = crate::audio::common::acquire_engine_lifecycle_lock().await;
    assert!(
        crate::commands::api_confirm_audio_cleanup(app(), &state, plan.token.clone())
            .await
            .is_err()
    );
    assert!(dir.join("audio.mp4").exists());
    drop(lifecycle);
    assert_eq!(
        crate::commands::api_confirm_audio_cleanup(app(), &state, plan.token)
            .await
            .unwrap()
            .removed_bytes,
        72
    );
}

async fn retained_dump(pool: &SqlitePool) -> Vec<(String, Vec<String>)> {
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(pool).await.unwrap();
    let mut dump = vec![];
    for table in tables {
        let columns: Vec<(i64, String, String, i64, Option<String>, i64)> =
            sqlx::query_as(&format!("PRAGMA table_info(\"{table}\")"))
                .fetch_all(pool)
                .await
                .unwrap();
        let expression = columns
            .iter()
            .map(|c| format!("quote(\"{}\")", c.1))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let rows = sqlx::query_scalar::<_, String>(&format!(
            "SELECT {expression} FROM \"{table}\" ORDER BY rowid"
        ))
        .fetch_all(pool)
        .await
        .unwrap();
        dump.push((table, rows));
    }
    dump
}
#[tokio::test]
async fn actual_library_search_and_export_survive_cleanup_with_all_database_rows_unchanged() {
    let mut f = fixture().await;
    let dir = meeting(&f, "a").await;
    f.pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(f.root.join("real-schema.sqlite"))
                .create_if_missing(true),
        )
        .await
        .unwrap();
    for migration in crate::migrations::MIGRATIONS {
        sqlx::raw_sql(migration).execute(&f.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at,folder_path) VALUES('a','Generated meeting','2020-01-01','2020-01-01',?)").bind(dir.to_str().unwrap()).execute(&f.pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO transcripts(id,meeting_id,transcript,timestamp,speaker,audio_start_time) VALUES('line','a','Исправленная speech API','00:01','You',1.25); INSERT INTO meeting_notes(meeting_id,notes_markdown,revision,created_at,updated_at) VALUES('a','Моя Notes мысль',7,'now','now'); INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result,user_edited) VALUES('a','completed','now','now','Generated summary',1); INSERT INTO transcript_versions(id,meeting_id,kind,status,created_at,segments_json,summary_json) VALUES('corrected','a','corrections','ready','now','[]','Archived summary'); INSERT INTO transcript_selection VALUES('a','corrected',9); INSERT INTO action_items(id,meeting_id,text,source,edited,transcript_id,audio_time,created_at,updated_at) VALUES('task','a','Preserved user task','user',1,'line',1.25,'now','now'); INSERT INTO post_call_jobs(meeting_id,status,stage,epoch,error) VALUES('a','complete','summary',4,'retained diagnostic'); INSERT INTO post_call_results(meeting_id,version_id,transcript_revision,notes_revision,result_json) VALUES('a','corrected',9,7,'{\"generated\":true}');").execute(&f.pool).await.unwrap();
    let before = retained_dump(&f.pool).await;
    let plans = Plans::default();
    let plan = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    execute(&f.pool, &plans, &plan.token).await.unwrap();
    assert_eq!(retained_dump(&f.pool).await, before);
    let snapshot = crate::repositories::library::export_snapshot(&f.pool, "a", 7)
        .await
        .unwrap();
    assert!(!snapshot.audio_available);
    assert_eq!(snapshot.transcripts[0].text, "Исправленная speech API");
    assert_eq!(snapshot.transcripts[0].audio_start_time, Some(1.25));
    assert_eq!(snapshot.notes.markdown, "Моя Notes мысль");
    assert_eq!(snapshot.summary.as_deref(), Some("Generated summary"));
    assert_eq!(snapshot.actions[0].text, "Preserved user task");
    assert_eq!(
        crate::repositories::library::search(&f.pool, "исправленная", 200, true)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        crate::repositories::library::search(&f.pool, "мысль", 200, true)
            .await
            .unwrap()
            .len(),
        1
    );
}
#[tokio::test]
async fn queue_transition_committing_while_cleanup_waits_is_seen_before_unlink() {
    let f = fixture().await;
    let dir = meeting(&f, "a").await;
    let plans = std::sync::Arc::new(Plans::default());
    let p = prepare(&f.pool, &plans, vec!["a".into()]).await.unwrap();
    let mut claim = f.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO post_call_jobs VALUES('a','running','local')")
        .execute(&mut *claim)
        .await
        .unwrap();
    let pool = f.pool.clone();
    let owned = plans.clone();
    let deletion = tokio::spawn(async move { execute(&pool, &owned, &p.token).await });
    tokio::task::yield_now().await;
    claim.commit().await.unwrap();
    let r = deletion.await.unwrap().unwrap();
    assert_eq!(r.removed_bytes, 0);
    assert!(dir.join("audio.mp4").exists());
}
#[tokio::test]
async fn abandoned_recovery_waiter_cannot_release_a_blocking_readers_audio_ownership() {
    use std::sync::{Arc, Barrier};
    let started = Arc::new(Barrier::new(2));
    let finish = Arc::new(Barrier::new(2));
    *crate::recovery_reader::BARRIERS.lock().unwrap() = Some((started.clone(), finish.clone()));
    let waiter = tokio::spawn(async {
        let _ =
            crate::recovery_reader::recover_audio_from_checkpoints("generated-only".into(), 48000)
                .await;
    });
    tokio::task::spawn_blocking(move || {
        started.wait();
    })
    .await
    .unwrap();
    waiter.abort();
    let _ = waiter.await;
    let retained = crate::diarization::try_operation_guard().is_err();
    tokio::task::spawn_blocking(move || {
        finish.wait();
    })
    .await
    .unwrap();
    let _drained = crate::diarization::operation_guard().await;
    assert!(
        retained,
        "blocking file reader must own admission after IPC waiter is abandoned"
    );
    *crate::recovery_reader::BARRIERS.lock().unwrap() = None;
}
