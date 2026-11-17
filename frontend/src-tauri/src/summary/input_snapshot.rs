//! The summary worker owns these immutable inputs. Frontend pagination and later
//! notes/transcript edits cannot change a run that has already been accepted.
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryInputSnapshot {
    pub captured_at: String,
    pub transcript: String,
    pub notes_markdown: Option<String>,
    pub notes_json: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryInputMetadata {
    pub captured_at: String,
    pub notes_markdown: Option<String>,
    pub notes_json: Option<String>,
}

impl SummaryInputSnapshot {
    /// Notes bypass transcript chunking and appear in full at the final report
    /// step. A provider context-limit failure is visible; we never trim notes.
    pub fn context_prompt(&self, custom_prompt: &str) -> String {
        let notes = self.notes_markdown.as_deref().filter(|value| !value.trim().is_empty())
            .or(self.notes_json.as_deref()).unwrap_or("");
        format!("{custom_prompt}\n\n<meeting_notes>\n{notes}\n</meeting_notes>")
    }

    pub fn metadata(self) -> SummaryInputMetadata {
        SummaryInputMetadata {
            captured_at: self.captured_at,
            notes_markdown: self.notes_markdown,
            notes_json: self.notes_json,
        }
    }
}

/// Snapshot both sources and initialize the run in one SQLite transaction. The
/// previous successful snapshot stays attached to its result until completion.
pub async fn capture_and_start(pool: &SqlitePool, meeting_id: &str) -> Result<SummaryInputSnapshot, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM meetings WHERE id = ?")
        .bind(meeting_id).fetch_optional(&mut *tx).await?;
    if exists.is_none() { return Err(sqlx::Error::RowNotFound); }
    let rows = sqlx::query_as::<_, (String, String, Option<f64>, Option<String>)>(
        "SELECT transcript, timestamp, audio_start_time, speaker FROM transcripts WHERE meeting_id = ? ORDER BY COALESCE(audio_start_time, 0), timestamp, id",
    ).bind(meeting_id).fetch_all(&mut *tx).await?;
    let transcript = rows.into_iter().map(|(text, timestamp, start, speaker)| {
        let timestamp = start.map(|seconds| {
            let seconds = seconds.max(0.0) as u64;
            format!("[{:02}:{:02}]", seconds / 60, seconds % 60)
        }).unwrap_or(timestamp);
        let speaker = speaker.filter(|value| !value.is_empty()).map(|value| format!("{value}: ")).unwrap_or_default();
        format!("{timestamp} {speaker}{text}")
    }).collect::<Vec<_>>().join("\n");
    let notes = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT notes_markdown, notes_json FROM meeting_notes WHERE meeting_id = ?",
    ).bind(meeting_id).fetch_optional(&mut *tx).await?;
    let (notes_markdown, notes_json) = notes.unwrap_or_default();
    let snapshot = SummaryInputSnapshot {captured_at: Utc::now().to_rfc3339(), transcript, notes_markdown, notes_json};
    if snapshot.transcript.trim().is_empty()
        && snapshot.notes_markdown.as_deref().unwrap_or("").trim().is_empty()
        && snapshot.notes_json.as_deref().unwrap_or("").trim().is_empty() {
        return Err(sqlx::Error::Protocol("No transcript or Meeting notes available for summary".into()));
    }
    let serialized = serde_json::to_string(&snapshot).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    sqlx::query(
        "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, start_time, pending_input_snapshot) \
         VALUES (?, 'PENDING', ?, ?, ?, ?) \
         ON CONFLICT(meeting_id) DO UPDATE SET status = 'PENDING', updated_at = excluded.updated_at, \
         start_time = excluded.start_time, result_backup = result, result_backup_timestamp = excluded.updated_at, \
         pending_input_snapshot = excluded.pending_input_snapshot, error = NULL",
    ).bind(meeting_id).bind(&snapshot.captured_at).bind(&snapshot.captured_at)
        .bind(&snapshot.captured_at).bind(serialized).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(snapshot)
}

pub async fn saved_snapshot(pool: &SqlitePool, meeting_id: &str) -> Result<Option<SummaryInputSnapshot>, sqlx::Error> {
    let raw: Option<Option<String>> = sqlx::query_scalar("SELECT input_snapshot FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting_id).fetch_optional(pool).await?;
    raw.flatten().map(|value| serde_json::from_str(&value).map_err(|error| sqlx::Error::Protocol(error.to_string()))).transpose()
}

#[cfg(test)]
#[path = "input_snapshot_tests.rs"]
mod tests;
