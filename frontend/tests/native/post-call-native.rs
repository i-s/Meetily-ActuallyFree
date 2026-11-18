//! Production scheduler/queue/refinement bodies + real SQLite/subprocess adapter.
//! Model inference, recorder/device transport and preferences are synthetic.
use crate::{
    post_call,
    state::{AppState, DbManager},
    AppHandle, State,
};
use sqlx::SqlitePool;
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
async fn migrate(p: &SqlitePool) {
    let folder = std::path::Path::new(file!())
        .parent()
        .unwrap()
        .join("../../src-tauri/migrations");
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    files.sort();
    for file in files {
        sqlx::raw_sql(&std::fs::read_to_string(file).unwrap())
            .execute(p)
            .await
            .unwrap();
    }
}
async fn until(p: &SqlitePool, meeting: &str, status: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if post_call::store::list(p)
                .await
                .unwrap()
                .iter()
                .any(|j| j.meeting_id == meeting && j.status == status)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("job {meeting} never reached {status}"));
}
#[cfg(unix)]
#[tokio::test]
async fn native_queue_two_saved_meetings_fake_cli_failure_retry_and_revision_fencing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    *crate::ROOT.lock().unwrap() = Some(dir.path().to_owned());
    let binary = dir.path().join("fake codex");
    std::fs::write(&binary,r#"#!/usr/bin/python3
import sys,json
from pathlib import Path
root=Path(__file__).parent
if sys.argv[1:]==['--version']: print('codex-cli 0.167.2')
elif sys.argv[1:]==['exec','--help']: print('--ephemeral --ignore-user-config --ignore-rules')
elif sys.argv[1:]==['login','status']: print('Logged in using ChatGPT')
else:
 prompt=sys.stdin.read()
 with root.joinpath('sent.txt').open('a') as f: f.write(prompt+'\n')
 if root.joinpath('offline').exists():
  sys.stderr.write('synthetic network unavailable');sys.exit(1)
 answer={'summary':[],'decisions':[],'tasks':[],'open_questions':[],'uncertainty':['No supported agreements']}
 print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':json.dumps(answer)}}))
 print(json.dumps({'type':'turn.completed'}))
"#).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.path().join("offline"), b"synthetic").unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(dir.path().join("synthetic.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let p = SqlitePool::connect_with(options).await.unwrap();
    migrate(&p).await;
    for id in ["a", "b"] {
        sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at,folder_path) VALUES(?,?,datetime('now'),datetime('now'),?)").bind(id).bind(format!("Synthetic {id}")).bind(format!("/synthetic/{id}")).execute(&p).await.unwrap();
        sqlx::query("INSERT INTO transcripts(id,meeting_id,transcript,timestamp,audio_start_time,audio_end_time,duration,speaker) VALUES(?,?,?,datetime('now'),1,2,1,'Microphone')").bind(format!("live-{id}")).bind(id).bind(format!("Selected original {id}")).execute(&p).await.unwrap();
        let mut tx = p.begin().await.unwrap();
        crate::database::transcript_versions::ensure(&mut tx, id)
            .await
            .unwrap();
        post_call::store::enqueue(&mut tx, id).await.unwrap();
        tx.commit().await.unwrap();
    }
    sqlx::query("UPDATE transcript_settings SET provider='whisper',model='synthetic',postCallProvider='whisper',postCallModel='synthetic' WHERE id='1'").execute(&p).await.unwrap();
    // Some historical schemas have no initial settings row.
    if sqlx::query_scalar::<_, i64>("SELECT count(*) FROM transcript_settings WHERE id='1'")
        .fetch_one(&p)
        .await
        .unwrap()
        == 0
    {
        sqlx::query("INSERT INTO transcript_settings(id,provider,model,postCallProvider,postCallModel) VALUES('1','whisper','synthetic','whisper','synthetic')").execute(&p).await.unwrap();
    }
    let state = Arc::new(AppState {
        db_manager: DbManager(p.clone()),
    });
    *crate::STATE.lock().unwrap() = Some(state.clone());
    let app = AppHandle::<()>(std::marker::PhantomData);
    crate::audio::recording_commands::LIVE.store(true, Ordering::SeqCst);
    post_call::worker::start(app.clone());
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        crate::audio::retranscription::ASR_CALLS.load(Ordering::SeqCst),
        0
    );
    crate::audio::recording_commands::LIVE.store(false, Ordering::SeqCst);
    until(&p, "a", "waiting").await;
    until(&p, "b", "waiting").await;
    for id in ["a", "b"] {
        let row = post_call::store::list(&p)
            .await
            .unwrap()
            .into_iter()
            .find(|j| j.meeting_id == id)
            .unwrap();
        assert_eq!(row.stage, "summary");
        assert!(row.error.unwrap().contains("network"));
        let selected = crate::database::transcript_versions::selected(&p, id)
            .await
            .unwrap();
        assert_eq!(selected.segments[0].text, format!("Selected original {id}"));
        let candidate: serde_json::Value = serde_json::from_str(
            &sqlx::query_scalar::<_, String>(
                "SELECT settings_json FROM transcript_versions WHERE id=?",
            )
            .bind(format!("post-call-refined:{id}"))
            .fetch_one(&p)
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(candidate["recording_metadata"]["future_policy"], "opaque");
        assert_eq!(candidate["source_map"]["candidate-room"], "room");
    }
    assert_eq!(
        crate::audio::retranscription::ASR_CALLS.load(Ordering::SeqCst),
        2
    );
    std::fs::remove_file(dir.path().join("offline")).unwrap();
    post_call::post_call_control(State(&state), "a".into(), "retry".into())
        .await
        .unwrap();
    until(&p, "a", "complete").await;
    assert_eq!(
        crate::audio::retranscription::ASR_CALLS.load(Ordering::SeqCst),
        2
    );
    assert_eq!(post_call::store::results(&p, "a").await.unwrap().len(), 1);
    post_call::post_call_control(State(&state), "a".into(), "refresh".into())
        .await
        .unwrap();
    until(&p, "a", "complete").await;
    assert_eq!(post_call::store::results(&p, "a").await.unwrap().len(), 1);
    let sent = std::fs::read_to_string(dir.path().join("sent.txt")).unwrap();
    assert!(sent.contains("selected_transcript"));
    assert!(!sent.contains("Synthetic refined speech"));
    assert!(!sent.contains("recording:/"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM transcript_versions WHERE kind='refined'"
        )
        .fetch_one(&p)
        .await
        .unwrap(),
        2
    );
}

