//! Durable automatic post-call pipeline. Commands observe/control native jobs;
//! neither WebView lifetime nor a JS promise owns required native side effects.
pub mod priority;
pub mod runner;
pub mod store;
pub mod summary;
pub mod worker;

#[tauri::command]
pub async fn post_call_list(
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<store::Job>, String> {
    store::list(state.db_manager.pool()).await
}
#[tauri::command]
pub async fn post_call_control(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
    action: String,
) -> Result<(), String> {
    // Invalidate publication first; cancellation/drain is scoped to this meeting.
    store::control(state.db_manager.pool(), &meeting_id, &action).await?;
    worker::PRIORITY.cancel(&meeting_id);
    Ok(())
}
#[tauri::command]
pub async fn post_call_results(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<Vec<store::Artifact>, String> {
    store::results(state.db_manager.pool(), &meeting_id).await
}
