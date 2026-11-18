//! Durable local transcript snapshots. Only explicit selection writes canonical
//! rows; the revision CAS includes edits from every old/native writer via SQLite
//! triggers. No audio or recovery checkpoint ownership is transferred here.
use crate::api::TranscriptSegment;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

type Row = (
    String,
    String,
    String,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<String>,
);

/// Called only on native process startup/first database activation, never on a
/// WebView remount. No partial candidate becomes selected or ready.
pub async fn interrupt_running(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query("UPDATE transcript_versions SET status = 'interrupted', error = 'App restarted before refinement completed; previous selection retained' WHERE status = 'running'")
        .execute(pool).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn current(
    conn: &mut SqliteConnection,
    meeting: &str,
) -> Result<Vec<TranscriptSegment>, String> {
    let rows: Vec<Row> = sqlx::query_as("SELECT id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker FROM transcripts WHERE meeting_id = ? ORDER BY audio_start_time, timestamp, id")
        .bind(meeting).fetch_all(conn).await.map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(id, text, timestamp, audio_start_time, audio_end_time, duration, speaker)| {
                TranscriptSegment {
                    id,
                    text,
                    timestamp,
                    audio_start_time,
                    audio_end_time,
                    duration,
                    speaker,
                }
            },
        )
        .collect())
}

async fn speaker_links(conn: &mut SqliteConnection, meeting: &str) -> Result<String, String> {
    let links: Vec<(String, String)> =
        sqlx::query_as("SELECT person_id,speaker_label FROM person_speakers WHERE meeting_id = ?")
            .bind(meeting)
            .fetch_all(conn)
            .await
            .map_err(|e| e.to_string())?;
    serde_json::to_string(&links).map_err(|e| e.to_string())
}

pub async fn ensure(conn: &mut SqliteConnection, meeting: &str) -> Result<(), String> {
    let selected: Option<String> =
        sqlx::query_scalar("SELECT version_id FROM transcript_selection WHERE meeting_id = ?")
            .bind(meeting)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
    if selected.is_some() {
        return Ok(());
    }
    let segments = current(conn, meeting).await?;
    let id = Uuid::new_v4().to_string();
    let links = speaker_links(conn, meeting).await?;
    sqlx::query("INSERT INTO transcript_versions(id, meeting_id, kind, status, created_at, segments_json, speaker_links_json) VALUES (?, ?, 'live', 'ready', ?, ?, ?)")
        .bind(&id).bind(meeting).bind(chrono::Utc::now().to_rfc3339()).bind(serde_json::to_string(&segments).map_err(|e| e.to_string())?).bind(links)
        .execute(&mut *conn).await.map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO transcript_selection(meeting_id, version_id) VALUES (?, ?)")
        .bind(meeting)
        .bind(id)
        .execute(conn)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn begin(
    pool: &SqlitePool,
    meeting: &str,
    settings: &serde_json::Value,
) -> Result<String, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Take the write lock before initialization, including legacy meetings.
    sqlx::query("UPDATE meetings SET id = id WHERE id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    ensure(&mut tx, meeting).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO transcript_versions(id, meeting_id, kind, status, created_at, parent_id, settings_json) SELECT ?, ?, 'refined', 'running', ?, version_id, ? FROM transcript_selection WHERE meeting_id = ?")
        .bind(&id).bind(meeting).bind(chrono::Utc::now().to_rfc3339()).bind(settings.to_string()).bind(meeting)
        .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

/// Automatic queue retries own one candidate identity, including a crash between
/// candidate reservation and the queue's next checkpoint. Completed ASR JSON and
/// its original model/policy/source map survive speaker-stage interruption.
/// A ready snapshot is immutable and is never selected by this operation.
pub async fn begin_automatic(
    pool: &SqlitePool,
    meeting: &str,
    settings: &serde_json::Value,
) -> Result<String, String> {
    let id = format!("post-call-refined:{meeting}");
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE meetings SET id = id WHERE id = ?")
        .bind(meeting).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    ensure(&mut tx, meeting).await?;
    sqlx::query("INSERT INTO transcript_versions(id,meeting_id,kind,status,created_at,parent_id,settings_json) SELECT ?,?,'refined','running',?,version_id,? FROM transcript_selection WHERE meeting_id=? ON CONFLICT(id) DO NOTHING")
        .bind(&id).bind(meeting).bind(chrono::Utc::now().to_rfc3339()).bind(settings.to_string()).bind(meeting)
        .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE transcript_versions SET status='running',error=NULL WHERE id=? AND meeting_id=? AND status!='ready'")
        .bind(&id).bind(meeting).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(id)
}

pub async fn finish(
    pool: &SqlitePool,
    id: &str,
    segments: &[TranscriptSegment],
) -> Result<(), String> {
    let changed = sqlx::query(
        "UPDATE transcript_versions SET segments_json = ? WHERE id = ? AND status = 'running'",
    )
    .bind(serde_json::to_string(segments).map_err(|e| e.to_string())?)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if changed != 1 {
        return Err("Refinement is no longer running; result was not committed".into());
    }
    Ok(())
}

/// ASR commits words and capture origin together. This is not a speaker/name
/// classification: the source map must survive every later label correction.
pub async fn finish_with_sources(
    pool: &SqlitePool,
    id: &str,
    segments: &[TranscriptSegment],
    sources: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    if segments.iter().any(|segment| {
        !sources.get(&segment.id).is_some_and(|source| {
            matches!(source.as_str(), "microphone" | "system" | "room" | "mixed")
        })
    }) {
        return Err("ASR source provenance is incomplete".into());
    }
    let changed = sqlx::query("UPDATE transcript_versions SET segments_json = ?, settings_json = json_set(settings_json, '$.source_map', json(?)) WHERE id = ? AND status = 'running'")
        .bind(serde_json::to_string(segments).map_err(|e| e.to_string())?).bind(serde_json::to_string(sources).map_err(|e| e.to_string())?).bind(id)
        .execute(pool).await.map_err(|e| e.to_string())?.rows_affected();
    if changed != 1 {
        return Err("Refinement is no longer running; source commit was not saved".into());
    }
    Ok(())
}

pub async fn version_sources(
    pool: &SqlitePool,
    meeting: &str,
    version: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let settings: String = sqlx::query_scalar(
        "SELECT settings_json FROM transcript_versions WHERE id = ? AND meeting_id = ?",
    )
    .bind(version)
    .bind(meeting)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    let settings: serde_json::Value = serde_json::from_str(&settings).map_err(|e| e.to_string())?;
    serde_json::from_value(
        settings
            .get("source_map")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )
    .map_err(|e| e.to_string())
}

pub async fn set_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    error: Option<String>,
) -> Result<(), String> {
    let changed = sqlx::query(
        "UPDATE transcript_versions SET status = ?, error = ? WHERE id = ? AND status = 'running' AND (? != 'ready' OR segments_json IS NOT NULL)",
    )
    .bind(status)
    .bind(error)
    .bind(id)
    .bind(status)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if changed != 1 {
        return Err("Refinement state changed or has no completed transcript; terminal state was not committed".into());
    }
    Ok(())
}

