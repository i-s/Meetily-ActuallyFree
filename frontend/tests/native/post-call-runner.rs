use crate::{runner, store, summary::Snapshot};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;
async fn pool() -> SqlitePool {
    let p = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY,folder_path TEXT); CREATE TABLE transcript_selection(meeting_id TEXT,version_id TEXT,revision INTEGER); CREATE TABLE meeting_notes(meeting_id TEXT,revision INTEGER); INSERT INTO meetings VALUES('a','/synthetic/a'),('b','/synthetic/b'); INSERT INTO transcript_selection VALUES('a','live-a',3),('b','live-b',0);").execute(&p).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../src-tauri/migrations/20261118001500_post_call_queue.sql"
    ))
    .execute(&p)
    .await
    .unwrap();
    let mut tx = p.begin().await.unwrap();
    store::enqueue(&mut tx, "a").await.unwrap();
    store::enqueue(&mut tx, "b").await.unwrap();
    tx.commit().await.unwrap();
    p
}
fn snapshot(id: &str) -> Snapshot {
    Snapshot {
        revision: store::Revision {
            meeting_id: id.into(),
            version_id: format!("live-{id}"),
            transcript_revision: if id == "a" { 3 } else { 0 },
            notes_revision: 0,
        },
        lines: vec![],
        notes: String::new(),
    }
}
const EMPTY: &str = r#"{"summary":[],"decisions":[],"tasks":[],"open_questions":[],"uncertainty":["No supported conclusions"]}"#;
#[tokio::test]
async fn two_calls_cli_failure_retry_and_restart_keep_local_results_once() {
    let p = pool().await;
    let a = store::claim(&p).await.unwrap().unwrap();
    let token = CancellationToken::new();
    runner::execute(
        &p,
        &a,
        &token,
        |_| async { Ok("post-call-refined:a".into()) },
        || async { Ok(snapshot("a")) },
        |_, _| async { Err("Offline/quota".into()) },
    )
    .await
    .unwrap_err();
    let row = &store::list(&p).await.unwrap()[0];
    assert_eq!(row.stage, "summary");
    assert_eq!(row.status, "waiting");
    assert_eq!(row.candidate_id.as_deref(), Some("post-call-refined:a"));
    let b = store::claim(&p).await.unwrap().unwrap();
    assert_eq!(b.meeting_id, "b");
    runner::execute(
        &p,
        &b,
        &CancellationToken::new(),
        |_| async { Ok("post-call-refined:b".into()) },
        || async { Ok(snapshot("b")) },
        |_, _| async { Ok(EMPTY.into()) },
    )
    .await
    .unwrap();
    store::control(&p, "a", "retry").await.unwrap();
    store::recover(&p).await.unwrap();
    let retry = store::claim(&p).await.unwrap().unwrap();
    runner::execute(
        &p,
        &retry,
        &CancellationToken::new(),
        |_| async { panic!("completed ASR must not rerun") },
        || async { Ok(snapshot("a")) },
        |_, _| async { Ok(EMPTY.into()) },
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM post_call_results")
            .fetch_one(&p)
            .await
            .unwrap(),
        2
    );
    assert!(store::claim(&p).await.unwrap().is_none());
}
#[tokio::test]
async fn interruption_waits_for_owned_local_drain_and_keeps_job_queued() {
    let p = pool().await;
    let j = store::claim(&p).await.unwrap().unwrap();
    let token = CancellationToken::new();
    let cancel = token.clone();
    runner::execute(
        &p,
        &j,
        &token,
        |owned| async move {
            cancel.cancel();
            owned.cancelled().await;
            Ok("post-call-refined:a".into())
        },
        || async { panic!("summary cannot start after interruption") },
        |_, _| async { panic!("CLI cannot start after interruption") },
    )
    .await
    .unwrap_err();
    let row = &store::list(&p).await.unwrap()[0];
    assert_eq!(row.status, "queued");
    assert_eq!(row.candidate_id.as_deref(), Some("post-call-refined:a"));
    assert_eq!(row.result_json, None);
}
#[tokio::test]
async fn notes_edit_during_cli_rejects_result_but_preserves_previous_publication() {
    let p = pool().await;
    let j = store::claim(&p).await.unwrap().unwrap();
    runner::execute(
        &p,
        &j,
        &CancellationToken::new(),
        |_| async { Ok("post-call-refined:a".into()) },
        || async { Ok(snapshot("a")) },
        |_, _| async {
            sqlx::query("INSERT INTO meeting_notes VALUES('a',1)")
                .execute(&p)
                .await
                .unwrap();
            Ok(EMPTY.into())
        },
    )
    .await
    .unwrap_err();
    let row = &store::list(&p).await.unwrap()[0];
    assert_eq!(row.status, "waiting");
    assert!(row.result_json.is_none());
    assert_eq!(row.stage, "summary");
}
