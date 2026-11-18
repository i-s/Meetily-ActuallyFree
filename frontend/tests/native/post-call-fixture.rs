//! Queue-only schema fixtures; native integration uses the complete migrations.
pub async fn queue_schema(pool: &sqlx::SqlitePool) {
    sqlx::raw_sql(include_str!(
        "../../src-tauri/migrations/20261118001500_post_call_queue.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../src-tauri/migrations/20261118001501_post_call_vocabulary.sql"
    ))
    .execute(pool)
    .await
    .unwrap();
}
