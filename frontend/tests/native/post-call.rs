use crate::store;
use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY, folder_path TEXT); CREATE TABLE transcript_selection(meeting_id TEXT PRIMARY KEY, version_id TEXT, revision INTEGER); CREATE TABLE meeting_notes(meeting_id TEXT PRIMARY KEY, notes_markdown TEXT, revision INTEGER); CREATE TABLE transcript_versions(id TEXT PRIMARY KEY, meeting_id TEXT, status TEXT, segments_json TEXT);
    INSERT INTO meetings VALUES('a','/synthetic/a'),('b','/synthetic/b'); INSERT INTO transcript_selection VALUES('a','live-a',3),('b','live-b',0); INSERT INTO meeting_notes VALUES('a','My idea, not agreed',7);").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../../src-tauri/migrations/20261118001500_post_call_queue.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    pool
}
async fn enqueue(pool: &SqlitePool, id: &str) {
    let mut tx = pool.begin().await.unwrap();
    store::enqueue(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn enqueue_is_transactional_idempotent_and_two_calls_are_independent() {
    let p = pool().await;
    {
        let mut tx = p.begin().await.unwrap();
        store::enqueue(&mut tx, "a").await.unwrap();
        tx.rollback().await.unwrap();
    }
    assert!(store::list(&p).await.unwrap().is_empty());
    enqueue(&p, "a").await;
    enqueue(&p, "a").await;
    enqueue(&p, "b").await;
    assert_eq!(store::list(&p).await.unwrap().len(), 2);
    let first = store::claim(&p).await.unwrap().unwrap();
    assert_eq!(first.meeting_id, "a");
    assert!(store::claim(&p).await.unwrap().is_none());
    store::advance(&p, &first, "summary").await.unwrap();
    store::release(&p, &first, "waiting", Some("CLI unavailable"))
        .await
        .unwrap();
    assert_eq!(store::claim(&p).await.unwrap().unwrap().meeting_id, "b");
}
#[tokio::test]
async fn restart_and_defer_fence_late_completion_without_losing_stage() {
    let p = pool().await;
    enqueue(&p, "a").await;
    let old = store::claim(&p).await.unwrap().unwrap();
    store::advance(&p, &old, "summary").await.unwrap();
    store::recover(&p).await.unwrap();
    let new = store::claim(&p).await.unwrap().unwrap();
    assert!(new.epoch > old.epoch);
    assert_eq!(new.stage, "summary");
    assert!(store::advance(&p, &old, "complete").await.is_err());
    store::control(&p, "a", "defer").await.unwrap();
    assert!(store::release(&p, &new, "queued", None).await.is_err());
    assert!(store::claim(&p).await.unwrap().is_none());
    store::recover(&p).await.unwrap();
    assert!(store::claim(&p).await.unwrap().is_none());
    store::control(&p, "a", "resume").await.unwrap();
    assert_eq!(store::claim(&p).await.unwrap().unwrap().stage, "summary");
}
#[tokio::test]
async fn publication_is_atomic_idempotent_and_checks_meeting_version_transcript_and_notes() {
    let p = pool().await;
    enqueue(&p, "a").await;
    let job = store::claim(&p).await.unwrap().unwrap();
    let tuple = store::Revision {
        meeting_id: "a".into(),
        version_id: "live-a".into(),
        transcript_revision: 3,
        notes_revision: 7,
    };
    for invalid in [
        store::Revision {
            meeting_id: "b".into(),
            ..tuple.clone()
        },
        store::Revision {
            version_id: "other".into(),
            ..tuple.clone()
        },
        store::Revision {
            transcript_revision: 2,
            ..tuple.clone()
        },
        store::Revision {
            notes_revision: 6,
            ..tuple.clone()
        },
    ] {
        assert!(store::publish(&p, &job, &invalid, "{\"summary\":[]}")
            .await
            .is_err());
    }
    assert_eq!(store::list(&p).await.unwrap()[0].result_json, None);
    store::publish(&p, &job, &tuple, "{\"summary\":[]}")
        .await
        .unwrap();
    assert!(store::publish(&p, &job, &tuple, "changed").await.is_err());
    let row = &store::list(&p).await.unwrap()[0];
    assert_eq!(row.status, "complete");
    assert_eq!(row.result_json.as_deref(), Some("{\"summary\":[]}"));
    enqueue(&p, "a").await;
    assert!(store::claim(&p).await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM post_call_results")
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let notes: String =
        sqlx::query_scalar("SELECT notes_markdown FROM meeting_notes WHERE meeting_id='a'")
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(notes, "My idea, not agreed");
}
#[tokio::test]
async fn cli_retry_preserves_local_stage_and_cancel_never_deletes_versions_or_notes() {
    let p = pool().await;
    enqueue(&p, "a").await;
    let j = store::claim(&p).await.unwrap().unwrap();
    store::advance(&p, &j, "summary").await.unwrap();
    store::release(&p, &j, "waiting", Some("No network or quota; retry later"))
        .await
        .unwrap();
    assert!(store::claim(&p).await.unwrap().is_none());
    store::control(&p, "a", "retry").await.unwrap();
    let retry = store::claim(&p).await.unwrap().unwrap();
    assert_eq!(retry.stage, "summary");
    store::control(&p, "a", "cancel").await.unwrap();
    assert!(store::publish(
        &p,
        &retry,
        &store::Revision {
            meeting_id: "a".into(),
            version_id: "live-a".into(),
            transcript_revision: 3,
            notes_revision: 7
        },
        "{}"
    )
    .await
    .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM meeting_notes")
            .fetch_one(&p)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn refresh_retains_historical_artifacts_and_exposes_stale_results() {
    let p = pool().await;
    enqueue(&p, "a").await;
    let job = store::claim(&p).await.unwrap().unwrap();
    let revision = store::Revision {
        meeting_id: "a".into(),
        version_id: "live-a".into(),
        transcript_revision: 3,
        notes_revision: 7,
    };
    store::publish(&p, &job, &revision, "first").await.unwrap();
    assert!(store::list(&p).await.unwrap()[0].result_current);
    sqlx::query("UPDATE meeting_notes SET revision=8 WHERE meeting_id='a'")
        .execute(&p)
        .await
        .unwrap();
    assert!(!store::list(&p).await.unwrap()[0].result_current);
    store::control(&p, "a", "refresh").await.unwrap();
    let refreshed = store::claim(&p).await.unwrap().unwrap();
    store::publish(
        &p,
        &refreshed,
        &store::Revision {
            notes_revision: 8,
            ..revision
        },
        "second",
    )
    .await
    .unwrap();
    let history = store::results(&p, "a").await.unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].result_json, "second");
    assert_eq!(history[1].result_json, "first");
    assert!(store::list(&p).await.unwrap()[0].result_current);
    assert!(store::results(&p, "b").await.unwrap().is_empty());
}

#[tokio::test]
async fn durable_queue_reopen_recovers_owned_stage_and_result_without_duplicates() {
    let dir=tempfile::tempdir().unwrap();
    let options=sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("queue.sqlite")).create_if_missing(true).foreign_keys(true);
    let p=SqlitePool::connect_with(options.clone()).await.unwrap();
    sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY,folder_path TEXT); CREATE TABLE transcript_selection(meeting_id TEXT,version_id TEXT,revision INTEGER); CREATE TABLE meeting_notes(meeting_id TEXT,revision INTEGER); INSERT INTO meetings VALUES('a','/synthetic/a'); INSERT INTO transcript_selection VALUES('a','live-a',3);").execute(&p).await.unwrap();
    sqlx::raw_sql(include_str!("../../src-tauri/migrations/20261118001500_post_call_queue.sql")).execute(&p).await.unwrap();
    enqueue(&p,"a").await;let old=store::claim(&p).await.unwrap().unwrap();store::candidate(&p,&old,"post-call-refined:a").await.unwrap();store::advance(&p,&old,"summary").await.unwrap();
    p.close().await;
    let reopened=SqlitePool::connect_with(options).await.unwrap();store::recover(&reopened).await.unwrap();let resumed=store::claim(&reopened).await.unwrap().unwrap();
    assert_eq!(resumed.stage,"summary");assert_eq!(resumed.candidate_id.as_deref(),Some("post-call-refined:a"));assert!(resumed.epoch>old.epoch);
    store::publish(&reopened,&resumed,&store::Revision{meeting_id:"a".into(),version_id:"live-a".into(),transcript_revision:3,notes_revision:0},"retained").await.unwrap();
    store::recover(&reopened).await.unwrap();enqueue(&reopened,"a").await;assert!(store::claim(&reopened).await.unwrap().is_none());assert_eq!(store::results(&reopened,"a").await.unwrap()[0].result_json,"retained");
}
