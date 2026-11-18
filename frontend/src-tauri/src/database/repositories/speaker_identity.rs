//! Shared normalized speaker/contact reconciliation, also used by version selection.
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

pub(crate) async fn ensure_label_identity(
    tx: &mut Transaction<'_, Sqlite>,
    meeting_id: &str,
    label: &str,
) -> Result<(), sqlx::Error> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT person_id FROM person_speakers WHERE meeting_id = ? AND speaker_label = ?",
    )
    .bind(meeting_id)
    .bind(label)
    .fetch_optional(&mut **tx)
    .await?;
    if existing.is_some() {
        return Ok(());
    }
    let normalized = normalize_person_name(label);
    let person_id = match find_person_by_normalized_name(tx, &normalized).await? {
        Some(id) => id,
        None => {
            let id = format!("person-{}", Uuid::new_v4());
            sqlx::query(
                "INSERT INTO people \
                 (id, display_name, normalized_name, notes, created_at, updated_at) \
                 VALUES (?, ?, ?, NULL, datetime('now'), datetime('now'))",
            )
            .bind(&id)
            .bind(label)
            .bind(&normalized)
            .execute(&mut **tx)
            .await?;
            id
        }
    };
    sqlx::query(
        "INSERT INTO person_speakers (person_id, meeting_id, speaker_label) \
         VALUES (?, ?, ?) \
         ON CONFLICT(meeting_id, speaker_label) DO UPDATE SET person_id = excluded.person_id",
    )
    .bind(person_id)
    .bind(meeting_id)
    .bind(label)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) fn normalize_person_name(name: &str) -> String {
    name.trim().to_lowercase()
}

pub(crate) fn is_person_name(name: &str) -> bool {
    let trimmed = name.trim();
    let lower = trimmed.to_ascii_lowercase();
    if trimmed.is_empty()
        || matches!(
            lower.as_str(),
            "you" | "guest" | "mic" | "microphone" | "system" | "system audio" | "speaker"
        )
        || lower.starts_with("speaker ")
        || trimmed.contains(" + ")
    {
        return false;
    }
    true
}

pub(crate) async fn find_person_by_normalized_name(
    tx: &mut Transaction<'_, Sqlite>,
    normalized: &str,
) -> Result<Option<String>, sqlx::Error> {
    if let Some(id) = sqlx::query_scalar("SELECT id FROM people WHERE normalized_name = ?")
        .bind(normalized)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(Some(id));
    }

    let candidates = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, display_name, normalized_name FROM people",
    )
    .fetch_all(&mut **tx)
    .await?;
    Ok(candidates
        .into_iter()
        .find(|(_, display_name, stored_normalized)| {
            normalize_person_name(display_name) == normalized
                || normalize_person_name(stored_normalized) == normalized
        })
        .map(|(id, _, _)| id))
}
