use std::collections::HashSet;

use chrono::Utc;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

pub const MAX_VOCABULARY_CHARS: usize = 1000;
static GLOBAL_VOCABULARY_WRITE_LOCK: Mutex<()> = Mutex::const_new(());
static MEETING_VOCABULARY_WRITE_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Clone, serde::Serialize)]
pub struct ApprovedGlobalVocabulary {
    pub revision: i64,
    pub terms: String,
}

#[derive(Debug, Clone)]
pub struct GlobalVocabularyState {
    pub global: String,
    pub approved_global: String,
    pub revision: i64,
}

pub struct VocabularyRepository;

/// Source identity belongs to the selected editable projection, not an archived
/// snapshot. All fields are checked under the dictionary's SQLite writer lock.
pub struct SuggestionSource<'a> {
    pub meeting_id: &'a str,
    pub version_id: &'a str,
    pub revision: i64,
    pub transcript_id: &'a str,
}

impl VocabularyRepository {
    pub fn normalize(raw: &str) -> Result<Option<String>, String> {
        if raw.chars().any(|c| c.is_control() && c != '\n' && c != '\r') {
            return Err("Vocabulary cannot contain control characters".to_string());
        }

        let mut seen = HashSet::new();
        let mut terms = Vec::new();
        for term in raw.split([',', '\n', '\r']) {
            let term = term.trim();
            if term.is_empty() {
                continue;
            }
            let key = term.to_lowercase();
            if seen.insert(key) {
                terms.push(term);
            }
        }

        if terms.is_empty() {
            return Ok(None);
        }

        let normalized = terms.join("\n");
        if normalized.chars().count() > MAX_VOCABULARY_CHARS {
            return Err(format!(
                "Vocabulary must be {} characters or fewer",
                MAX_VOCABULARY_CHARS
            ));
        }
        Ok(Some(normalized))
    }

    pub fn merge(primary: Option<&str>, secondary: Option<&str>) -> Option<String> {
        let combined = [primary, secondary]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");

        // Stored values are already validated. This path can exceed the
        // per-scope character limit when meeting and global terms are combined.
        let mut seen = HashSet::new();
        let terms = combined
            .split([',', '\n', '\r'])
            .map(str::trim)
            .filter(|term| !term.is_empty())
            .filter(|term| seen.insert(term.to_lowercase()))
            .collect::<Vec<_>>();
        (!terms.is_empty()).then(|| terms.join(", "))
    }

    /// Only proof-bearing terms enter Codex. Triggers invalidate proof when
    /// an older writer changes the legacy store outside confirmation/review.
    pub async fn global_state(pool: &SqlitePool) -> Result<GlobalVocabularyState, sqlx::Error> {
        let (global, approved_global, revision): (String, String, i64) = sqlx::query_as("SELECT COALESCE((SELECT whisperVocabulary FROM transcript_settings WHERE id = '1'), ''), approved_terms, revision FROM vocabulary_approval WHERE id = 1")
            .fetch_one(pool).await?;
        Ok(GlobalVocabularyState { global, approved_global, revision })
    }

    pub async fn approved_global(pool: &SqlitePool) -> Result<ApprovedGlobalVocabulary, sqlx::Error> {
        let (revision, terms): (i64, String) = sqlx::query_as("SELECT revision, approved_terms FROM vocabulary_approval WHERE id = 1")
            .fetch_one(pool).await?;
        Ok(ApprovedGlobalVocabulary { revision, terms })
    }

