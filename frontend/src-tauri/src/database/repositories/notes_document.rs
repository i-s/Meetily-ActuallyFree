//! Session-keyed Notes persistence. A folder is allocated natively per recording;
//! titles, browser session IDs and anonymous speaker IDs are not identities.
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use super::meeting_notes::MeetingNotes;

async fn resolve(conn: &mut SqliteConnection, target: &str) -> Result<(bool, String), sqlx::Error> {
    if let Some(id) = target.strip_prefix("meeting:") {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM meetings WHERE id = ?)")
            .bind(id).fetch_one(&mut *conn).await?;
        if !exists { return Err(sqlx::Error::RowNotFound); }
        return Ok((true, id.into()));
    }
    if let Some(folder) = target.strip_prefix("recording:").filter(|s| !s.is_empty()) {
        let id: Option<String> = sqlx::query_scalar("SELECT id FROM meetings WHERE folder_path = ? ORDER BY created_at, id LIMIT 1")
            .bind(folder).fetch_optional(conn).await?;
        return Ok(match id { Some(id) => (true, id), None => (false, folder.into()) });
    }
    Err(sqlx::Error::Protocol("Invalid Notes target".into()))
}

pub async fn read(pool: &SqlitePool, target: &str) -> Result<Option<MeetingNotes>, sqlx::Error> {
    // Resolve and read in one snapshot, including when Stop concurrently attaches.
    let mut tx = pool.begin().await?;
    let (meeting, id) = resolve(&mut tx, target).await?;
    let query = if meeting {
        "SELECT notes_markdown, notes_json, updated_at, revision FROM meeting_notes WHERE meeting_id = ?"
    } else {
        "SELECT notes_markdown, notes_json, updated_at, revision FROM recording_notes WHERE folder_path = ?"
    };
    let row = sqlx::query_as::<_, (Option<String>, Option<String>, String, i64)>(query)
        .bind(id).fetch_optional(&mut *tx).await?;
    Ok(row.map(|(markdown, json, updated_at, revision)| MeetingNotes {
        source: "user_notes",
        markdown, json: json.and_then(|s| serde_json::from_str(&s).ok()), updated_at, revision,
    }))
}

pub async fn write(pool: &SqlitePool, target: &str, markdown: &str, json: Option<serde_json::Value>, expected_revision: i64) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Obtain the writer lock before resolving a folder. A concurrent library save
    // either sees this draft or completes first and redirects us to meeting_notes.
    sqlx::query("UPDATE recording_notes SET revision = revision WHERE 0").execute(&mut *tx).await?;
    let (meeting, id) = resolve(&mut tx, target).await?;
    let update = if meeting {
        "UPDATE meeting_notes SET notes_markdown = ?, notes_json = ?, revision = revision + 1, updated_at = datetime('now') WHERE meeting_id = ? AND revision = ?"
    } else {
        "UPDATE recording_notes SET notes_markdown = ?, notes_json = ?, revision = revision + 1, updated_at = datetime('now') WHERE folder_path = ? AND revision = ?"
    };
    let json = json.as_ref().map(|j| j.to_string());
    let mut changed = sqlx::query(update).bind(markdown).bind(&json).bind(&id)
        .bind(expected_revision).execute(&mut *tx).await?.rows_affected();
    if changed == 0 && expected_revision == 0 {
        let insert = if meeting {
            "INSERT INTO meeting_notes (meeting_id, notes_markdown, notes_json, revision, created_at, updated_at) VALUES (?, ?, ?, 1, datetime('now'), datetime('now')) ON CONFLICT DO NOTHING"
        } else {
            "INSERT INTO recording_notes (folder_path, notes_markdown, notes_json, revision, updated_at) VALUES (?, ?, ?, 1, datetime('now')) ON CONFLICT DO NOTHING"
        };
        changed = sqlx::query(insert).bind(&id).bind(markdown).bind(&json).execute(&mut *tx).await?.rows_affected();
    }
    if changed == 0 {
        // Retrying an acknowledged-by-SQLite but lost IPC response is harmless.
        // A different editor's content must never be overwritten by that retry.
        let select = if meeting {
            "SELECT notes_markdown, notes_json, revision FROM meeting_notes WHERE meeting_id = ?"
        } else {
            "SELECT notes_markdown, notes_json, revision FROM recording_notes WHERE folder_path = ?"
        };
        let current: Option<(Option<String>, Option<String>, i64)> = sqlx::query_as(select).bind(&id).fetch_optional(&mut *tx).await?;
        if !matches!(current, Some((ref m, ref j, r)) if m.as_deref() == Some(markdown) && j == &json && r == expected_revision + 1) {
            return Err(sqlx::Error::Protocol("Notes conflict: another editor saved changes. Draft retained; copy your text before reopening.".into()));
        }
    }
    tx.commit().await?;
    Ok(expected_revision + 1)
}

