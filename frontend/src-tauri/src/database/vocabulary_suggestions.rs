//! Read-only, bounded input for local spelling suggestions. Keep this separate
//! from #14's complete selected snapshot and its lazy initialization contract.
use serde::Serialize;
use sqlx::SqlitePool;

const MAX_ROWS: usize = 2000;
const MAX_CHARS: usize = 100_000; // Unicode scalar values (SQLite substr units).

#[derive(Serialize)]
pub struct SuggestionSegment {
    pub id: String,
    pub text: String,
    pub audio_start_time: Option<f64>,
    pub text_truncated: bool,
}

#[derive(Serialize)]
pub struct SuggestionContext {
    pub version_id: String,
    pub revision: i64,
    pub segments: Vec<SuggestionSegment>,
    pub truncated: bool,
}

pub async fn context(pool: &SqlitePool, meeting: &str) -> Result<SuggestionContext, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let (version_id, revision): (String, i64) = sqlx::query_as(
        "SELECT version_id, revision FROM transcript_selection WHERE meeting_id = ?",
    ).bind(meeting).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
    // Fetch only identities first. Neither settings_json/source_map nor entire
    // transcript strings enter the row list, even for a pathological long line.
    let rows: Vec<(String, Option<f64>)> = sqlx::query_as(
        "SELECT id, audio_start_time FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time, timestamp, id LIMIT ?",
    ).bind(meeting).bind((MAX_ROWS + 1) as i64).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    let mut truncated = rows.len() > MAX_ROWS;
    let mut remaining = MAX_CHARS;
    let mut segments = Vec::new();
    for (id, audio_start_time) in rows.into_iter().take(MAX_ROWS) {
        if remaining == 0 { truncated = true; break; }
        // The aggregate Rust/IPC text is bounded before materialization. One
        // lookahead scalar detects a cut token without loading the original row.
        let raw: String = sqlx::query_scalar("SELECT substr(transcript, 1, ?) FROM transcripts WHERE meeting_id = ? AND id = ?")
            .bind((remaining + 1) as i64).bind(meeting).bind(&id)
            .fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        let (text, text_truncated) = bounded_text(&raw, remaining);
        remaining = remaining.saturating_sub(raw.chars().count());
        truncated |= text_truncated;
        segments.push(SuggestionSegment { id, text, audio_start_time, text_truncated });
        if text_truncated { break; }
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(SuggestionContext { version_id, revision, segments, truncated })
}

/// SQL callers fetch at most limit+1 Unicode scalars. Remove an incomplete
/// trailing token, rather than allowing its prefix to become a spelling proof.
pub(crate) fn bounded_text(raw: &str, limit: usize) -> (String, bool) {
    let mut chars = raw.chars();
    let text: String = chars.by_ref().take(limit).collect();
    let truncated = chars.next().is_some();
    let text = if truncated {
        text.trim_end_matches(|c: char| c.is_alphanumeric() || c == '-' || c == '_').to_string()
    } else { text };
    (text, truncated)
}
