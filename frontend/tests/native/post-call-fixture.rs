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
    // Queue-only tests do not project into summary/task tables. They still
    // exercise the publication hook's absent-attempt no-op against its schema.
    let bridge = include_str!("../../src-tauri/migrations/20261118001801_post_call_projection.sql");
    sqlx::raw_sql(bridge.split("-- Keep the underlying").next().unwrap()).execute(pool).await.unwrap();
}
