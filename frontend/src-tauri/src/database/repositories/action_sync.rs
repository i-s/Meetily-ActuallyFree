//! Action extraction is a projection of one committed summary and transcript.
//! Capture ownership before JS extraction; commit tasks/anchors and trimmed
//! summary together, or do nothing if any writer has superseded that context.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use super::action_item::{ActionItemDraft, ActionItemView, ActionItemsRepository};
use crate::{api::TranscriptSegment, database::transcript_versions};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSyncGuard {
    pub meeting_id: String,
    pub version_id: String,
    pub revision: i64,
    pub summary_token: String,
    pub summary: Value,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionSyncContext {
    #[serde(flatten)]
    pub guard: ActionSyncGuard,
    pub transcripts: Vec<TranscriptSegment>,
}

// Match api_get_summary/parseSummaryData, including older double-encoded rows.
fn summary_data(raw: &str) -> Value {
    let data = serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.into()));
    match data {
        Value::String(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(object)) => Value::Object(object),
            _ => serde_json::json!({"markdown":text}),
        },
        other => other,
    }
}
fn terminal(status: &str) -> bool {
    matches!(status.to_ascii_lowercase().as_str(), "completed" | "failed" | "cancelled")
}

pub async fn context(pool: &SqlitePool, meeting: &str) -> Result<Option<ActionSyncContext>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Serialize initialization and ownership capture with selection and all SQL
    // writers. A JS promise is not a lock across WebViews or native jobs.
    let exists = sqlx::query("UPDATE meetings SET id=id WHERE id=?").bind(meeting)
        .execute(&mut *tx).await.map_err(|e| e.to_string())?.rows_affected();
    if exists != 1 { return Ok(None); }
    let row: Option<(String,String,String)> = sqlx::query_as("SELECT action_sync_token,result,status FROM summary_processes WHERE meeting_id=? AND result IS NOT NULL")
        .bind(meeting).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
    let Some((summary_token, raw, status)) = row else { return Ok(None); };
    let summary = summary_data(&raw);
    if !terminal(&status) { return Ok(None); }
    transcript_versions::ensure(&mut tx, meeting).await?;
    let (version_id, revision): (String,i64) = sqlx::query_as("SELECT version_id,revision FROM transcript_selection WHERE meeting_id=?")
        .bind(meeting).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
    let transcripts = transcript_versions::current(&mut tx, meeting).await?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(Some(ActionSyncContext { guard: ActionSyncGuard { meeting_id:meeting.into(),version_id,revision,summary_token,summary }, transcripts }))
}

pub async fn commit(pool: &SqlitePool, context: &ActionSyncGuard, drafts: Vec<ActionItemDraft>, source_fingerprint: &str, summary: Option<Value>) -> Result<Option<Vec<ActionItemView>>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE meetings SET id=id WHERE id=?").bind(&context.meeting_id)
        .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    let row: Option<(String,i64,String,String,String)> = sqlx::query_as("SELECT t.version_id,t.revision,s.action_sync_token,s.result,s.status FROM transcript_selection t JOIN summary_processes s ON s.meeting_id=t.meeting_id WHERE t.meeting_id=? AND s.result IS NOT NULL")
        .bind(&context.meeting_id).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
    let Some((version,revision,token,raw,status)) = row else { return Ok(None); };
    if version != context.version_id || revision != context.revision || token != context.summary_token || summary_data(&raw) != context.summary || !terminal(&status) {
        return Ok(None);
    }
    // Never attach a draft to another meeting or an obsolete projection. JS
    // locates against the captured rows; validate its anchors inside this lock.
    for draft in &drafts {
        if let Some(id) = &draft.transcript_id {
            let time: Option<Option<f64>> = sqlx::query_scalar("SELECT audio_start_time FROM transcripts WHERE meeting_id=? AND id=?")
                .bind(&context.meeting_id).bind(id).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
            if time.is_none() || time.flatten().unwrap_or(0.0) != draft.audio_time.unwrap_or(0.0) {
                return Err("Action anchor no longer belongs to the selected transcript".into());
            }
        } else if draft.audio_time.is_some() {
            return Err("Action time requires a selected transcript anchor".into());
        }
    }
    if summary.is_some() {
        ActionItemsRepository::sync_ai_in_transaction(&mut tx, &context.meeting_id, drafts, source_fingerprint).await.map_err(|e| e.to_string())?;
    } else {
        // A section-free backfill is just a read marker. In particular, reading
        // an already-trimmed summary must not erase the tasks it produced.
        if !drafts.is_empty() { return Err("Action drafts require a summary extraction".into()); }
        sqlx::query("UPDATE meetings SET action_items_source=? WHERE id=?").bind(source_fingerprint).bind(&context.meeting_id)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    if let Some(summary) = summary {
        sqlx::query("UPDATE summary_processes SET result=?,updated_at=? WHERE meeting_id=?")
            .bind(summary.to_string()).bind(chrono::Utc::now().to_rfc3339()).bind(&context.meeting_id)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
        sqlx::query("UPDATE meetings SET updated_at=? WHERE id=?").bind(chrono::Utc::now().to_rfc3339()).bind(&context.meeting_id)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    let items = ActionItemsRepository::list_in_transaction(&mut tx, &context.meeting_id).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(Some(items))
}

#[tauri::command]
pub async fn api_action_sync_context(state: tauri::State<'_, crate::state::AppState>, meeting_id: String) -> Result<Option<ActionSyncContext>, String> {
    context(state.db_manager.pool(), &meeting_id).await
}
#[tauri::command]
pub async fn api_commit_action_sync(state: tauri::State<'_, crate::state::AppState>, context: ActionSyncGuard, items: Vec<ActionItemDraft>, source_fingerprint: String, summary: Option<Value>) -> Result<Option<Vec<ActionItemView>>, String> {
    commit(state.db_manager.pool(), &context, items, &source_fingerprint, summary).await
}
