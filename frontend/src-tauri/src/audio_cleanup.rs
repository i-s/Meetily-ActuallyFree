//! Audio-only retention. SQLite and the existing native operation/lifecycle
//! admission fence app-owned readers; no text, metadata or queue row is mutated.
use serde::Serialize;
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

const EXTENSIONS: &[&str] = &[
    "mp4", "m4a", "mp3", "wav", "aac", "flac", "ogg", "webm", "mkv", "wma",
];
const PLAN_LIFETIME: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFile {
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    pub meeting_id: String,
    pub title: String,
    pub created_at: String,
    pub files: Vec<AudioFile>,
    pub bytes: u64,
    pub blocked: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub total_bytes: u64,
    pub usage_complete: bool,
    pub recordings: Vec<Recording>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Consent {
    pub token: String,
    pub recordings: Vec<Recording>,
    pub bytes: u64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutcome {
    pub meeting_id: String,
    pub name: String,
    pub status: String,
    pub removed_bytes: u64,
    pub error: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub files: Vec<FileOutcome>,
    pub removed_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct Identity {
    bytes: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}
fn identity(path: &Path) -> Result<Identity, String> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.file_type().is_symlink() {
        return Err("Not an ordinary owned audio file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if m.nlink() != 1 {
            return Err("Linked files are not eligible for cleanup".into());
        }
    }
    Ok(Identity {
        bytes: m.len(),
        modified: m.modified().ok(),
        #[cfg(unix)]
        dev: {
            use std::os::unix::fs::MetadataExt;
            m.dev()
        },
        #[cfg(unix)]
        ino: {
            use std::os::unix::fs::MetadataExt;
            m.ino()
        },
        #[cfg(unix)]
        changed: {
            use std::os::unix::fs::MetadataExt;
            (m.ctime(), m.ctime_nsec())
        },
    })
}
fn owned_dir(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Recording folder must be absolute".into());
    }
    let mut part = PathBuf::new();
    for c in path.components() {
        if matches!(c, Component::ParentDir) {
            return Err("Ambiguous recording folder".into());
        }
        part.push(c);
        let m = fs::symlink_metadata(&part).map_err(|e| e.to_string())?;
        if m.file_type().is_symlink() || !m.is_dir() {
            return Err("Linked or non-directory recording folder".into());
        }
    }
    fs::canonicalize(path).map_err(|e| e.to_string())
}
// Count observed audio in the saved folder, including protected checkpoints.
// This is a logical byte count, not allocated blocks or promised disk space.
fn measured_audio_bytes(dir: &Path, checkpoint_depth: Option<u8>) -> Result<u64, String> {
    owned_dir(dir)?;
    let mut bytes = 0u64;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err("Linked entries prevent complete audio usage measurement".into());
        }
        if meta.is_dir() && entry.file_name() == ".checkpoints" && checkpoint_depth.is_none() {
            bytes = bytes.saturating_add(measured_audio_bytes(&path, Some(0))?);
        } else if meta.is_dir() && checkpoint_depth.is_some() {
            if checkpoint_depth == Some(1) {
                return Err("Unexpected nested recovery directory".into());
            }
            bytes = bytes.saturating_add(measured_audio_bytes(&path, Some(1))?);
        } else if meta.is_file()
            && path.extension().and_then(|x| x.to_str()).is_some_and(|x| {
                x == "pending" || EXTENSIONS.contains(&x.to_ascii_lowercase().as_str())
            })
        {
            bytes = bytes.saturating_add(meta.len());
        }
    }
    Ok(bytes)
}
#[derive(Clone)]
struct Snapshot {
    recording: Recording,
    dir: PathBuf,
    metadata: Vec<u8>,
    identities: Vec<(String, Identity)>,
    usage_complete: bool,
}
struct Plan {
    token: String,
    created: Instant,
    snapshots: Vec<Snapshot>,
}
#[derive(Default)]
pub struct Plans(Mutex<VecDeque<Plan>>);