pub async fn segments(
    pool: &SqlitePool,
    meeting: &str,
    version: &str,
) -> Result<Vec<TranscriptSegment>, String> {
    let json: String = sqlx::query_scalar("SELECT segments_json FROM transcript_versions WHERE meeting_id = ? AND id = ? AND segments_json IS NOT NULL")
        .bind(meeting).bind(version).fetch_one(pool).await.map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Version {
    id: String,
    kind: String,
    status: String,
    created_at: String,
    parent_id: Option<String>,
    settings_json: String,
    error: Option<String>,
    summary_json: Option<String>,
}
#[derive(Serialize)]
pub struct Versions {
    pub selected_id: String,
    pub revision: i64,
    pub versions: Vec<Version>,
    pub audio_available: bool,
}

pub async fn list(pool: &SqlitePool, meeting: &str) -> Result<Versions, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE meetings SET id = id WHERE id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    ensure(&mut tx, meeting).await?;
    let (selected_id, revision): (String, i64) = sqlx::query_as(
        "SELECT version_id, revision FROM transcript_selection WHERE meeting_id = ?",
    )
    .bind(meeting)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let versions = sqlx::query_as::<_, Version>("SELECT id, kind, status, created_at, parent_id, settings_json, error, summary_json FROM transcript_versions WHERE meeting_id = ? ORDER BY created_at, id")
        .bind(meeting).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    let audio_available = false;
    Ok(Versions {
        selected_id,
        revision,
        versions,
        audio_available,
    })
}