#[tokio::test]
async fn native_queue_cancelled_before_operation_admission_does_not_wait_for_manual_work() {
    let dir=tempfile::tempdir().unwrap();
    let options=sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("admission.sqlite")).create_if_missing(true);
    let p=SqlitePool::connect_with(options).await.unwrap();migrate(&p).await;
    sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at,folder_path) VALUES('admission','Synthetic',datetime('now'),datetime('now'),'/synthetic/admission')").execute(&p).await.unwrap();
    *crate::STATE.lock().unwrap()=Some(Arc::new(AppState{db_manager:DbManager(p)}));
    let manual=crate::diarization::operation_guard().await;
    let token=tokio_util::sync::CancellationToken::new();let cancel=token.clone();
    let job=tokio::spawn(async move{crate::audio::retranscription::run_queued_refinement(AppHandle::<()>(std::marker::PhantomData),"admission".into(),&token).await});
    tokio::time::timeout(Duration::from_secs(5),async {while !crate::audio::retranscription::is_retranscription_in_progress(){tokio::task::yield_now().await;}}).await.unwrap();
    cancel.cancel();
    let result=tokio::time::timeout(Duration::from_secs(1),job).await;
    drop(manual);
    assert!(result.is_ok(),"an unstarted background inference blocked recorder admission behind unrelated manual work");
    assert!(result.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn native_queue_invalid_asr_checkpoint_is_not_left_running() {
    let p=SqlitePool::connect(":memory:").await.unwrap();migrate(&p).await;
    sqlx::query("INSERT INTO meetings(id,title,created_at,updated_at,folder_path) VALUES('invalid','Synthetic',datetime('now'),datetime('now'),'/synthetic/invalid')").execute(&p).await.unwrap();
    let settings=serde_json::json!({"provider":"whisper","model":"synthetic","diarization_engine":"nemotron"});
    let id=crate::database::transcript_versions::begin_automatic(&p,"invalid",&settings).await.unwrap();
    sqlx::query("UPDATE transcript_versions SET status='interrupted',segments_json='invalid json' WHERE id=?").bind(&id).execute(&p).await.unwrap();
    *crate::STATE.lock().unwrap()=Some(Arc::new(AppState{db_manager:DbManager(p.clone())}));
    assert!(crate::audio::retranscription::run_queued_refinement(AppHandle::<()>(std::marker::PhantomData),"invalid".into(),&tokio_util::sync::CancellationToken::new()).await.is_err());
    let status:String=sqlx::query_scalar("SELECT status FROM transcript_versions WHERE id=?").bind(id).fetch_one(&p).await.unwrap();assert_eq!(status,"failed");
}