async fn inspect(
    conn: &mut SqliteConnection,
    selected: Option<&HashSet<String>>,
) -> Result<Vec<Snapshot>, String> {
    let rows = sqlx::query("SELECT id,title,created_at,folder_path FROM meetings WHERE folder_path IS NOT NULL ORDER BY created_at,id")
        .fetch_all(&mut *conn).await.map_err(|e|e.to_string())?;
    // Include aliases even if their spelling differs. A folder is never owned
    // exclusively when another meeting points into it (or its parent).
    let folders: Vec<_> = rows
        .iter()
        .filter_map(|r| {
            r.get::<Option<String>, _>("folder_path")
                .map(|s| (r.get::<String, _>("id"), PathBuf::from(s)))
        })
        .collect();
    let folders = std::sync::Arc::new(folders);
    let has_queue: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='post_call_jobs')",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let has_versions: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='transcript_versions')").fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
    let mut snapshots = Vec::new();
    for row in rows {
        let id: String = row.get("id");
        if selected.is_some_and(|ids| !ids.contains(&id)) {
            continue;
        }
        let path = PathBuf::from(row.get::<String, _>("folder_path"));
        let mut s = Snapshot {
            recording: Recording {
                meeting_id: id.clone(),
                title: row.get("title"),
                created_at: row.get("created_at"),
                files: vec![],
                bytes: 0,
                blocked: None,
            },
            dir: path.clone(),
            metadata: vec![],
            identities: vec![],
            usage_complete: false,
        };
        let folders = folders.clone();
        let inspection_id = id.clone();
        let mut s = tokio::task::spawn_blocking(move || {
            let id = inspection_id;
            let check = (|| -> Result<(), String> {
                s.dir = owned_dir(&path)?;
                s.recording.bytes = measured_audio_bytes(&s.dir, None)?;
                s.usage_complete = true;
                if folders.iter().any(|(other, p)| {
                    other != &id
                        && fs::canonicalize(p)
                            .ok()
                            .is_some_and(|p| s.dir.starts_with(&p) || p.starts_with(&s.dir))
                }) {
                    return Err("Recording folder is shared with another meeting".into());
                }
                // Never traverse: recovery checkpoints and unknown audio are not an
                // implicit extension of the user's consent to delete final tracks.
                for entry in fs::read_dir(&s.dir).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name == ".checkpoints" || name.ends_with(".pending") {
                        return Err(
                            "Recovery/checkpoint material is protected; finish recovery first"
                                .into(),
                        );
                    }
                    let p = entry.path();
                    if p.extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| EXTENSIONS.contains(&x.to_ascii_lowercase().as_str()))
                    {
                        let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or_default();
                        if !["audio", "mic", "system"].contains(&stem) {
                            return Err(format!(
                                "Unrecognized audio {name}; cleanup will not guess ownership"
                            ));
                        }
                        let ident = identity(&p)?;
                        s.recording.files.push(AudioFile {
                            name: name.clone(),
                            bytes: ident.bytes,
                        });
                        s.identities.push((name, ident));
                    }
                }
                s.recording.files.sort_by(|a, b| a.name.cmp(&b.name));
                s.identities.sort_by(|a, b| a.0.cmp(&b.0));
                let meta = s.dir.join("metadata.json");
                identity(&meta)?;
                s.metadata = fs::read(meta).map_err(|e| e.to_string())?;
                let metadata: serde_json::Value = serde_json::from_slice(&s.metadata)
                    .map_err(|e| format!("Invalid recording metadata: {e}"))?;
                if metadata.get("status").and_then(|v| v.as_str()) != Some("completed") {
                    return Err("Unfinished recording/recovery is protected".into());
                }
                if let Some(owner) = metadata.get("meeting_id").and_then(|v| v.as_str()) {
                    if owner != id {
                        return Err("Recording metadata belongs to another meeting".into());
                    }
                }
                for (name, _) in &s.identities {
                    let field = match Path::new(name).file_stem().and_then(|x| x.to_str()) {
                        Some("audio") => "audio_file",
                        Some("mic") => "mic_audio_file",
                        _ => "system_audio_file",
                    };
                    if metadata.get(field).and_then(|v| v.as_str()) != Some(name.as_str()) {
                        return Err(format!("{name} is not declared as an owned final track"));
                    }
                }
                if s.identities.is_empty() {
                    return Err("No saved audio remains".into());
                }
                Ok(())
            })();
            if let Err(e) = check {
                s.recording.blocked = Some(e);
            }
            s
        })
        .await
        .map_err(|e| format!("Could not inspect recording files: {e}"))?;
        if has_queue {
            let busy: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM post_call_jobs WHERE meeting_id=? AND status NOT IN ('complete','cancelled','failed'))")
                .bind(&id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
            if busy {
                s.recording.blocked =
                    Some("Background job retains this audio; finish or cancel it first".into());
            }
        }
        if has_versions {
            let busy: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transcript_versions WHERE meeting_id=? AND status='running')")
                .bind(&id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
            if busy {
                s.recording.blocked =
                    Some("Native transcript refinement retains this audio".into());
            }
        }
        snapshots.push(s);
    }
    Ok(snapshots)
}
pub async fn inventory(pool: &SqlitePool) -> Result<Inventory, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let snapshots = inspect(&mut tx, None).await?;
    let usage_complete = snapshots.iter().all(|s| s.usage_complete);
    let mut measured: Vec<_> = snapshots.iter().filter(|s| s.usage_complete).collect();
    measured.sort_by_key(|s| s.dir.components().count());
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut total_bytes = 0u64;
    for snapshot in measured {
        // A separately registered checkpoint subfolder was already measured by
        // its parent. Other nested folders were not traversed and count normally.
        let covered = seen.iter().any(|parent| {
            parent == &snapshot.dir
                || snapshot
                    .dir
                    .strip_prefix(parent)
                    .ok()
                    .is_some_and(|relative| {
                        relative
                            .components()
                            .next()
                            .is_some_and(|part| part.as_os_str() == ".checkpoints")
                    })
        });
        if !covered {
            total_bytes = total_bytes.saturating_add(snapshot.recording.bytes);
            seen.push(snapshot.dir.clone());
        }
    }
    Ok(Inventory {
        total_bytes,
        usage_complete,
        recordings: snapshots.into_iter().map(|s| s.recording).collect(),
    })
}
pub async fn prepare(
    pool: &SqlitePool,
    plans: &Plans,
    ids: Vec<String>,
) -> Result<Consent, String> {
    let ids: HashSet<_> = ids.into_iter().collect();
    if ids.is_empty() || ids.len() > 100 {
        return Err("Select between 1 and 100 recordings".into());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let snapshots: Vec<_> = inspect(&mut tx, Some(&ids)).await?.into_iter().collect();
    if snapshots.len() != ids.len() {
        return Err("A selected recording no longer exists; refresh the list".into());
    }
    for s in &snapshots {
        if let Some(error) = &s.recording.blocked {
            return Err(format!("{}: {error}", s.recording.title));
        }
    }
    let recordings: Vec<_> = snapshots.iter().map(|s| s.recording.clone()).collect();
    let token = uuid::Uuid::new_v4().to_string();
    let mut store = plans
        .0
        .lock()
        .map_err(|_| "Cleanup consent store unavailable")?;
    store.retain(|p| p.created.elapsed() < PLAN_LIFETIME);
    while store.len() >= 16 {
        store.pop_front();
    }
    store.push_back(Plan {
        token: token.clone(),
        created: Instant::now(),
        snapshots,
    });
    Ok(Consent {
        token,
        bytes: recordings.iter().map(|r| r.bytes).sum(),
        recordings,
    })
}
/// Caller owns offline-operation + recording lifecycle admission for the entire
/// future. BEGIN IMMEDIATE prevents new queue claims/retries and folder/version
/// changes until all unlink outcomes are known. A rollback never restores audio.
pub async fn execute(pool: &SqlitePool, plans: &Plans, token: &str) -> Result<Outcome, String> {
    let plan = {
        let mut store = plans
            .0
            .lock()
            .map_err(|_| "Cleanup consent store unavailable")?;
        let i = store
            .iter()
            .position(|p| p.token == token)
            .ok_or("Cleanup review expired or was already used; review again")?;
        store.remove(i).unwrap()
    };
    if plan.created.elapsed() >= PLAN_LIFETIME {
        return Err("Cleanup review expired; review again".into());
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|e| format!("Cannot reserve audio cleanup; nothing deleted: {e}"))?;
    let ids = plan
        .snapshots
        .iter()
        .map(|s| s.recording.meeting_id.clone())
        .collect();
    let current: HashMap<_, _> = inspect(&mut tx, Some(&ids))
        .await?
        .into_iter()
        .map(|s| (s.recording.meeting_id.clone(), s))
        .collect();
    let mut files = Vec::new();
    for old in plan.snapshots {
        let now = current.get(&old.recording.meeting_id);
        let blocked = match now {
            None => Some("Meeting was removed or its folder changed".to_string()),
            Some(s) if s.recording.blocked.is_some() => s.recording.blocked.clone(),
            Some(s)
                if s.dir != old.dir
                    || s.metadata != old.metadata
                    || s.identities != old.identities =>
            {
                Some("Audio or metadata changed since review; review again".into())
            }
            _ => None,
        };
        for (name, expected) in old.identities {
            let path = old.dir.join(&name);
            let result = if let Some(reason) = &blocked {
                Err(reason.clone())
            } else {
                let dir = old.dir.clone();
                let metadata = old.metadata.clone();
                let expected = expected.clone();
                tokio::task::spawn_blocking(move || {
                    if owned_dir(&dir)? != dir
                        || fs::read(dir.join("metadata.json")).map_err(|e| e.to_string())?
                            != metadata
                        || identity(&path)? != expected
                    {
                        return Err("Audio ownership changed during cleanup; review again".into());
                    }
                    fs::remove_file(&path).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| format!("Cleanup worker stopped; refresh remaining files: {e}"))?
            };
            let (status, removed_bytes, error) = match result {
                Ok(()) => ("removed", expected.bytes, None),
                Err(e) => (
                    if blocked.is_some() {
                        "blocked"
                    } else {
                        "error"
                    },
                    0,
                    Some(e),
                ),
            };
            files.push(FileOutcome {
                meeting_id: old.recording.meeting_id.clone(),
                name,
                status: status.into(),
                removed_bytes,
                error,
            });
        }
    }
    // Read-only transaction: release the write reservation without a fallible
    // commit after irreversible I/O. File outcomes remain truthful on disconnect.
    drop(tx);
    Ok(Outcome {
        removed_bytes: files.iter().map(|f| f.removed_bytes).sum(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expired_consent_is_consumed_before_any_database_or_file_operation() {
        let plans = Plans::default();
        plans.0.lock().unwrap().push_back(Plan {
            token: "expired-generated-token".into(),
            created: Instant::now() - PLAN_LIFETIME,
            snapshots: Vec::new(),
        });
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        assert!(execute(&pool, &plans, "expired-generated-token")
            .await
            .unwrap_err()
            .contains("expired"));
        assert!(plans.0.lock().unwrap().is_empty());
    }
}