pub async fn select(
    pool: &SqlitePool,
    meeting: &str,
    version: &str,
    revision: i64,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let changed = sqlx::query("UPDATE transcript_selection SET revision = revision + 1 WHERE meeting_id = ? AND revision = ?")
        .bind(meeting).bind(revision).execute(&mut *tx).await.map_err(|e| e.to_string())?.rows_affected();
    if changed != 1 {
        return Err("Transcript changed in another operation. Refresh versions and review again; nothing was replaced.".into());
    }
    let (json, saved_links): (String, Option<String>) = sqlx::query_as("SELECT segments_json,speaker_links_json FROM transcript_versions WHERE id = ? AND meeting_id = ? AND status = 'ready'")
        .bind(version).bind(meeting).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
    let target: Vec<TranscriptSegment> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    // A pre-migration baseline may not have its own mapping, but an exact
    // checkpoint made when leaving it can still preserve its curated identity.
    let historical_links: Vec<String> = if saved_links.is_none() {
        sqlx::query_scalar("SELECT speaker_links_json FROM transcript_versions WHERE meeting_id=? AND parent_id=? AND kind='corrections' AND speaker_links_json IS NOT NULL ORDER BY created_at DESC,id DESC")
            .bind(meeting).bind(version).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };
    let current_links = speaker_links(&mut tx, meeting).await?;
    let checkpoint_id = checkpoint(&mut tx, meeting, true).await?;
    // Capture the source before deleting canonical rows. Existing audio_time is
    // authoritative; older tasks can recover it from their still-present row.
    sqlx::query("INSERT OR IGNORE INTO action_item_transcript_sources(action_id,version_id,transcript_id,audio_time) SELECT a.id,?,a.transcript_id,COALESCE(a.audio_time,t.audio_start_time) FROM action_items a LEFT JOIN transcripts t ON t.id=a.transcript_id AND t.meeting_id=a.meeting_id WHERE a.meeting_id=?")
        .bind(&checkpoint_id).bind(meeting).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE action_items SET audio_time=COALESCE(audio_time,(SELECT audio_time FROM action_item_transcript_sources WHERE action_id=action_items.id)) WHERE meeting_id=?")
        .bind(meeting).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM transcripts WHERE meeting_id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for s in &target {
        sqlx::query("INSERT INTO transcripts(id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration, speaker) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&s.id).bind(meeting).bind(&s.text).bind(&s.timestamp).bind(s.audio_start_time).bind(s.audio_end_time).bind(s.duration).bind(&s.speaker)
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    // Prefer the original exact anchor when restoring; otherwise use only a
    // segment containing the saved source time. Gaps fall back to audio/meeting.
    sqlx::query("UPDATE action_items SET transcript_id=COALESCE((SELECT t.id FROM action_item_transcript_sources a JOIN transcripts t ON t.id=a.transcript_id AND t.meeting_id=action_items.meeting_id WHERE a.action_id=action_items.id),(SELECT t.id FROM transcripts t WHERE t.meeting_id=action_items.meeting_id AND t.audio_start_time<=action_items.audio_time AND t.audio_end_time>action_items.audio_time ORDER BY t.audio_start_time DESC,t.id LIMIT 1)) WHERE meeting_id=?")
        .bind(meeting).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM person_speakers WHERE meeting_id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // Snapshot links win, including curated associations whose display name is
    // unrelated. Older snapshots reuse surviving current label associations.
    let mut links: Vec<(String, String)> = if let Some(saved) = saved_links {
        serde_json::from_str(&saved).map_err(|e| e.to_string())?
    } else {
        let mut preserved = Vec::new();
        for history in historical_links {
            preserved.extend(
                serde_json::from_str::<Vec<(String, String)>>(&history)
                    .map_err(|e| e.to_string())?,
            );
        }
        preserved.extend(
            serde_json::from_str::<Vec<(String, String)>>(&current_links)
                .map_err(|e| e.to_string())?,
        );
        preserved
    };
    // The most recent explicit mapping for a spelling wins; unrelated older
    // labels remain recoverable for legacy baselines without their own mapping.
    let mut known_labels = std::collections::HashSet::new();
    links.retain(|(_, label)| known_labels.insert(label.clone()));
    for label in target.iter().filter_map(|s| s.speaker.as_deref()) {
        let exact = links.iter().find(|(_, source)| source == label);
        let normalized: Vec<_> = links
            .iter()
            .filter(|(_, source)| {
                crate::database::repositories::speaker_identity::normalize_person_name(source)
                    == crate::database::repositories::speaker_identity::normalize_person_name(label)
            })
            .collect();
        let mapping = exact.or_else(|| {
            normalized
                .first()
                .copied()
                .filter(|first| normalized.iter().all(|link| link.0 == first.0))
        });
        // A recorded but deleted identity is not absent evidence: the INSERT
        // leaves it unlinked. Conflicting normalized mappings likewise must not
        // fall through to generic display-name reconstruction.
        if let Some((person, _)) = mapping {
            sqlx::query("INSERT OR IGNORE INTO person_speakers(person_id,meeting_id,speaker_label) SELECT id,?,? FROM people WHERE id=?")
                .bind(meeting).bind(label).bind(person).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        } else if normalized.is_empty()
            && crate::database::repositories::speaker_identity::is_person_name(label)
        {
            crate::database::repositories::speaker_identity::ensure_label_identity(
                &mut tx, meeting, label,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    sqlx::query("UPDATE transcript_selection SET version_id = ? WHERE meeting_id = ?")
        .bind(version)
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // Summary/cache text belongs to the previously selected version. Do not
    // present it as a summary of the new text. #15/#16 should key by version+revision.
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Existing speaker controls and speaker-only reruns checkpoint before editing
/// the selected projection too; generated labels can also be manual corrections.
pub async fn checkpoint_current(pool: &SqlitePool, meeting: &str) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE meetings SET id = id WHERE id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    ensure(&mut tx, meeting).await?;
    checkpoint(&mut tx, meeting, false).await?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn checkpoint(
    conn: &mut SqliteConnection,
    meeting: &str,
    clear_derived: bool,
) -> Result<String, String> {
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM summary_processes WHERE meeting_id = ? AND status NOT IN ('completed', 'failed', 'cancelled', 'error')")
        .bind(meeting).fetch_one(&mut *conn).await.map_err(|e| e.to_string())?;
    if clear_derived && pending > 0 {
        return Err(
            "Wait for or cancel summary generation before selecting a transcript version".into(),
        );
    }
    let summary: Option<String> = sqlx::query_scalar("SELECT json_object('result', result, 'result_backup', result_backup, 'user_edited', user_edited) FROM summary_processes WHERE meeting_id = ?")
        .bind(meeting).fetch_optional(&mut *conn).await.map_err(|e| e.to_string())?;
    let old = current(conn, meeting).await?;
    let links = speaker_links(conn, meeting).await?;
    let checkpoint_id = Uuid::new_v4().to_string();
    // Always checkpoint the editable projection, even when edits came from a
    // legacy writer. Never mutate the immutable live/refined baseline.
    sqlx::query("INSERT INTO transcript_versions(id, meeting_id, kind, status, created_at, parent_id, segments_json, summary_json, settings_json, speaker_links_json) SELECT ?, ?, 'corrections', 'ready', ?, s.version_id, ?, ?, v.settings_json, ? FROM transcript_selection s JOIN transcript_versions v ON v.id = s.version_id WHERE s.meeting_id = ?")
        .bind(&checkpoint_id).bind(meeting).bind(chrono::Utc::now().to_rfc3339()).bind(serde_json::to_string(&old).map_err(|e| e.to_string())?).bind(summary).bind(links).bind(meeting)
        .execute(&mut *conn).await.map_err(|e| e.to_string())?;
    if clear_derived {
        sqlx::query("DELETE FROM transcript_chunks WHERE meeting_id = ?")
            .bind(meeting)
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM summary_processes WHERE meeting_id = ?")
            .bind(meeting)
            .execute(conn)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(checkpoint_id)
}

#[derive(Serialize)]
pub struct SelectedTranscript {
    pub settings_json: String,
    pub version_id: String,
    pub revision: i64,
    pub segments: Vec<TranscriptSegment>,
}

pub async fn selected(pool: &SqlitePool, meeting: &str) -> Result<SelectedTranscript, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    // Consumers such as #15 may request this before the version browser has
    // lazily initialized an older meeting. Acquire write ownership before ensure.
    sqlx::query("UPDATE meetings SET id = id WHERE id = ?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    ensure(&mut tx, meeting).await?;

    let (version_id, revision) = sqlx::query_as::<_, (String, i64)>(
        "SELECT version_id, revision FROM transcript_selection WHERE meeting_id = ?",
    )
    .bind(meeting)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let segments = current(&mut tx, meeting).await?;
    let settings_json: String = sqlx::query_scalar(
        "SELECT settings_json FROM transcript_versions WHERE id = ? AND meeting_id = ?",
    )
    .bind(&version_id)
    .bind(meeting)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(SelectedTranscript {
        settings_json,
        version_id,
        revision,
        segments,
    })
}

pub async fn correct_text(
    pool: &SqlitePool,
    meeting: &str,
    transcript: &str,
    text: &str,
    revision: i64,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("Correction cannot be empty".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let changed = sqlx::query("UPDATE transcript_selection SET revision = revision + 1 WHERE meeting_id = ? AND revision = ?")
        .bind(meeting).bind(revision).execute(&mut *tx).await.map_err(|e| e.to_string())?.rows_affected();
    if changed != 1 {
        return Err(
            "Transcript changed in another operation. Reload before editing; nothing was replaced."
                .into(),
        );
    }
    checkpoint(&mut tx, meeting, true).await?;
    let updated =
        sqlx::query("UPDATE transcripts SET transcript = ? WHERE id = ? AND meeting_id = ?")
            .bind(text)
            .bind(transcript)
            .bind(meeting)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
    if updated != 1 {
        return Err("Transcript line no longer belongs to the selected version".into());
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        initialize_fixture(&pool).await;
        pool
    }
    async fn initialize_fixture(pool: &SqlitePool) {
        sqlx::raw_sql("CREATE TABLE meetings(id TEXT PRIMARY KEY, folder_path TEXT); CREATE TABLE transcripts(id TEXT PRIMARY KEY, meeting_id TEXT, transcript TEXT, timestamp TEXT, audio_start_time REAL, audio_end_time REAL, duration REAL, speaker TEXT); CREATE TABLE people(id TEXT PRIMARY KEY, display_name TEXT, normalized_name TEXT, notes TEXT, created_at TEXT, updated_at TEXT); CREATE TABLE action_items(id TEXT PRIMARY KEY, meeting_id TEXT, transcript_id TEXT, audio_time REAL, text TEXT, done INTEGER, edited INTEGER); CREATE TABLE person_speakers(person_id TEXT, meeting_id TEXT, speaker_label TEXT, UNIQUE(meeting_id, speaker_label)); CREATE TABLE transcript_chunks(meeting_id TEXT); CREATE TABLE summary_processes(meeting_id TEXT, status TEXT, result TEXT, result_backup TEXT, user_edited INTEGER); INSERT INTO meetings VALUES ('m', NULL); INSERT INTO meetings VALUES ('other', NULL); INSERT INTO transcripts VALUES ('live-row', 'm', 'RU/EN original API', '2026-11-18T10:00:01Z', 1, 2, 1, 'You');")
            .execute(pool).await.unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261118001400_transcript_versions.sql"
        ))
        .execute(pool)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261118001401_transcript_version_references.sql"
        ))
        .execute(pool)
        .await
        .unwrap();
    }
    fn segment(id: &str, text: &str, speaker: &str) -> TranscriptSegment {
        TranscriptSegment {
            id: id.into(),
            text: text.into(),
            timestamp: "2026-11-18T10:00:01Z".into(),
            audio_start_time: Some(1.0),
            audio_end_time: Some(2.0),
            duration: Some(1.0),
            speaker: Some(speaker.into()),
        }
    }
    async fn ready(pool: &SqlitePool) -> String {
        let id = begin(
            pool,
            "m",
            &serde_json::json!({"provider":"whisper", "model":"synthetic"}),
        )
        .await
        .unwrap();
        finish(
            pool,
            &id,
            &[segment("refined-row", "Уточнение API", "Guest")],
        )
        .await
        .unwrap();
        set_status(pool, &id, "ready", None).await.unwrap();
        id
    }
    async fn text(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT transcript FROM transcripts WHERE meeting_id = 'm'")
            .fetch_one(pool)
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn candidate_never_replaces_live_and_all_edits_are_checkpointed() {
        let pool = pool().await;
        let initial = list(&pool, "m").await.unwrap();
        let id = ready(&pool).await;
        assert_eq!(text(&pool).await, "RU/EN original API");
        sqlx::query("UPDATE transcripts SET transcript = 'ручное исправление API', speaker = 'Alice' WHERE id = 'live-row'").execute(&pool).await.unwrap();
        let state = list(&pool, "m").await.unwrap();
        select(&pool, "m", &id, state.revision).await.unwrap();
        assert_eq!(text(&pool).await, "Уточнение API");
        let saved: String =
            sqlx::query_scalar("SELECT id FROM transcript_versions WHERE kind = 'corrections'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let correction = segments(&pool, "m", &saved).await.unwrap();
        assert_eq!(correction[0].text, "ручное исправление API");
        assert_eq!(correction[0].speaker.as_deref(), Some("Alice"));
        let original = segments(&pool, "m", &initial.selected_id).await.unwrap();
        assert_eq!(original[0].text, "RU/EN original API");
        assert_eq!(original[0].speaker.as_deref(), Some("You"));
        let state = list(&pool, "m").await.unwrap();
        select(&pool, "m", &saved, state.revision).await.unwrap();
        assert_eq!(text(&pool).await, "ручное исправление API");
    }
    #[tokio::test]
    async fn stale_ui_and_cross_meeting_selection_roll_back_without_changes() {
        let pool = pool().await;
        let id = ready(&pool).await;
        let stale = list(&pool, "m").await.unwrap();
        sqlx::query("UPDATE transcripts SET speaker = 'Corrected' WHERE id = 'live-row'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(select(&pool, "m", &id, stale.revision)
            .await
            .unwrap_err()
            .contains("changed"));
        let state = list(&pool, "m").await.unwrap();
        assert!(select(&pool, "other", &id, 0).await.is_err());
        assert_eq!(list(&pool, "m").await.unwrap().revision, state.revision);
        assert_eq!(text(&pool).await, "RU/EN original API");
    }
    #[tokio::test]
    async fn failed_cancelled_interrupted_and_retry_leave_selected_rows_readable() {
        let pool = pool().await;
        let initial = list(&pool, "m").await.unwrap().selected_id;
        for status in ["failed", "cancelled", "interrupted"] {
            let id = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
            finish(&pool, &id, &[segment("partial", "partial", "Guest")])
                .await
                .unwrap();
            set_status(&pool, &id, status, Some(status.into()))
                .await
                .unwrap();
            let state = list(&pool, "m").await.unwrap();
            assert!(select(&pool, "m", &id, state.revision).await.is_err());
            assert_eq!(state.selected_id, initial);
            assert_eq!(text(&pool).await, "RU/EN original API");
        }
        ready(&pool).await;
        assert_eq!(text(&pool).await, "RU/EN original API");
    }
    #[tokio::test]
    async fn summary_is_archived_not_misattributed_and_pending_summary_blocks_switch() {
        let pool = pool().await;
        let id = ready(&pool).await;
        sqlx::query(
            "INSERT INTO summary_processes VALUES ('m', 'PENDING', 'manual summary', 'backup', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = list(&pool, "m").await.unwrap();
        assert!(select(&pool, "m", &id, state.revision)
            .await
            .unwrap_err()
            .contains("summary"));
        sqlx::query("UPDATE summary_processes SET status = 'completed'")
            .execute(&pool)
            .await
            .unwrap();
        select(&pool, "m", &id, state.revision).await.unwrap();
        let archive: String = sqlx::query_scalar(
            "SELECT summary_json FROM transcript_versions WHERE kind = 'corrections'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(archive.contains("manual summary") && archive.contains("backup"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM summary_processes")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn transaction_failure_during_projection_restores_previous_version() {
        let pool = pool().await;
        let id = ready(&pool).await;
        sqlx::query("INSERT INTO transcripts VALUES ('refined-row', 'other', 'other meeting', 't', 0, 1, 1, 'Guest')").execute(&pool).await.unwrap();
        let state = list(&pool, "m").await.unwrap();
        assert!(select(&pool, "m", &id, state.revision).await.is_err());
        assert_eq!(text(&pool).await, "RU/EN original API");
        assert_eq!(list(&pool, "m").await.unwrap().revision, state.revision);
    }
    #[tokio::test]
    async fn deleted_audio_does_not_gate_read_or_select() {
        let pool = pool().await;
        let id = ready(&pool).await;
        let state = list(&pool, "m").await.unwrap();
        assert!(!state.audio_available);
        assert_eq!(
            segments(&pool, "m", &id).await.unwrap()[0].text,
            "Уточнение API"
        );
        select(&pool, "m", &id, state.revision).await.unwrap();
        assert_eq!(text(&pool).await, "Уточнение API");
    }
    #[tokio::test]
    async fn manual_text_command_preserves_identity_timing_and_rejects_old_revision() {
        let pool = pool().await;
        list(&pool, "m").await.unwrap();
        let before = selected(&pool, "m").await.unwrap();
        correct_text(&pool, "m", "live-row", "исправлено API", before.revision)
            .await
            .unwrap();
        let after = selected(&pool, "m").await.unwrap();
        assert_eq!(after.segments[0].text, "исправлено API");
        assert_eq!(after.segments[0].id, before.segments[0].id);
        assert_eq!(after.segments[0].timestamp, before.segments[0].timestamp);
        assert_eq!(after.segments[0].speaker, before.segments[0].speaker);
        assert!(
            correct_text(&pool, "m", "live-row", "stale", before.revision)
                .await
                .is_err()
        );
        assert!(correct_text(&pool, "m", "missing", "bad", after.revision)
            .await
            .is_err());
        assert_eq!(selected(&pool, "m").await.unwrap().revision, after.revision);
    }
    #[tokio::test]
    async fn restart_marks_only_running_candidates_and_preserves_corrections() {
        let pool = pool().await;
        list(&pool, "m").await.unwrap();
        let ready = ready(&pool).await;
        let pending = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        correct_text(
            &pool,
            "m",
            "live-row",
            "saved correction",
            selected(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        interrupt_running(&pool).await.unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM transcript_versions WHERE id = ?")
                .bind(pending)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "interrupted");
        let status: String =
            sqlx::query_scalar("SELECT status FROM transcript_versions WHERE id = ?")
                .bind(ready)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "ready");
        assert_eq!(text(&pool).await, "saved correction");
    }
    #[tokio::test]
    async fn concurrent_selection_and_text_edit_have_exactly_one_winner() {
        let pool = pool().await;
        let id = ready(&pool).await;
        let state = selected(&pool, "m").await.unwrap();
        let (selection, correction) = tokio::join!(
            select(&pool, "m", &id, state.revision),
            correct_text(
                &pool,
                "m",
                "live-row",
                "concurrent manual edit",
                state.revision
            )
        );
        assert_ne!(selection.is_ok(), correction.is_ok());
        if correction.is_ok() {
            assert_eq!(text(&pool).await, "concurrent manual edit");
        } else {
            assert_eq!(text(&pool).await, "Уточнение API");
        }
    }

    #[tokio::test]
    async fn terminal_snapshots_cannot_be_rewritten_or_marked_ready_without_text() {
        let pool = pool().await;
        let id = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        assert!(set_status(&pool, &id, "ready", None).await.is_err());
        finish(&pool, &id, &[segment("candidate", "saved", "Guest")])
            .await
            .unwrap();
        set_status(&pool, &id, "ready", None).await.unwrap();
        assert!(
            finish(&pool, &id, &[segment("candidate", "overwrite", "You")])
                .await
                .is_err()
        );
        assert!(set_status(&pool, &id, "cancelled", None).await.is_err());
        assert_eq!(segments(&pool, "m", &id).await.unwrap()[0].text, "saved");
    }

    #[tokio::test]
    async fn disk_reopen_retains_original_corrections_candidates_and_interruption_state() {
        let dir = std::env::temp_dir().join(format!("meetily-version-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("synthetic.sqlite");
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        initialize_fixture(&pool).await;
        let initial = list(&pool, "m").await.unwrap();
        correct_text(
            &pool,
            "m",
            "live-row",
            "durable ручная API",
            initial.revision,
        )
        .await
        .unwrap();
        let completed = ready(&pool).await;
        let running = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        pool.close().await;
        let reopened = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        interrupt_running(&reopened).await.unwrap();
        assert_eq!(text(&reopened).await, "durable ручная API");
        assert_eq!(
            segments(&reopened, "m", &initial.selected_id)
                .await
                .unwrap()[0]
                .text,
            "RU/EN original API"
        );
        assert_eq!(
            segments(&reopened, "m", &completed).await.unwrap()[0].text,
            "Уточнение API"
        );
        let status: String =
            sqlx::query_scalar("SELECT status FROM transcript_versions WHERE id = ?")
                .bind(running)
                .fetch_one(&reopened)
                .await
                .unwrap();
        assert_eq!(status, "interrupted");
        reopened.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[tokio::test]
    async fn speaker_control_checkpoint_keeps_generated_manual_labels_and_summary() {
        let pool = pool().await;
        list(&pool, "m").await.unwrap();
        sqlx::query("UPDATE transcripts SET speaker = 'Speaker 2' WHERE id = 'live-row'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO summary_processes VALUES ('m', 'completed', 'keep summary', NULL, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        checkpoint_current(&pool, "m").await.unwrap();
        sqlx::query("UPDATE transcripts SET speaker = 'Speaker 3' WHERE id = 'live-row'")
            .execute(&pool)
            .await
            .unwrap();
        let saved: String =
            sqlx::query_scalar("SELECT id FROM transcript_versions WHERE kind = 'corrections'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            segments(&pool, "m", &saved).await.unwrap()[0]
                .speaker
                .as_deref(),
            Some("Speaker 2")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM summary_processes")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn moving_a_canonical_row_invalidates_both_meetings_revisions() {
        let pool = pool().await;
        let before = list(&pool, "m").await.unwrap();
        let other = list(&pool, "other").await.unwrap();
        sqlx::query("UPDATE transcripts SET meeting_id = 'other' WHERE id = 'live-row'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(list(&pool, "m").await.unwrap().revision > before.revision);
        assert!(list(&pool, "other").await.unwrap().revision > other.revision);
    }
    #[tokio::test]
    async fn capture_origin_and_opaque_policy_survive_correction_selection_and_restore() {
        let pool = pool().await;
        let metadata = serde_json::json!({"in_person":false, "future_native_policy":{"uninterpreted_flag":true}, "provenance":{"session":"synthetic-session"}});
        let id = begin(
            &pool,
            "m",
            &serde_json::json!({"recording_metadata":metadata, "label_origin":"generated_asr"}),
        )
        .await
        .unwrap();
        let rows = [
            segment("mic", "local", "You"),
            segment("remote", "remote", "Guest"),
        ];
        let sources = std::collections::HashMap::from([
            ("mic".into(), "microphone".into()),
            ("remote".into(), "system".into()),
        ]);
        assert!(
            finish_with_sources(&pool, &id, &rows, &std::collections::HashMap::new())
                .await
                .is_err()
        );
        finish_with_sources(&pool, &id, &rows, &sources)
            .await
            .unwrap();
        assert_eq!(version_sources(&pool, "m", &id).await.unwrap(), sources);
        set_status(&pool, &id, "ready", None).await.unwrap();
        select(&pool, "m", &id, list(&pool, "m").await.unwrap().revision)
            .await
            .unwrap();
        let before = selected(&pool, "m").await.unwrap();
        correct_text(&pool, "m", "mic", "manual words", before.revision)
            .await
            .unwrap();
        checkpoint_current(&pool, "m").await.unwrap();
        let snapshot: String = sqlx::query_scalar("SELECT id FROM transcript_versions WHERE kind = 'corrections' AND segments_json LIKE '%manual words%'").fetch_one(&pool).await.unwrap();
        select(
            &pool,
            "m",
            &snapshot,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        let restored = selected(&pool, "m").await.unwrap();
        let settings: serde_json::Value = serde_json::from_str(&restored.settings_json).unwrap();
        assert_eq!(settings["recording_metadata"], metadata);
        assert_eq!(settings["label_origin"], "generated_asr");
        assert_eq!(
            version_sources(&pool, "m", &snapshot).await.unwrap(),
            sources
        );
        assert_eq!(
            restored
                .segments
                .iter()
                .find(|s| s.id == "mic")
                .unwrap()
                .text,
            "manual words"
        );
        assert!(finish_with_sources(&pool, &id, &rows, &sources)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn selected_snapshot_initializes_legacy_meeting_without_ui_side_effects() {
        let pool = pool().await;
        let current = selected(&pool, "m").await.unwrap();
        assert_eq!(current.segments[0].text, "RU/EN original API");
        assert_eq!(current.revision, 0);
        assert_eq!(
            list(&pool, "m").await.unwrap().selected_id,
            current.version_id
        );
    }
    #[tokio::test]
    async fn r1_selection_reconciles_normalized_names_and_restores_curated_links() {
        let pool = pool().await;
        sqlx::raw_sql("INSERT INTO people(id,display_name) VALUES ('alice-person','Alice'), ('curated','Different Display Name'); INSERT INTO person_speakers VALUES ('curated','m','You');").execute(&pool).await.unwrap();
        let initial = list(&pool, "m").await.unwrap();
        // Model a version created before the additive association migration.
        sqlx::query("UPDATE transcript_versions SET speaker_links_json=NULL WHERE id=?")
            .bind(&initial.selected_id)
            .execute(&pool)
            .await
            .unwrap();
        let candidate = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        finish(&pool, &candidate, &[segment("new", "new words", "alice")])
            .await
            .unwrap();
        set_status(&pool, &candidate, "ready", None).await.unwrap();
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        let owner: Option<String> =
            sqlx::query_scalar("SELECT person_id FROM person_speakers WHERE speaker_label='alice'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(owner.as_deref(), Some("alice-person"));
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        let owner: Option<String> =
            sqlx::query_scalar("SELECT person_id FROM person_speakers WHERE speaker_label='You'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(owner.as_deref(), Some("curated"));
    }
    #[tokio::test]
    async fn r1_selection_preserves_tasks_and_resolves_source_after_resegmentation() {
        let pool = pool().await;
        sqlx::raw_sql("INSERT INTO action_items VALUES ('task','m','live-row',NULL,'curated task',1,1),('absent','m','missing-row',NULL,'no anchor',0,1);").execute(&pool).await.unwrap();
        let initial = list(&pool, "m").await.unwrap();
        let candidate = ready(&pool).await;
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        let anchor: (Option<String>, Option<f64>, String, i64, i64) = sqlx::query_as(
            "SELECT transcript_id,audio_time,text,done,edited FROM action_items WHERE id='task'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            anchor,
            (
                Some("refined-row".into()),
                Some(1.0),
                "curated task".into(),
                1,
                1
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT transcript_id FROM action_items WHERE id='absent'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            None
        );
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT transcript_id FROM action_items WHERE id='task'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some("live-row".into())
        );
    }

    #[tokio::test]
    async fn r1_task_gaps_fall_back_to_audio_and_selection_failure_rolls_back_references() {
        let pool = pool().await;
        sqlx::query("INSERT INTO action_items VALUES ('task','m','live-row',9,'keep task',1,1)")
            .execute(&pool)
            .await
            .unwrap();
        let candidate = ready(&pool).await;
        let state = list(&pool, "m").await.unwrap();
        sqlx::raw_sql("CREATE TRIGGER fail_selection BEFORE UPDATE OF version_id ON transcript_selection BEGIN SELECT RAISE(ABORT,'synthetic reference failure'); END;").execute(&pool).await.unwrap();
        assert!(select(&pool, "m", &candidate, state.revision)
            .await
            .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM action_item_transcript_sources")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT transcript_id FROM action_items WHERE id='task'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "live-row"
        );
        sqlx::raw_sql("DROP TRIGGER fail_selection")
            .execute(&pool)
            .await
            .unwrap();
        select(&pool, "m", &candidate, state.revision)
            .await
            .unwrap();
        let anchor: (Option<String>, Option<f64>) =
            sqlx::query_as("SELECT transcript_id,audio_time FROM action_items WHERE id='task'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(anchor, (None, Some(9.0)));
        let source:(String,Option<String>)=sqlx::query_as("SELECT version_id,transcript_id FROM action_item_transcript_sources WHERE action_id='task'").fetch_one(&pool).await.unwrap();
        assert_eq!(
            segments(&pool, "m", &source.0).await.unwrap()[0].id,
            source.1.unwrap()
        );
    }
    #[tokio::test]
    async fn r2_deleted_named_contact_is_not_recreated_on_restore() {
        let pool = pool().await;
        sqlx::raw_sql("UPDATE transcripts SET speaker='Alice'; INSERT INTO people(id,display_name,normalized_name) VALUES('deleted-person','Alice','alice'); INSERT INTO person_speakers VALUES('deleted-person','m','Alice');").execute(&pool).await.unwrap();
        let initial = list(&pool, "m").await.unwrap();
        let candidate = ready(&pool).await;
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        // The contact is not currently used by this projection, so production
        // delete_person removes its record without relabelling historical JSON.
        sqlx::raw_sql("DELETE FROM person_speakers WHERE person_id='deleted-person'; DELETE FROM people WHERE id='deleted-person';").execute(&pool).await.unwrap();
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        let contacts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM people WHERE normalized_name='alice'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            contacts, 0,
            "restoring a snapshot recreated the deleted contact"
        );
        let restored = selected(&pool, "m").await.unwrap();
        assert_eq!(restored.segments[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(restored.segments[0].text, "RU/EN original API");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM person_speakers WHERE meeting_id='m'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn r2_ambiguous_normalized_curated_links_do_not_guess_a_person() {
        let pool = pool().await;
        sqlx::query("UPDATE transcripts SET speaker='alice'")
            .execute(&pool)
            .await
            .unwrap();
        let initial = list(&pool, "m").await.unwrap();
        // Existing baseline made before speaker_links_json migration.
        sqlx::query("UPDATE transcript_versions SET speaker_links_json=NULL WHERE id=?")
            .bind(&initial.selected_id)
            .execute(&pool)
            .await
            .unwrap();
        // Ordinary speaker controls can curate differently-spelled labels to
        // distinct contacts in the current editable projection.
        sqlx::raw_sql("INSERT INTO people(id,display_name,normalized_name) VALUES('p1','Alice','alice'),('p2','Other display','other'); UPDATE transcripts SET speaker='ALICE'; INSERT INTO transcripts VALUES('second','m','second','t',2,3,1,'Alice'); INSERT INTO person_speakers VALUES('p1','m','ALICE'),('p2','m','Alice');").execute(&pool).await.unwrap();
        let candidate = ready(&pool).await;
        select(
            &pool,
            "m",
            &candidate,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        let person: Option<String> = sqlx::query_scalar(
            "SELECT person_id FROM person_speakers WHERE meeting_id='m' AND speaker_label='alice'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(
            person, None,
            "ambiguous curated links silently fell back to a display-name identity"
        );
        let restored = selected(&pool, "m").await.unwrap();
        assert_eq!(restored.segments.len(), 1);
        assert_eq!(restored.segments[0].speaker.as_deref(), Some("alice"));
        assert_eq!(restored.segments[0].text, "RU/EN original API");
        let people: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id,display_name,normalized_name FROM people ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            people,
            vec![
                ("p1".into(), "Alice".into(), "alice".into()),
                ("p2".into(), "Other display".into(), "other".into())
            ]
        );
    }
    #[tokio::test]
    async fn r2_exact_curated_mapping_wins_over_normalized_conflict() {
        let pool = pool().await;
        sqlx::raw_sql("UPDATE transcripts SET speaker='Alice'; INSERT INTO people(id,display_name,normalized_name,notes) VALUES('p1','Alice','alice','keep p1'),('p2','Curated identity','curated','keep p2'); INSERT INTO person_speakers VALUES('p2','m','Alice'),('p1','m','ALICE');").execute(&pool).await.unwrap();
        let initial = list(&pool, "m").await.unwrap();
        let candidate = ready(&pool).await;
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        let person: String = sqlx::query_scalar(
            "SELECT person_id FROM person_speakers WHERE meeting_id='m' AND speaker_label='Alice'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(person, "p2");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT notes FROM people WHERE id='p1'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "keep p1"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT notes FROM people WHERE id='p2'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "keep p2"
        );
    }

    #[tokio::test]
    async fn r2_unambiguous_normalized_curated_mapping_is_restored() {
        let pool = pool().await;
        sqlx::query("UPDATE transcripts SET speaker='alice'")
            .execute(&pool)
            .await
            .unwrap();
        let initial = list(&pool, "m").await.unwrap();
        sqlx::query("UPDATE transcript_versions SET speaker_links_json=NULL WHERE id=?")
            .bind(&initial.selected_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql("INSERT INTO people(id,display_name,normalized_name) VALUES('name-guess','Alice','alice'),('curated','Different display','different'); UPDATE transcripts SET speaker='ALICE'; INSERT INTO person_speakers VALUES('curated','m','ALICE');").execute(&pool).await.unwrap();
        let candidate = ready(&pool).await;
        select(
            &pool,
            "m",
            &candidate,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT person_id FROM person_speakers WHERE meeting_id='m' AND speaker_label='alice'").fetch_one(&pool).await.unwrap(),"curated");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM people")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn r2_no_evidence_still_uses_normalized_existing_contact() {
        let pool = pool().await;
        sqlx::raw_sql("INSERT INTO people(id,display_name,normalized_name,notes) VALUES('p','Alice','alice','keep Alice'),('unrelated','Bob','bob','keep Bob');").execute(&pool).await.unwrap();
        let candidate = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        finish(
            &pool,
            &candidate,
            &[segment("new-alice", "unchanged words", "alice")],
        )
        .await
        .unwrap();
        set_status(&pool, &candidate, "ready", None).await.unwrap();
        select(
            &pool,
            "m",
            &candidate,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT person_id FROM person_speakers WHERE meeting_id='m' AND speaker_label='alice'").fetch_one(&pool).await.unwrap(),"p");
        let people: Vec<(String, String)> =
            sqlx::query_as("SELECT id,notes FROM people ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            people,
            vec![
                ("p".into(), "keep Alice".into()),
                ("unrelated".into(), "keep Bob".into())
            ]
        );
    }

    #[tokio::test]
    async fn r2_foreign_meeting_mapping_evidence_does_not_affect_restoration() {
        let pool = pool().await;
        sqlx::query("UPDATE transcripts SET speaker='Alice' WHERE meeting_id='m'")
            .execute(&pool)
            .await
            .unwrap();
        let initial = list(&pool, "m").await.unwrap();
        sqlx::query("UPDATE transcript_versions SET speaker_links_json=NULL WHERE id=?")
            .bind(&initial.selected_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql("INSERT INTO people(id,display_name,normalized_name,notes) VALUES('p','Alice','alice','keep p'),('foreign','Other identity','other','keep foreign'); INSERT INTO person_speakers VALUES('foreign','other','Alice');").execute(&pool).await.unwrap();
        // Even a malformed legacy parent link must not import another meeting's
        // curated mapping into this meeting's evidence set.
        sqlx::query("INSERT INTO transcript_versions(id,meeting_id,kind,status,created_at,parent_id,segments_json,speaker_links_json) VALUES('foreign-history','other','corrections','ready','2099',?,'[]',?)")
            .bind(&initial.selected_id).bind(serde_json::json!([["foreign","Alice"]]).to_string()).execute(&pool).await.unwrap();
        let candidate = ready(&pool).await;
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        select(
            &pool,
            "m",
            &initial.selected_id,
            list(&pool, "m").await.unwrap().revision,
        )
        .await
        .unwrap();
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT person_id FROM person_speakers WHERE meeting_id='m' AND speaker_label='Alice'").fetch_one(&pool).await.unwrap(),"p");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT person_id FROM person_speakers WHERE meeting_id='other' AND speaker_label='Alice'").fetch_one(&pool).await.unwrap(),"foreign");
        let before = list(&pool, "other").await.unwrap();
        assert!(
            select(&pool, "other", &initial.selected_id, before.revision)
                .await
                .is_err()
        );
        assert_eq!(
            list(&pool, "other").await.unwrap().revision,
            before.revision
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT notes FROM people WHERE id='foreign'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "keep foreign"
        );
    }

    #[tokio::test]
    async fn r2_no_evidence_contact_creation_and_links_roll_back_with_selection() {
        let pool = pool().await;
        let initial = list(&pool, "m").await.unwrap();
        let candidate = begin(&pool, "m", &serde_json::json!({})).await.unwrap();
        finish(
            &pool,
            &candidate,
            &[segment("new-alice", "unchanged words", "Alice")],
        )
        .await
        .unwrap();
        set_status(&pool, &candidate, "ready", None).await.unwrap();
        let versions = list(&pool, "m").await.unwrap().versions.len();
        sqlx::raw_sql("CREATE TRIGGER fail_identity_selection BEFORE UPDATE OF version_id ON transcript_selection BEGIN SELECT RAISE(ABORT,'synthetic identity failure'); END;").execute(&pool).await.unwrap();
        assert!(select(&pool, "m", &candidate, initial.revision)
            .await
            .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM people")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM person_speakers")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        let unchanged = list(&pool, "m").await.unwrap();
        assert_eq!(unchanged.revision, initial.revision);
        assert_eq!(unchanged.selected_id, initial.selected_id);
        assert_eq!(unchanged.versions.len(), versions);
        assert_eq!(text(&pool).await, "RU/EN original API");
        sqlx::raw_sql("DROP TRIGGER fail_identity_selection")
            .execute(&pool)
            .await
            .unwrap();
        select(&pool, "m", &candidate, initial.revision)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM people WHERE normalized_name='alice'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM person_speakers WHERE meeting_id='m' AND speaker_label='Alice'").fetch_one(&pool).await.unwrap(),1);
    }
}
