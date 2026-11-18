use crate::{
    audio_cleanup::{self, Consent, Inventory, Outcome, Plans},
    state::AppState,
};
use std::sync::OnceLock;
use tauri::Emitter;

fn plans() -> &'static Plans {
    static PLANS: OnceLock<Plans> = OnceLock::new();
    PLANS.get_or_init(Plans::default)
}
fn idle() -> Result<(), String> {
    if crate::audio::recording_commands::is_recording_active()
        || crate::audio::recording_commands::is_recording_stopping()
    {
        Err("Finish the active recording and its save before cleaning audio".into())
    } else {
        Ok(())
    }
}
#[tauri::command]
pub async fn api_audio_storage(state: tauri::State<'_, AppState>) -> Result<Inventory, String> {
    let mut inventory = audio_cleanup::inventory(state.db_manager.pool()).await?;
    if let Err(error) = idle() {
        for recording in &mut inventory.recordings {
            recording.blocked = Some(error.clone());
        }
    }
    Ok(inventory)
}
#[tauri::command]
pub async fn api_prepare_audio_cleanup(
    state: tauri::State<'_, AppState>,
    meeting_ids: Vec<String>,
) -> Result<Consent, String> {
    idle()?;
    audio_cleanup::prepare(state.db_manager.pool(), plans(), meeting_ids).await
}
#[tauri::command]
pub async fn api_confirm_audio_cleanup<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<Outcome, String> {
    let pool = state.db_manager.pool().clone();
    // A WebView may disappear after confirming. The native task owns admission,
    // file outcomes and the completion event even if its IPC waiter is dropped.
    tokio::spawn(async move {
        // Never queue behind inference or capture startup: stale consent must
        // not become a delayed delete after a long recording/model pass.
        let _offline = crate::diarization::try_operation_guard().map_err(|_| {
            "Audio is in use by native processing; review cleanup again when it finishes"
        })?;
        let _lifecycle = crate::audio::common::try_engine_lifecycle_lock()?;
        idle()?;
        let outcome = audio_cleanup::execute(&pool, plans(), &token).await;
        let _ = app.emit("audio-cleanup-finished", ());
        outcome
    })
    .await
    .map_err(|e| format!("Native cleanup stopped; refresh remaining files before retry: {e}"))?
}