/// Called within BOTH normal save and idempotent recovery transactions. Never
/// overwrite post-call edits on a retry. Late live writes resolve to this row.
pub async fn attach(tx: &mut Transaction<'_, Sqlite>, folder: Option<&str>, meeting_id: &str) -> Result<(), sqlx::Error> {
    if let Some(folder) = folder {
        sqlx::query("INSERT INTO meeting_notes (meeting_id, notes_markdown, notes_json, revision, created_at, updated_at)
            SELECT ?, notes_markdown, notes_json, revision, updated_at, updated_at FROM recording_notes WHERE folder_path = ?
            ON CONFLICT(meeting_id) DO NOTHING")
            .bind(meeting_id).bind(folder).execute(&mut **tx).await?;
        sqlx::query("DELETE FROM recording_notes WHERE folder_path = ?").bind(folder).execute(&mut **tx).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn api_read_notes_document(state: tauri::State<'_, crate::state::AppState>, target: String) -> Result<Option<MeetingNotes>, String> {
    read(state.db_manager.pool(), &target).await.map_err(|e| format!("Failed to load Notes: {e}"))
}

#[tauri::command]
pub async fn api_write_notes_document(state: tauri::State<'_, crate::state::AppState>, target: String, markdown: String, json: Option<serde_json::Value>, expected_revision: i64) -> Result<i64, String> {
    write(state.db_manager.pool(), &target, &markdown, json, expected_revision).await.map_err(|e| format!("Failed to save Notes: {e}"))
}

/// Lets library/recovery views find a last-keystroke WebView journal written
/// before Stop attached the corresponding native draft.
#[tauri::command]
pub async fn api_notes_recording_target(state: tauri::State<'_, crate::state::AppState>, meeting_id: String) -> Result<Option<String>, String> {
    let folder: Option<Option<String>> = sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
        .bind(meeting_id).fetch_optional(state.db_manager.pool()).await.map_err(|e| e.to_string())?;
    Ok(folder.flatten().map(|f| format!("recording:{f}")))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub async fn create_notes_schema(pool: &SqlitePool) {
        sqlx::raw_sql("CREATE TABLE meeting_notes (meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE, notes_markdown TEXT, notes_json TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);")
            .execute(pool).await.unwrap();
        sqlx::raw_sql(include_str!("../../../migrations/20261118001200_recording_notes.sql")).execute(pool).await.unwrap();
    }
    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect(":memory:").await.unwrap();
        sqlx::raw_sql("CREATE TABLE meetings (id TEXT PRIMARY KEY, folder_path TEXT, created_at TEXT);").execute(&pool).await.unwrap();
        create_notes_schema(&pool).await;
        pool
    }
    #[tokio::test]
    async fn notes_survive_database_close_and_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(directory.path().join("synthetic-notes.sqlite"))
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options.clone()).await.unwrap();
        sqlx::raw_sql("CREATE TABLE meetings (id TEXT PRIMARY KEY, folder_path TEXT, created_at TEXT);").execute(&pool).await.unwrap();
        create_notes_schema(&pool).await;
        write(&pool, "recording:/synthetic/restart", "RU/EN idea: обсудить API", Some(serde_json::json!([])), 0).await.unwrap();
        pool.close().await;
        let reopened = SqlitePool::connect_with(options).await.unwrap();
        let notes = read(&reopened, "recording:/synthetic/restart").await.unwrap().unwrap();
        assert_eq!(notes.markdown.as_deref(), Some("RU/EN idea: обсудить API"));
        assert_eq!(notes.revision, 1);
        assert_eq!(notes.source, "user_notes");
    }
    #[tokio::test]
    async fn retries_rapid_edits_conflicts_and_empty_notes() {
        let pool = pool().await;
        let target = "recording:/synthetic/a";
        assert_eq!(write(&pool, target, "first", None, 0).await.unwrap(), 1);
        assert_eq!(write(&pool, target, "first", None, 0).await.unwrap(), 1);
        assert!(write(&pool, target, "stale", None, 0).await.is_err());
        assert_eq!(write(&pool, target, "latest", None, 1).await.unwrap(), 2);
        assert_eq!(read(&pool, target).await.unwrap().unwrap().markdown.as_deref(), Some("latest"));
        write(&pool, target, "", None, 2).await.unwrap();
        assert_eq!(read(&pool, target).await.unwrap().unwrap().markdown.as_deref(), Some(""));
    }
    #[tokio::test]
    async fn attachment_is_atomic_and_late_writes_follow_correct_meeting() {
        let pool = pool().await;
        write(&pool, "recording:/synthetic/a", "A", None, 0).await.unwrap();
        write(&pool, "recording:/synthetic/b", "B", None, 0).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO meetings VALUES ('a', '/synthetic/a', '2026')").execute(&mut *tx).await.unwrap();
        attach(&mut tx, Some("/synthetic/a"), "a").await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool, "recording:/synthetic/a").await.unwrap().unwrap().revision, 1);
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO meetings VALUES ('a', '/synthetic/a', '2026')").execute(&mut *tx).await.unwrap();
        attach(&mut tx, Some("/synthetic/a"), "a").await.unwrap();
        tx.commit().await.unwrap();
        write(&pool, "recording:/synthetic/a", "late A", None, 1).await.unwrap();
        assert_eq!(read(&pool, "meeting:a").await.unwrap().unwrap().markdown.as_deref(), Some("late A"));
        assert_eq!(read(&pool, "recording:/synthetic/b").await.unwrap().unwrap().markdown.as_deref(), Some("B"));
        let mut tx = pool.begin().await.unwrap();
        attach(&mut tx, Some("/synthetic/a"), "a").await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(read(&pool, "meeting:a").await.unwrap().unwrap().revision, 2);
    }
    #[tokio::test]
    async fn legacy_notes_survive_migration_and_can_be_edited() {
        let pool = pool().await;
        sqlx::query("INSERT INTO meetings VALUES ('old', NULL, '2025')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meeting_notes VALUES ('old', 'keep', NULL, '2025', '2025', 0)").execute(&pool).await.unwrap();
        assert_eq!(read(&pool, "meeting:old").await.unwrap().unwrap().markdown.as_deref(), Some("keep"));
        write(&pool, "meeting:old", "updated", None, 0).await.unwrap();
        assert_eq!(read(&pool, "meeting:old").await.unwrap().unwrap().revision, 1);
        assert!(write(&pool, "meeting:missing", "orphan", None, 0).await.is_err());
    }
}
