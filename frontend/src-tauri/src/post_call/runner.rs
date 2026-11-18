//! Transport-independent production orchestrator. Model/desktop boundaries are
//! injected, so crash/retry/cancellation policy runs unchanged in synthetic tests.
use super::{
    store::{self, Job},
    summary::{Snapshot, Summary},
};
use sqlx::SqlitePool;
use std::{future::Future, time::Duration};
use tokio_util::sync::CancellationToken;

async fn owned<T, F: Future<Output = Result<T, String>>>(
    pool: &SqlitePool,
    job: &Job,
    token: &CancellationToken,
    work: F,
) -> Result<T, String> {
    tokio::pin!(work);
    let mut watch = tokio::time::interval(Duration::from_millis(100));
    let result = loop {
        tokio::select! {
            result=&mut work=>break result,
            _=watch.tick()=> {
                // Defer/cancel can arrive through a different WebView. Cancel
                // only this attempt and KEEP awaiting it: dropping a model
                // future does not stop its spawn_blocking inference thread.
                if !store::current(pool,job).await.unwrap_or(false) {token.cancel();}
            }
        }
    };
    if token.is_cancelled() {
        return Err("Post-call processing interrupted; retained stage will resume after recording or on request".into());
    }
    if !store::current(pool, job).await? {
        return Err(
            "Post-call attempt is stale; result retained only in its owned candidate".into(),
        );
    }
    result
}

pub async fn execute<L, LF, S, SF, G, GF>(
    pool: &SqlitePool,
    job: &Job,
    token: &CancellationToken,
    local: L,
    snapshot: S,
    generate: G,
) -> Result<(), String>
where
    L: FnOnce(CancellationToken) -> LF,
    LF: Future<Output = Result<String, String>>,
    S: FnOnce() -> SF,
    SF: Future<Output = Result<Snapshot, String>>,
    G: FnOnce(String, CancellationToken) -> GF,
    GF: Future<Output = Result<String, String>>,
{
    let result=async {
        if token.is_cancelled() {return Err("Recording has priority".into());}
        if job.stage=="local" {
            let expected=format!("post-call-refined:{}",job.meeting_id);
            store::candidate(pool,job,&expected).await?;
            let candidate=owned(pool,job,token,local(token.clone())).await?;
            if candidate!=expected {return Err("Local refinement returned a candidate for another job".into());}
            store::advance(pool,job,"summary").await?;
        }
        let snapshot=owned(pool,job,token,snapshot()).await?;
        if snapshot.revision.meeting_id!=job.meeting_id {return Err("Snapshot belongs to another meeting".into());}
        // Explicit refresh of the same tuple is idempotent even after a lost
        // response. Never send another request when a valid artifact exists.
        let cached:Option<String>=sqlx::query_scalar("SELECT result_json FROM post_call_results WHERE meeting_id=? AND version_id=? AND transcript_revision=? AND notes_revision=?")
            .bind(&snapshot.revision.meeting_id).bind(&snapshot.revision.version_id).bind(snapshot.revision.transcript_revision).bind(snapshot.revision.notes_revision)
            .fetch_optional(pool).await.map_err(|e|e.to_string())?;
        let json=if let Some(cached)=cached {cached} else {
            let prompt=snapshot.prompt()?;
            let raw=owned(pool,job,token,generate(prompt,token.clone())).await?;
            let result=Summary::parse(&raw,&snapshot)?;
            serde_json::to_string(&result).map_err(|e|e.to_string())?
        };
        if token.is_cancelled(){return Err("Post-call summary cancelled before publication".into());}
        store::publish(pool,job,&snapshot.revision,&json).await
    }.await;
    if let Err(error) = &result {
        // A stale epoch means the user already deferred/cancelled; don't undo it.
        let status = if token.is_cancelled() {
            "queued"
        } else {
            "waiting"
        };
        let message: String = error.chars().take(2000).collect();
        let _ = store::release(pool, job, status, Some(&message)).await;
    }
    result
}
