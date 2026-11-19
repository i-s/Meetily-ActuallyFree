//! SQLite is the durable owner. Epochs fence abandoned attempts; the process
//! scheduler is only an executor. No method writes transcript or Notes content.
#[path = "projection.rs"]
pub mod projection;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Job {
    pub meeting_id: String,
    pub status: String,
    pub stage: String,
    pub epoch: i64,
    pub vocabulary_revision: Option<i64>,
    pub candidate_id: Option<String>,
    pub error: Option<String>,
    pub result_json: Option<String>,
    pub result_version: Option<String>,
    pub result_transcript_revision: Option<i64>,
    pub result_notes_revision: Option<i64>,
    pub result_vocabulary_revision: Option<i64>,
    pub result_current: bool,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Artifact {
    pub version_id: String,
    pub transcript_revision: i64,
    pub notes_revision: i64,
    pub vocabulary_revision: i64,
    pub result_json: String,
    pub created_at: String,
}
pub async fn results(pool: &SqlitePool, meeting: &str) -> Result<Vec<Artifact>, String> {
    sqlx::query_as("SELECT version_id,transcript_revision,notes_revision,vocabulary_revision,result_json,created_at FROM post_call_results WHERE meeting_id=? ORDER BY rowid DESC")
        .bind(meeting).fetch_all(pool).await.map_err(|e|e.to_string())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Revision {
    pub meeting_id: String,
    pub version_id: String,
    pub transcript_revision: i64,
    pub notes_revision: i64,
}

pub async fn enqueue(tx: &mut Transaction<'_, Sqlite>, meeting: &str) -> Result<(), sqlx::Error> {
    // Only saved recordings, not arbitrary legacy imports without owned audio.
    sqlx::query("INSERT INTO post_call_jobs(meeting_id) SELECT id FROM meetings WHERE id = ? AND folder_path IS NOT NULL ON CONFLICT DO NOTHING")
        .bind(meeting).execute(&mut **tx).await?;
    Ok(())
}
const SELECT: &str = "SELECT j.meeting_id,j.status,j.stage,j.epoch,j.vocabulary_revision,j.candidate_id,j.error,r.result_json,r.version_id AS result_version,r.transcript_revision AS result_transcript_revision,r.notes_revision AS result_notes_revision,r.vocabulary_revision AS result_vocabulary_revision, CASE WHEN EXISTS(SELECT 1 FROM transcript_selection s WHERE s.meeting_id=j.meeting_id AND s.version_id=r.version_id AND s.revision=r.transcript_revision) AND COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=j.meeting_id),0)=r.notes_revision AND (j.vocabulary_revision IS NULL OR j.vocabulary_revision=r.vocabulary_revision) THEN 1 ELSE 0 END AS result_current FROM post_call_jobs j LEFT JOIN post_call_results r ON r.rowid = (SELECT rowid FROM post_call_results WHERE meeting_id=j.meeting_id ORDER BY rowid DESC LIMIT 1)";
pub async fn list(pool: &SqlitePool) -> Result<Vec<Job>, String> {
    sqlx::query_as(&format!("{SELECT} ORDER BY j.created_at,j.meeting_id"))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}
/// Called once per native database activation, not on a WebView reload.
pub async fn recover(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query("UPDATE post_call_jobs SET status='queued',epoch=epoch+1,error='App restarted; resuming retained stage' WHERE status='running'").execute(pool).await.map_err(|e|e.to_string())?;
    Ok(())
}
pub async fn claim(pool: &SqlitePool) -> Result<Option<Job>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let id:Option<String>=sqlx::query_scalar("UPDATE post_call_jobs SET status='running',epoch=epoch+1,error=NULL,vocabulary_revision=NULL,updated_at=datetime('now') WHERE meeting_id=(SELECT meeting_id FROM post_call_jobs WHERE status='queued' ORDER BY created_at,meeting_id LIMIT 1) AND NOT EXISTS(SELECT 1 FROM post_call_jobs WHERE status='running') RETURNING meeting_id")
        .fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
    let job = if let Some(id) = id {
        sqlx::query_as(&format!("{SELECT} WHERE j.meeting_id=?"))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
    } else {
        None
    };
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(job)
}
fn checked(changed: u64) -> Result<(), String> {
    if changed == 1 {
        Ok(())
    } else {
        Err("Post-call attempt is stale; nothing was published".into())
    }
}
pub async fn current(pool: &SqlitePool, job: &Job) -> Result<bool, String> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM post_call_jobs WHERE meeting_id=? AND epoch=? AND status='running')").bind(&job.meeting_id).bind(job.epoch).fetch_one(pool).await.map_err(|e|e.to_string())
}
pub async fn revision_current(pool: &SqlitePool, revision: &Revision) -> Result<bool, String> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transcript_selection WHERE meeting_id=? AND version_id=? AND revision=?) AND COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=?),0)=?")
        .bind(&revision.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(&revision.meeting_id).bind(revision.notes_revision)
        .fetch_one(pool).await.map_err(|e|e.to_string())
}
pub async fn bind_vocabulary(pool: &SqlitePool, job: &Job, revision: i64) -> Result<(), String> {
    if revision < 0 {
        return Err("Invalid approved vocabulary revision".into());
    }
    checked(sqlx::query("UPDATE post_call_jobs SET vocabulary_revision=? WHERE meeting_id=? AND epoch=? AND status='running' AND (vocabulary_revision IS NULL OR vocabulary_revision=?)")
        .bind(revision).bind(&job.meeting_id).bind(job.epoch).bind(revision).execute(pool).await.map_err(|e|e.to_string())?.rows_affected())
}
pub async fn advance(pool: &SqlitePool, job: &Job, stage: &str) -> Result<(), String> {
    checked(sqlx::query("UPDATE post_call_jobs SET stage=?,updated_at=datetime('now') WHERE meeting_id=? AND epoch=? AND status='running'").bind(stage).bind(&job.meeting_id).bind(job.epoch).execute(pool).await.map_err(|e|e.to_string())?.rows_affected())
}
pub async fn candidate(pool: &SqlitePool, job: &Job, id: &str) -> Result<(), String> {
    checked(sqlx::query("UPDATE post_call_jobs SET candidate_id=? WHERE meeting_id=? AND epoch=? AND status='running'").bind(id).bind(&job.meeting_id).bind(job.epoch).execute(pool).await.map_err(|e|e.to_string())?.rows_affected())
}
pub async fn release(
    pool: &SqlitePool,
    job: &Job,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    checked(sqlx::query("UPDATE post_call_jobs SET status=?,error=?,updated_at=datetime('now') WHERE meeting_id=? AND epoch=? AND status='running'").bind(status).bind(error).bind(&job.meeting_id).bind(job.epoch).execute(pool).await.map_err(|e|e.to_string())?.rows_affected())
}
/// Retain one exact claim's terminal intent until SQLite accepts it or the
/// claim is superseded. The caller must drop inference/resource admission first.
/// A zero-row UPDATE is success here: defer/cancel/new epochs own their state.
pub async fn reconcile_release(pool: &SqlitePool, job: &Job, status: &str, error: Option<&str>) {
    let mut delay = std::time::Duration::from_millis(500);
    loop {
        match sqlx::query("UPDATE post_call_jobs SET status=?,error=?,updated_at=datetime('now') WHERE meeting_id=? AND epoch=? AND status='running'")
            .bind(status).bind(error).bind(&job.meeting_id).bind(job.epoch).execute(pool).await {
            Ok(_) => return,
            Err(_) => {
                // Persistence failure is not lost ownership. Never rerun work
                // or reset other claims; keep this intent with bounded backoff.
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(std::time::Duration::from_secs(5));
            }
        }
    }
}
pub async fn control(pool: &SqlitePool, meeting: &str, action: &str) -> Result<(), String> {
    let status = match action {
        "defer" => "deferred",
        "cancel" => "cancelled",
        "resume" | "retry" | "refresh" => "queued",
        _ => return Err("Unknown post-call action".into()),
    };
    // Refresh is explicit: summarize the latest selected text/Notes, keeping old artifacts.
    let changed=sqlx::query("UPDATE post_call_jobs SET status=?,epoch=epoch+1,error=NULL,stage=CASE WHEN ?='refresh' THEN 'summary' ELSE stage END,updated_at=datetime('now') WHERE meeting_id=? AND (status!='complete' OR ?='refresh')")
        .bind(status).bind(action).bind(meeting).bind(action).execute(pool).await.map_err(|e|e.to_string())?.rows_affected();
    checked(changed)
}
/// The write lock and tuple check are in the publication transaction. Checking
/// in JS or before awaiting the CLI leaves a correction/Notes race.
pub async fn publish(
    pool: &SqlitePool,
    job: &Job,
    revision: &Revision,
    vocabulary_revision: i64,
    json: &str,
) -> Result<(), String> {
    if revision.meeting_id != job.meeting_id {
        return Err("Summary belongs to another meeting".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let changed=sqlx::query("UPDATE post_call_jobs SET status='complete',error=NULL,updated_at=datetime('now') WHERE meeting_id=? AND epoch=? AND status='running' AND vocabulary_revision=? AND EXISTS(SELECT 1 FROM transcript_selection s WHERE s.meeting_id=? AND s.version_id=? AND s.revision=?) AND COALESCE((SELECT revision FROM meeting_notes WHERE meeting_id=?),0)=?")
        .bind(&job.meeting_id).bind(job.epoch).bind(vocabulary_revision).bind(&revision.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(&revision.meeting_id).bind(revision.notes_revision).execute(&mut *tx).await.map_err(|e|e.to_string())?.rows_affected();
    checked(changed)?;
    sqlx::query("INSERT INTO post_call_results(meeting_id,version_id,transcript_revision,notes_revision,vocabulary_revision,result_json) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING")
        .bind(&revision.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(revision.notes_revision).bind(vocabulary_revision).bind(json).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    // ON CONFLICT retains the first immutable result. Project those exact
    // stored bytes, never a different late argument for the same artifact tuple.
    let stored:String=sqlx::query_scalar("SELECT result_json FROM post_call_results WHERE meeting_id=? AND version_id=? AND transcript_revision=? AND notes_revision=? AND vocabulary_revision=?")
        .bind(&revision.meeting_id).bind(&revision.version_id).bind(revision.transcript_revision).bind(revision.notes_revision).bind(vocabulary_revision).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    projection::apply(&mut tx,job,revision,vocabulary_revision,&stored).await?;
    tx.commit().await.map_err(|e| e.to_string())
}