    pub async fn get_global(pool: &SqlitePool) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT whisperVocabulary FROM transcript_settings WHERE id = '1' LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
    }

    pub async fn save_global(
        pool: &SqlitePool,
        vocabulary: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let _guard = GLOBAL_VOCABULARY_WRITE_LOCK.lock().await;
        Self::save_global_unlocked(pool, vocabulary).await
    }

    async fn save_global_unlocked(
        pool: &SqlitePool,
        vocabulary: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO transcript_settings (id, provider, model, whisperVocabulary)
            VALUES ('1', 'parakeet', $1, $2)
            ON CONFLICT(id) DO UPDATE SET
                whisperVocabulary = excluded.whisperVocabulary
            "#,
        )
        .bind(crate::config::DEFAULT_PARAKEET_MODEL)
        .bind(vocabulary)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Confirmation is a separate operation from transcript correction. The
    /// first approved spelling wins on a case-insensitive retry.
    pub async fn confirm_global(pool: &SqlitePool, term: &str) -> Result<Option<String>, String> {
        let term = term.trim();
        if term.is_empty() || term.contains([',', '\n', '\r']) || term.chars().any(char::is_control) {
            return Err("Enter one term, name or abbreviation without separators or control characters".into());
        }
        Self::add_global(pool, term).await
    }

    /// Settings replacement must compare the actual loaded value, not a new
    /// read at Save time: otherwise a stale WebView can erase a confirmed term.
    pub async fn replace_global_at_revision(pool: &SqlitePool, expected: &str, revision: i64, raw: &str) -> Result<Option<String>, String> {
        let normalized = Self::normalize(raw)?;
        Self::update_global(pool, Some(expected), Some(revision), normalized.as_deref(), false, None).await
    }

    pub async fn confirm_suggestion(pool: &SqlitePool, term: &str, source: SuggestionSource<'_>) -> Result<Option<String>, String> {
        if term.is_empty() || term.chars().count() > 64 || !term.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err("Invalid suggestion spelling".into());
        }
        Self::update_global(pool, None, None, Some(term), true, Some(source)).await
    }

    async fn update_global(pool: &SqlitePool, expected: Option<&str>, expected_revision: Option<i64>, raw: Option<&str>, append: bool, source: Option<SuggestionSource<'_>>) -> Result<Option<String>, String> {
        let _guard = GLOBAL_VOCABULARY_WRITE_LOCK.lock().await;
        // Reserve SQLite's writer before reading, including across processes.
        // Neither overflow nor stale replacement can partially change the store.
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.map_err(|e| e.to_string())?;
        if let Some(source) = source {
            let text: Option<String> = sqlx::query_scalar("SELECT substr(t.transcript, 1, 100001) FROM transcripts t JOIN transcript_selection s ON s.meeting_id = t.meeting_id WHERE t.meeting_id = ? AND t.id = ? AND s.version_id = ? AND s.revision = ?")
                .bind(source.meeting_id).bind(source.transcript_id).bind(source.version_id).bind(source.revision)
                .fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
            // Match a whole exact source token: neither case-folded guesses nor
            // substrings of a longer spelling constitute confirmation evidence.
            if !text.is_some_and(|text| crate::database::vocabulary_suggestions::bounded_text(&text, 100000).0.split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .any(|token| token.trim_matches(['-', '_']) == raw.unwrap_or_default())) {
                return Err("Suggestion source changed. Find terms again in the current selected transcript; nothing was added.".into());
            }
        }
        let (approval_revision, previously_approved): (i64, String) = sqlx::query_as("SELECT revision, approved_terms FROM vocabulary_approval WHERE id = 1")
            .fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
        let existing: Option<String> = sqlx::query_scalar("SELECT whisperVocabulary FROM transcript_settings WHERE id = '1'")
            .fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?.flatten();
        if expected.is_some_and(|value| value != existing.as_deref().unwrap_or_default()) || expected_revision.is_some_and(|r| r != approval_revision) {
            return Err("Vocabulary changed in another window. Reload the saved vocabulary and reapply your edits; nothing was replaced.".into());
        }
        let normalized = if append {
            let combined = Self::normalize(&[existing.as_deref(), raw].into_iter().flatten().collect::<Vec<_>>().join("\n"))?;
            let confirmed_keys: HashSet<_> = previously_approved.lines().map(str::to_lowercase).collect();
            let incoming = Self::normalize(raw.unwrap_or_default())?;
            // The first *approved* spelling wins. A matching legacy hint has
            // no proof, so its casing must not override the user's confirmation.
            let terms = combined.as_deref().unwrap_or_default().lines().map(|term| {
                if confirmed_keys.contains(&term.to_lowercase()) { return term; }
                incoming.as_deref().unwrap_or_default().lines()
                    .find(|candidate| candidate.to_lowercase() == term.to_lowercase())
                    .unwrap_or(term)
            }).collect::<Vec<_>>().join("\n");
            Self::normalize(&terms)?
        } else { raw.map(str::to_owned) };
        // An append approves only the submitted terms, never other legacy
        // entries. Explicit Settings Save is review of the whole submitted list.
        let approved = if append {
            let keys: HashSet<String> = previously_approved.split('\n')
                .chain(raw.unwrap_or_default().split([',', '\n', '\r']))
                .map(|t| t.trim().to_lowercase()).collect();
            normalized.as_deref().unwrap_or_default().split('\n')
                .filter(|t| !t.is_empty() && keys.contains(&t.to_lowercase())).collect::<Vec<_>>().join("\n")
        } else { normalized.clone().unwrap_or_default() };
        sqlx::query("INSERT INTO transcript_settings (id, provider, model, whisperVocabulary) VALUES ('1', 'parakeet', ?, ?) ON CONFLICT(id) DO UPDATE SET whisperVocabulary = excluded.whisperVocabulary")
            .bind(crate::config::DEFAULT_PARAKEET_MODEL).bind(normalized.as_deref())
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
        let changed = existing != normalized || previously_approved != approved;
        sqlx::query("UPDATE vocabulary_approval SET approved_terms = ?, revision = ? WHERE id = 1")
            .bind(&approved).bind(approval_revision + i64::from(changed))
            .execute(&mut *tx).await.map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(normalized)
    }

    pub async fn add_global(pool: &SqlitePool, raw: &str) -> Result<Option<String>, String> {
        Self::update_global(pool, None, None, Some(raw), true, None).await
    }

    pub async fn get_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT vocabulary FROM meeting_whisper_vocabulary WHERE meeting_id = ? LIMIT 1",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn add_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
        raw: &str,
    ) -> Result<Option<String>, String> {
        let _guard = MEETING_VOCABULARY_WRITE_LOCK.lock().await;
        let existing = Self::get_meeting(pool, meeting_id)
            .await
            .map_err(|error| error.to_string())?;
        let combined = [existing.as_deref(), Some(raw)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        let normalized = Self::normalize(&combined)?;

        if let Some(vocabulary) = normalized.as_deref() {
            sqlx::query(
                r#"
                INSERT INTO meeting_whisper_vocabulary (meeting_id, vocabulary, updated_at)
                VALUES ($1, $2, $3)
                ON CONFLICT(meeting_id) DO UPDATE SET
                    vocabulary = excluded.vocabulary,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(meeting_id)
            .bind(vocabulary)
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
        }
        Ok(normalized)
    }

    pub async fn save_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
        raw: &str,
    ) -> Result<Option<String>, String> {
        let _guard = MEETING_VOCABULARY_WRITE_LOCK.lock().await;
        let normalized = Self::normalize(raw)?;
        match normalized.as_deref() {
            Some(vocabulary) => {
                sqlx::query(
                    r#"
                    INSERT INTO meeting_whisper_vocabulary (meeting_id, vocabulary, updated_at)
                    VALUES ($1, $2, $3)
                    ON CONFLICT(meeting_id) DO UPDATE SET
                        vocabulary = excluded.vocabulary,
                        updated_at = excluded.updated_at
                    "#,
                )
                .bind(meeting_id)
                .bind(vocabulary)
                .bind(Utc::now().to_rfc3339())
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
            }
            None => {
                sqlx::query("DELETE FROM meeting_whisper_vocabulary WHERE meeting_id = ?")
                    .bind(meeting_id)
                    .execute(pool)
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(normalized)
    }

    pub async fn get_effective(
        pool: &SqlitePool,
        meeting_id: Option<&str>,
    ) -> Result<Option<String>, sqlx::Error> {
        let global = Self::get_global(pool).await?;
        let meeting = match meeting_id {
            Some(meeting_id) => Self::get_meeting(pool, meeting_id).await?,
            None => None,
        };
        Ok(Self::merge(meeting.as_deref(), global.as_deref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_deduplicates_terms() {
        assert_eq!(
            VocabularyRepository::normalize(" Meetily, Tauri\nmeetily \r\n OKR ")
                .unwrap()
                .as_deref(),
            Some("Meetily\nTauri\nOKR")
        );
    }

    #[test]
    fn meeting_terms_are_prioritized_when_merged() {
        assert_eq!(
            VocabularyRepository::merge(
                Some("Project Phoenix\nMeetily"),
                Some("meetily\nTauri")
            )
            .as_deref(),
            Some("Project Phoenix, Meetily, Tauri")
        );
    }

    #[test]
    fn rejects_invalid_or_oversized_values() {
        assert!(VocabularyRepository::normalize("bad\0term").is_err());
        assert!(VocabularyRepository::normalize(&"a".repeat(MAX_VOCABULARY_CHARS + 1)).is_err());
    }
}
