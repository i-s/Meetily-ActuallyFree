use super::repositories::library;
use crate::state::AppState;

#[tauri::command]
pub async fn api_library_materials(state: tauri::State<'_, AppState>) -> Result<Vec<library::LibraryMaterial>, String> {
    library::materials(state.db_manager.pool()).await.map_err(|e| format!("Could not read library materials: {e}"))
}

#[tauri::command]
pub async fn api_search_library(state: tauri::State<'_, AppState>, query: String) -> Result<Vec<library::LibrarySearchHit>, String> {
    library::search(state.db_manager.pool(), &query, 200, true).await.map_err(|e| format!("Could not search library: {e}"))
}

#[tauri::command]
pub async fn api_export_meeting_snapshot(state: tauri::State<'_, AppState>, meeting_id: String, expected_notes_revision: i64) -> Result<library::MeetingExportSnapshot, String> {
    library::export_snapshot(state.db_manager.pool(), &meeting_id, expected_notes_revision).await
}
