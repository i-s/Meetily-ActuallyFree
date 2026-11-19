# Issue #20 — preserve the installed SQLx migration lineage

Sanitized installed-startup evidence: `lib.rs:613` startup expectation failed with
`sqlx::migrate::MigrateError::VersionMissing(20260930000000)`. No owner database,
path, recording or credential was accessed. This is a source-lineage repair,
not permission to patch database receipts, ignore missing migrations or bypass
checksum validation. Physical installation/relaunch of a corrected DMG is pending.

## Exact supported-history audit (2026-11-19)

The candidate at `94371863eab27c4cf4790d4e9ed7eb2c8053caff` retained 19 common
historical migrations and nine later #5–19 migrations, but dropped three installed
fork migrations. Audit compared every SQL byte, version and Git blob against:

- `d9cb04f4ff9bc2560be30853863e3c89dd713102`: 20 migrations, all 19 common files
  identical; includes the Codex path migration below.
- `origin/fix/codex-call-detection` at
  `2169cabcac80a22f5a8fea6c54d13e8b971bb261`: 22 migrations, all 19 common files
  identical; all three missing files below.
- `origin/feat/meeting-notes-persistent-chat` at
  `791b1448e5a83992084b9ac205e8efb83ef3da5e`: same 22 migration blobs as the preceding
  history, including all three missing files.

There are **no conflicting overlapping versions or differing common SQL bytes**
on these supported histories. Restore the following files byte-for-byte (LF and
final newline included), without modifying any already-present migration. Explicit
LF attributes cover only the three restored files and their historical fixtures;
no older migration is retroactively normalized:

| Version / file | Authentic Git blob | SQLx SHA-384 checksum |
| --- | --- | --- |
| `20260930000000_add_codex_cli_path.sql` | `281ef523379fdfa5fa05324a19b9dc7391133751` | `29b92447c1c8739f9ecdb47613ca700f183b9c10d6c7e7231e4d621dec89adb931023eb9962b5f4fd3cfa2f4fcbb53db` |
| `20261026020000_persistent_meeting_chat.sql` | `80b108b0a50233382d2aca9343bd638887b80a83` | `534a9886d92403c210248e25f9531b4706efb550c66d216ff95da9f5ae7386a038f79fe09f423fdbd1ef771b9ce6420a` |
| `20261026030000_summary_input_snapshot.sql` | `969272af232c62b47506109533d206c92b7bc25c` | `aa22c0aea2c0433fd3898e3cdfafe9677ff639cd94b3e22ef7284eeaea11d93a85d9a448c2fc91d0705c371d012d41ce` |

The restored SQL adds `settings.codexCliPath`, historical `meeting_chat_turns` and
`ask_ai_context_settings` tables/indexes/delete trigger, and separate displayed /
pending input-snapshot columns on `summary_processes`. These names do not collide
with the candidate's Notes revision, transcript versions, post-call queue,
vocabulary or canonical projection migrations. The historical chat table is not
a replacement for the new Codex runner; restoring persistence does not enable a
new AI provider, import old chat into new context, or grant network consent.

## Startup and upgrade behavior

`database/manager.rs` still embeds the full migration directory with SQLx's
`migrate!`. Startup runs its existing strict Migrator; this fix changes no runtime
validation or repair logic. The pre-existing tightly guarded people-migration
LF/CRLF compatibility path is unchanged and is not invoked for the restored
migration versions. No new receipt update or missing-version ignore policy exists.

- Genuine legacy receipt sets now find the exact known SQL/checksums, retain
  their historical rows/receipts, and apply the later migrations normally.
- Fresh DBs apply all 31 migrations in order.
- A previous candidate DB may already contain the nine later migrations while
  lacking these three older ones. SQLx 0.8.6 applies missing versions individually,
  not merely versions above the latest receipt. The real Migrator test verifies
  this backfill, including non-collision and preservation of later state.
- Unknown versions and wrong bytes still fail closed. Do not repair an actual
  mismatched DB by editing `_sqlx_migrations` or using `ignore_missing`; report
  the sanitized error and retain the data for a separately scoped investigation.

## Regression evidence and reproducible commands

`database/migration_lineage_tests.rs` is compiled under the production database
manager and uses its actual static Migrator. The immutable historical test SQL
fixtures carry the authentic checksums above. All DBs are generated temporary
**on-disk** SQLite files, created/migrated by SQLx, closed and reopened between
upgrade phases. No hand-written receipts, Python executescript stand-in, or
in-memory-only upgrade is used.

Seven tests cover:

1. Exact restored SQL/checksums, ordered unique versions, strict missing policy.
2. Authentic Codex-only historical lineage with legacy transcript/timestamps/
   source labels, Notes, edited summary and saved CLI-path sentinel; production
   `DatabaseManager::new` upgrade, repeated migration and reopen preserve them.
3. Authentic chat/snapshot lineage with additional chat and both summary-input
   sentinels; same production startup upgrade and persistence checks.
4. Fresh full schema and reopen.
5. Previous candidate carrying later versions: missing older versions backfill,
   all existing receipts and transcript/Notes/summary data remain, and deferred
   queue, selected version/revision and summary ownership token survive.
6. For each restored version, SQLx applies a fixture with different SQL bytes to
   generate a genuine differing checksum; corrected Migrator refuses it with
   `VersionMismatch(version)` and leaves receipts/data unchanged.
7. A genuine SQLx-applied unsupported version remains `VersionMissing`; no pending
   migration or receipt/data mutation occurs.

Before restoring the files, the new real-Migrator regression reproduced the
reported `VersionMissing(20260930000000)` for both historical upgrade paths.
After restoration, all seven pass. Portable runner:

```sh
# Python >=3.11, Rust/Cargo and the workspace-lock dependency set required.
python3 -I -B frontend/scripts/test-migration-lineage.py --offline
```

Omit `--offline` only to obtain required public Cargo dependencies. The runner
uses a temporary minimal manifest, retains workspace-pinned versions, verifies
its entire resolved dependency set against `Cargo.lock`, and tests with
`--locked`. It compiles the **production manager**, stubbing only unused Tauri
AppHandle/path entry points; use of desktop paths is a panic, not owner access.
No frontend/audio/GTK build is needed. `CARGO_TARGET_DIR` may select an explicit
local cache; that cache is not an authoritative portable test environment.

macOS workflow addition, within the existing sequential native regression block:

```sh
cargo test --locked -p meetily --lib --target "$TARGET" migration_lineage
```

It runs the same seven tests against the normal app module graph on Apple Silicon
before packaging, and retains output in the existing native-test log. No trigger,
permission, publisher, remote source pin or workflow run was changed/launched by
this repair. Coordinator must repin the corrected source SHA/tree **and include
the added workflow command** before the one corrected artifact build. Full desktop
compilation and owner installed-startup evidence remain pending that build/test;
portable SQLx success alone is not physical acceptance.
