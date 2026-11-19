//! Historical fixtures are immutable SQL, not invented migration receipts.
//! Both app-native and portable runs use SQLx's real Migrator on disk databases.
use super::{DatabaseManager, MIGRATOR};
use sqlx::{
    migrate::{MigrateError, Migration, MigrationType, Migrator},
    sqlite::SqliteConnectOptions,
    SqlitePool,
};
use std::{borrow::Cow, path::Path};

const HISTORICAL: &[(i64, &str, &str)] = &[
    (
        20260930000000,
        "add codex cli path",
        include_str!(
            "../../../tests/fixtures/migration-lineage/20260930000000_add_codex_cli_path.sql"
        ),
    ),
    (
        20261026020000,
        "persistent meeting chat",
        include_str!(
            "../../../tests/fixtures/migration-lineage/20261026020000_persistent_meeting_chat.sql"
        ),
    ),
    (
        20261026030000,
        "summary input snapshot",
        include_str!(
            "../../../tests/fixtures/migration-lineage/20261026030000_summary_input_snapshot.sql"
        ),
    ),
];

fn is_restored(version: i64) -> bool {
    HISTORICAL.iter().any(|m| m.0 == version)
}
fn migrator(migrations: Vec<Migration>) -> Migrator {
    Migrator {
        migrations: Cow::Owned(migrations),
        ..Migrator::DEFAULT
    }
}
fn historical(through: i64) -> Migrator {
    let mut migrations: Vec<_> = MIGRATOR
        .iter()
        .filter(|m| m.version < 20260930000000)
        .cloned()
        .collect();
    migrations.extend(
        HISTORICAL
            .iter()
            .filter(|m| m.0 <= through)
            .map(|(v, name, sql)| {
                Migration::new(
                    *v,
                    (*name).into(),
                    MigrationType::Simple,
                    (*sql).into(),
                    false,
                )
            }),
    );
    migrator(migrations)
}
fn prior_candidate() -> Migrator {
    migrator(
        MIGRATOR
            .iter()
            .filter(|m| !is_restored(m.version))
            .cloned()
            .collect(),
    )
}
async fn open(path: &Path) -> SqlitePool {
    SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await
    .unwrap()
}
async fn seed(pool: &SqlitePool) {
    for sql in [
        "INSERT INTO meetings(id,title,created_at,updated_at) VALUES('generated','Owned fixture','2026','2026')",
        "INSERT INTO transcripts(id,meeting_id,transcript,timestamp,speaker,audio_start_time,audio_end_time) VALUES('t','generated','Тест / owned words','2026','Microphone',1.0,2.0)",
        "INSERT INTO meeting_notes(meeting_id,notes_markdown,created_at,updated_at) VALUES('generated','Personal thought, not agreement','2026','2026')",
        "INSERT INTO summary_processes(meeting_id,status,created_at,updated_at,result,user_edited) VALUES('generated','completed','2026','2026','{\"summary\":\"Owned result\"}',1)",
        "INSERT INTO settings(id,provider,model,whisperModel) VALUES('1','local','fixture','fixture')",
    ] { sqlx::query(sql).execute(pool).await.unwrap(); }
}
async fn preserved(pool: &SqlitePool) {
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT transcript FROM transcripts WHERE id='t'")
            .fetch_one(pool)
            .await
            .unwrap(),
        "Тест / owned words"
    );
    assert_eq!(
        sqlx::query_as::<_, (String, f64, f64)>(
            "SELECT speaker,audio_start_time,audio_end_time FROM transcripts WHERE id='t'"
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        ("Microphone".into(), 1.0, 2.0)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT notes_markdown FROM meeting_notes WHERE meeting_id='generated'"
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        "Personal thought, not agreement"
    );
    assert_eq!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT result,user_edited FROM summary_processes WHERE meeting_id='generated'"
        )
        .fetch_one(pool)
        .await
        .unwrap(),
        ("{\"summary\":\"Owned result\"}".into(), 1)
    );
}
async fn receipts(pool: &SqlitePool) -> Vec<(i64, Vec<u8>, bool)> {
    sqlx::query_as("SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}
async fn upgrade(through: i64) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("generated.sqlite");
    let pool = open(&path).await;
    historical(through).run(&pool).await.unwrap();
    seed(&pool).await;
    sqlx::query("UPDATE settings SET codexCliPath='/generated/codex'")
        .execute(&pool)
        .await
        .unwrap();
    if through == 20261026030000 {
        sqlx::query("INSERT INTO meeting_chat_turns(id,meeting_id,question,answer,status,created_at,context_json) VALUES('chat','generated','Owned question','Owned answer','completed','2026','{}')").execute(&pool).await.unwrap();
        sqlx::query("UPDATE summary_processes SET input_snapshot='owned input',pending_input_snapshot='owned pending'").execute(&pool).await.unwrap();
    }
    let before = receipts(&pool).await;
    pool.close().await;
    let pool = open(&path).await;
    assert!(matches!(
        prior_candidate().run(&pool).await,
        Err(MigrateError::VersionMissing(20260930000000))
    ));
    pool.close().await;
    let manager = DatabaseManager::new(
        path.to_str().unwrap(),
        folder.path().join("absent-backend.db").to_str().unwrap(),
    )
    .await
    .unwrap();
    let pool = manager.pool().clone();
    preserved(&pool).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT codexCliPath FROM settings WHERE id='1'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "/generated/codex"
    );
    if through == 20261026030000 {
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT answer FROM meeting_chat_turns WHERE id='chat'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "Owned answer"
        );
        assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT input_snapshot,pending_input_snapshot FROM summary_processes WHERE meeting_id='generated'").fetch_one(&pool).await.unwrap(),("owned input".into(),"owned pending".into()));
    }
    let after = receipts(&pool).await;
    for receipt in before {
        assert!(after.contains(&receipt), "historical receipt changed");
    }
    assert_eq!(after.len(), MIGRATOR.iter().count());
    MIGRATOR.run(&pool).await.unwrap();
    pool.close().await;
    let pool = open(&path).await;
    MIGRATOR.run(&pool).await.unwrap();
    preserved(&pool).await;
    pool.close().await;
}

