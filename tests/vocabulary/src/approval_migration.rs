//! R1 exercises the real migration and repository reads with SQLite's default
//! REPLACE behavior: implicit conflict deletes do not fire delete triggers.
use crate::vocabulary::VocabularyRepository as V;
use sqlx::SqlitePool;

async fn apply_r1(pool: &SqlitePool) {
    sqlx::raw_sql(include_str!("../../../frontend/src-tauri/migrations/20261118001601_vocabulary_canonical_slot.sql")).execute(pool).await.unwrap();
}

async fn pool() -> SqlitePool {
    let pool = crate::tests::pool_01600().await;
    sqlx::query("PRAGMA recursive_triggers = OFF").execute(&pool).await.unwrap();
    let recursive: i64 = sqlx::query_scalar("PRAGMA recursive_triggers").fetch_one(&pool).await.unwrap();
    assert_eq!(recursive, 0);
    apply_r1(&pool).await;
    pool
}

async fn assert_reset(pool: &SqlitePool, old_revision: i64) -> i64 {
    let snapshot = V::approved_global(pool).await.unwrap();
    assert_eq!(snapshot.terms, "", "canonical mutation must invalidate approved_global");
    assert!(snapshot.revision > old_revision, "canonical mutation must change approved_global revision");
    snapshot.revision
}

async fn replacement_clears(value: Option<&str>) {
    let pool = pool().await;
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    let before = V::approved_global(&pool).await.unwrap();
    sqlx::query("INSERT OR REPLACE INTO transcript_settings(id,provider,model,whisperVocabulary) VALUES ('1','parakeet','synthetic',?)")
        .bind(value).execute(&pool).await.unwrap();
    let reset_revision = assert_reset(&pool, before.revision).await;
    assert_eq!(V::get_global(&pool).await.unwrap().as_deref(), value);
    assert!(V::replace_global_at_revision(&pool, value.unwrap_or_default(), before.revision, "stale").await.is_err());
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    let confirmed = V::approved_global(&pool).await.unwrap();
    assert_eq!(confirmed.terms, "OpenAPI");
    assert!(confirmed.revision > reset_revision);
    V::confirm_global(&pool, "openapi").await.unwrap();
    assert_eq!(V::approved_global(&pool).await.unwrap().revision, confirmed.revision);
}

#[tokio::test]
async fn replace_canonical_with_null_invalidates_actual_approved_global() { replacement_clears(None).await; }

#[tokio::test]
async fn replace_canonical_with_empty_invalidates_actual_approved_global() { replacement_clears(Some("")).await; }

#[tokio::test]
async fn replacing_canonical_with_equal_text_still_requires_new_confirmation() { replacement_clears(Some("OpenAPI")).await; }

#[tokio::test]
async fn moving_id_out_of_canonical_slot_resets_approval_and_moving_it_back_needs_confirmation() {
    let pool = pool().await;
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    let before = V::approved_global(&pool).await.unwrap();
    sqlx::query("UPDATE transcript_settings SET id = '2' WHERE id = '1'").execute(&pool).await.unwrap();
    let moved = assert_reset(&pool, before.revision).await;
    assert_eq!(V::get_global(&pool).await.unwrap(), None);
    sqlx::query("UPDATE transcript_settings SET id = '1' WHERE id = '2'").execute(&pool).await.unwrap();
    assert_reset(&pool, moved).await;
    assert_eq!(V::get_global(&pool).await.unwrap().as_deref(), Some("OpenAPI"));
}

#[tokio::test]
async fn moving_id_into_canonical_slot_invalidates_even_when_vocabulary_column_is_not_updated() {
    let pool = pool().await;
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    sqlx::query("DELETE FROM transcript_settings WHERE id = '1'").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO transcript_settings VALUES ('2','parakeet','synthetic','SRE')").execute(&pool).await.unwrap();
    let before = V::approved_global(&pool).await.unwrap();
    sqlx::query("UPDATE transcript_settings SET id = '1' WHERE id = '2'").execute(&pool).await.unwrap();
    assert_reset(&pool, before.revision).await;
    assert_eq!(V::get_global(&pool).await.unwrap().as_deref(), Some("SRE"));
}

#[tokio::test]
async fn update_or_replace_into_canonical_slot_invalidates_implicit_conflict_delete() {
    let pool = pool().await;
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    sqlx::query("INSERT INTO transcript_settings VALUES ('2','parakeet','synthetic',NULL)").execute(&pool).await.unwrap();
    let before = V::approved_global(&pool).await.unwrap();
    sqlx::query("UPDATE OR REPLACE transcript_settings SET id = '1' WHERE id = '2'").execute(&pool).await.unwrap();
    assert_reset(&pool, before.revision).await;
    assert_eq!(V::get_global(&pool).await.unwrap(), None);
}

#[tokio::test]
async fn unrelated_rows_and_canonical_noop_updates_do_not_change_approval_revision() {
    let pool = pool().await;
    V::confirm_global(&pool, "OpenAPI").await.unwrap();
    let before = V::approved_global(&pool).await.unwrap();
    for sql in [
        "INSERT INTO transcript_settings VALUES ('2','parakeet','synthetic','Unrelated')",
        "INSERT OR REPLACE INTO transcript_settings VALUES ('2','parakeet','synthetic',NULL)",
        "UPDATE transcript_settings SET whisperVocabulary = 'Other', id = '3' WHERE id = '2'",
        "DELETE FROM transcript_settings WHERE id = '3'",
        "UPDATE transcript_settings SET id = '1', whisperVocabulary = whisperVocabulary WHERE id = '1'",
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
        let current = V::approved_global(&pool).await.unwrap();
        assert_eq!(current.terms, "OpenAPI", "{sql}");
        assert_eq!(current.revision, before.revision, "{sql}");
    }
}

#[tokio::test]
async fn upgrade_clears_unverifiable_historical_proof_without_changing_legacy_asr_terms() {
    // All these histories can have identical old approval proofs. The final
    // canonical text alone cannot establish whether approval remained valid.
    for damage in [
        "",
        "INSERT OR REPLACE INTO transcript_settings VALUES ('1','parakeet','synthetic',NULL)",
        "UPDATE transcript_settings SET id = '2' WHERE id = '1'",
        "UPDATE transcript_settings SET id = '2' WHERE id = '1'; UPDATE transcript_settings SET id = '1' WHERE id = '2'",
    ] {
        let pool = crate::tests::pool_01600().await;
        sqlx::query("PRAGMA recursive_triggers = OFF").execute(&pool).await.unwrap();
        V::confirm_global(&pool, "OpenAPI").await.unwrap();
        sqlx::raw_sql(damage).execute(&pool).await.unwrap();
        let before = V::approved_global(&pool).await.unwrap();
        assert_eq!(before.terms, "OpenAPI", "01600 lacks reliable invalidation history");
        let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, whisperVocabulary FROM transcript_settings ORDER BY id")
            .fetch_all(&pool).await.unwrap();
        apply_r1(&pool).await;
        assert_reset(&pool, before.revision).await;
        let restored: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, whisperVocabulary FROM transcript_settings ORDER BY id")
            .fetch_all(&pool).await.unwrap();
        assert_eq!(restored, rows, "upgrade must not restore/remove/rewrite legacy ASR terms");
        V::confirm_global(&pool, "OpenAPI").await.unwrap();
        let confirmed = V::approved_global(&pool).await.unwrap();
        V::confirm_global(&pool, "OPENAPI").await.unwrap();
        assert_eq!(V::approved_global(&pool).await.unwrap().revision, confirmed.revision);
    }
}
