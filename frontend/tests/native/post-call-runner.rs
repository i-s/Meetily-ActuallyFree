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
    crate::test_schema::queue_schema(&p).await;
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
        approved_vocabulary: crate::database::repositories::vocabulary::ApprovedGlobalVocabulary {
            terms: String::new(),
            revision: 0,
        },
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
        || async { Ok(snapshot("a")) },
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

#[tokio::test]
async fn ownership_watchdog_does_not_starve_a_work_future_holding_the_only_sqlite_connection() {
    let p = pool().await;
    let j = store::claim(&p).await.unwrap().unwrap();
    let token = CancellationToken::new();
    let run = runner::execute(
        &p,
        &j,
        &token,
        |_| async { Ok("post-call-refined:a".into()) },
        || async { Ok(snapshot("a")) },
        |_, _| async {
            let tx = p.begin().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            tx.commit().await.unwrap();
            Ok(EMPTY.into())
        },
    );
    assert!(tokio::time::timeout(std::time::Duration::from_secs(2), run)
        .await
        .expect("watchdog starved owned SQLite work")
        .is_ok());
}

#[tokio::test]
async fn changed_spelling_snapshot_cannot_hit_a_previous_guidance_cache_entry() {
    let p = pool().await;
    let calls = std::sync::atomic::AtomicUsize::new(0);
    for revision in [7, 8, 8] {
        if revision == 8 {
            store::control(&p, "a", "refresh").await.unwrap();
        }
        let j = store::claim(&p).await.unwrap().unwrap();
        let mut s = snapshot("a");
        s.approved_vocabulary.revision = revision;
        s.approved_vocabulary.terms = if revision == 7 {
            "OpenAPI".into()
        } else {
            String::new()
        };
        let counter = &calls;
        runner::execute(
            &p,
            &j,
            &CancellationToken::new(),
            |_| async { Ok("post-call-refined:a".into()) },
            || async { Ok(s) },
            |prompt, _| async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                assert!(prompt.contains(&format!("vocabulary:global:{revision}")));
                Ok(EMPTY.into())
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(store::results(&p, "a").await.unwrap().len(), 2);
}

#[tokio::test]
async fn speech_or_notes_changed_during_local_work_are_rejected_before_cli_dispatch() {
    let p = pool().await;
    let j = store::claim(&p).await.unwrap().unwrap();
    runner::execute(
        &p,
        &j,
        &CancellationToken::new(),
        |_| async {
            sqlx::query("INSERT INTO meeting_notes VALUES('a',1)")
                .execute(&p)
                .await
                .unwrap();
            Ok("post-call-refined:a".into())
        },
        || async { Ok(snapshot("a")) },
        |_, _| async { panic!("a known-stale snapshot must not consume a Codex request") },
    )
    .await
    .unwrap_err();
    let a = &store::list(&p).await.unwrap()[0];
    assert_eq!(a.status, "waiting");
    assert_eq!(a.stage, "summary");
    assert_eq!(a.candidate_id.as_deref(), Some("post-call-refined:a"));
}