#[tokio::test]
async fn migration_lineage_codex_history_upgrade() {
    upgrade(20260930000000).await;
}
#[tokio::test]
async fn migration_lineage_chat_snapshot_history_upgrade() {
    upgrade(20261026030000).await;
}

#[tokio::test]
async fn migration_lineage_fresh_and_reopen() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("fresh.sqlite");
    let pool = open(&path).await;
    MIGRATOR.run(&pool).await.unwrap();
    seed(&pool).await;
    assert_eq!(receipts(&pool).await.len(), MIGRATOR.iter().count());
    pool.close().await;
    let pool = open(&path).await;
    MIGRATOR.run(&pool).await.unwrap();
    preserved(&pool).await;
    pool.close().await;
}

#[tokio::test]
async fn migration_lineage_later_candidate_backfills_without_collisions() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("candidate.sqlite");
    let pool = open(&path).await;
    prior_candidate().run(&pool).await.unwrap();
    seed(&pool).await;
    sqlx::query("INSERT INTO post_call_jobs(meeting_id,status) VALUES('generated','deferred')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO transcript_versions(id,meeting_id,kind,status,created_at,segments_json) VALUES('v','generated','live','ready','2026','[]')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO transcript_selection(meeting_id,version_id,revision) VALUES('generated','v',7)").execute(&pool).await.unwrap();
    let token: String = sqlx::query_scalar(
        "SELECT action_sync_token FROM summary_processes WHERE meeting_id='generated'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let before = receipts(&pool).await;
    pool.close().await;
    let pool = open(&path).await;
    MIGRATOR.run(&pool).await.unwrap();
    preserved(&pool).await;
    for receipt in before {
        assert!(receipts(&pool).await.contains(&receipt));
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM post_call_jobs WHERE meeting_id='generated'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "deferred"
    );
    assert_eq!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT version_id,revision FROM transcript_selection WHERE meeting_id='generated'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        ("v".into(), 7)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT action_sync_token FROM summary_processes WHERE meeting_id='generated'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        token
    );
    assert_eq!(receipts(&pool).await.len(), MIGRATOR.iter().count());
    MIGRATOR.run(&pool).await.unwrap();
    pool.close().await;
}

#[tokio::test]
async fn migration_lineage_checksum_mismatch_remains_strict() {
    // Produce a different checksum through SQLx applying different input bytes;
    // never forge/update a receipt, even in this generated fixture.
    for version in HISTORICAL.iter().map(|m| m.0) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("mismatch.sqlite");
        let pool = open(&path).await;
        let mut legacy = historical(20261026030000);
        for m in legacy.migrations.to_mut() {
            if m.version == version {
                *m = Migration::new(
                    m.version,
                    m.description.clone(),
                    m.migration_type,
                    format!("{}\n-- different historical bytes\n", m.sql).into(),
                    false,
                );
            }
        }
        legacy.run(&pool).await.unwrap();
        seed(&pool).await;
        let before = receipts(&pool).await;
        pool.close().await;
        let pool = open(&path).await;
        assert!(
            matches!(MIGRATOR.run(&pool).await,Err(MigrateError::VersionMismatch(v)) if v==version)
        );
        assert_eq!(receipts(&pool).await, before);
        preserved(&pool).await;
        pool.close().await;
    }
}

#[test]
fn migration_lineage_authentic_checksums_and_unique_versions() {
    let expected = [
        "29b92447c1c8739f9ecdb47613ca700f183b9c10d6c7e7231e4d621dec89adb931023eb9962b5f4fd3cfa2f4fcbb53db",
        "534a9886d92403c210248e25f9531b4706efb550c66d216ff95da9f5ae7386a038f79fe09f423fdbd1ef771b9ce6420a",
        "aa22c0aea2c0433fd3898e3cdfafe9677ff639cd94b3e22ef7284eeaea11d93a85d9a448c2fc91d0705c371d012d41ce",
    ];
    assert!(!MIGRATOR.ignore_missing);
    let mut previous = 0;
    for migration in MIGRATOR.iter() {
        assert!(
            migration.version > previous,
            "duplicate or unordered migration version"
        );
        previous = migration.version;
    }
    for ((version, _, sql), checksum) in HISTORICAL.iter().zip(expected) {
        let migration = MIGRATOR
            .iter()
            .find(|m| m.version == *version)
            .expect("authentic migration missing");
        assert_eq!(migration.sql.as_ref(), *sql);
        let actual: String = migration
            .checksum
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(actual, checksum);
    }
}

#[tokio::test]
async fn migration_lineage_unknown_version_still_fails_closed() {
    let folder = tempfile::tempdir().unwrap();
    let pool = open(&folder.path().join("unknown.sqlite")).await;
    let mut unsupported = historical(20261026030000);
    unsupported.migrations.to_mut().push(Migration::new(
        20261118009999,
        "unsupported".into(),
        MigrationType::Simple,
        "SELECT 1;".into(),
        false,
    ));
    unsupported.run(&pool).await.unwrap();
    seed(&pool).await;
    let before = receipts(&pool).await;
    assert!(matches!(
        MIGRATOR.run(&pool).await,
        Err(MigrateError::VersionMissing(20261118009999))
    ));
    assert_eq!(receipts(&pool).await, before);
    preserved(&pool).await;
    pool.close().await;
}
