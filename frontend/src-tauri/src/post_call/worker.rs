//! Native process owns scheduling, independent of WebView/navigation. The loop
//! has one executor and never starts work while a recorder is active/stopping.
use super::{priority, runner, store, store::Revision, summary::Snapshot};
use crate::{audio::recording_commands, codex_cli, database::transcript_versions, state::AppState};
use sqlx::SqlitePool;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub static PRIORITY: LazyLock<Arc<priority::Priority>> =
    LazyLock::new(|| Arc::new(priority::Priority::default()));
static STARTED: AtomicBool = AtomicBool::new(false);

async fn snapshot(pool: &SqlitePool, meeting: &str) -> Result<Snapshot, String> {
    // Read the proof-bearing list once, before any local inference. An edit
    // during this attempt affects the next job/retry, never this prompt snapshot.
    let approved_vocabulary =
        crate::database::repositories::vocabulary::VocabularyRepository::approved_global(pool)
            .await
            .map_err(|e| e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("UPDATE meetings SET id=id WHERE id=?")
        .bind(meeting)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    transcript_versions::ensure(&mut tx, meeting).await?;
    let (version_id, transcript_revision): (String, i64) =
        sqlx::query_as("SELECT version_id,revision FROM transcript_selection WHERE meeting_id=?")
            .bind(meeting)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    let lines = transcript_versions::current(&mut tx, meeting).await?;
    let notes: Option<(Option<String>, i64)> =
        sqlx::query_as("SELECT notes_markdown,revision FROM meeting_notes WHERE meeting_id=?")
            .bind(meeting)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    let (notes, notes_revision) = notes
        .map(|(text, r)| (text.unwrap_or_default(), r))
        .unwrap_or_default();
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(Snapshot {
        revision: Revision {
            meeting_id: meeting.into(),
            version_id,
            transcript_revision,
            notes_revision,
        },
        lines,
        notes,
        approved_vocabulary,
    })
}

/// Invoke after native AppState activation. Repeated WebView initialization must
/// not interrupt/recover a live worker. No AC/battery preference gates admission.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let pool = state.db_manager.pool().clone();
        if let Err(error) = store::recover(&pool).await {
            log::error!("Cannot recover post-call queue: {error}");
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
        drop(state);
        loop {
            if !recording_commands::is_recording_active()
                && !recording_commands::is_recording_stopping()
            {
                match store::claim(&pool).await {
                    Ok(Some(job)) => {
                        if let Some(work) = PRIORITY.try_background(&job.meeting_id).await {
                            // Recheck after admission, not just before SQLite IO.
                            if recording_commands::is_recording_active()
                                || recording_commands::is_recording_stopping()
                            {
                                drop(work);
                                store::reconcile_release(&pool, &job, "queued", None).await;
                            } else {
                                let _ = app.emit("post-call-queue-changed", ());
                                let meeting = job.meeting_id.clone();
                                let local_app = app.clone();
                                let token = work.token.clone();
                                let result=runner::run(&pool,&job,&work.token,
                                    |token|async move {crate::audio::retranscription::run_queued_refinement(local_app,meeting,&token).await},
                                    ||snapshot(&pool,&job.meeting_id),
                                    |prompt,token|async move {
                                        let path=codex_cli::commands::codex_cli_get_path().await?;
                                        let home=codex_cli::commands::isolated_auth_home().await?;
                                        // Snapshot::prompt uses the accepted #16 bounded spelling helper.
                                        // No legacy hints, live-session DTO or generic provider participates.
                                        codex_cli::cli::generate(path.as_deref(),&home,codex_cli::cli::DEFAULT_MODEL,
                                            "Saved meeting summary. Return only the specified evidence-backed JSON.",&prompt,Some(&token)).await
                                    }).await;
                                drop(work);
                                runner::finish(&pool, &job, &token, &result).await;
                                if result.is_err() {
                                    log::info!(
                                        "Post-call job retained for resume/retry: {}",
                                        job.meeting_id
                                    );
                                }
                                let _ = app.emit("post-call-queue-changed", ());
                            }
                        } else {
                            store::reconcile_release(&pool, &job, "queued", None).await;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => log::warn!("Cannot claim post-call queue: {error}"),
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}
