use super::transcript_versions::*;
use crate::api::TranscriptSegment;

#[tauri::command]
pub async fn list_transcript_versions(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<Versions, String> {
    let pool = state.db_manager.pool();
    let mut result = list(pool, &meeting_id).await?;
    let folder: Option<String> =
        sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
            .bind(&meeting_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;
    result.audio_available = folder.is_some_and(|f| {
        crate::audio::retranscription::saved_sources(std::path::Path::new(&f)).is_ok()
    });
    Ok(result)
}

#[tauri::command]
pub async fn get_transcript_version(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
    version_id: String,
) -> Result<Vec<TranscriptSegment>, String> {
    segments(state.db_manager.pool(), &meeting_id, &version_id).await
}

#[tauri::command]
pub async fn select_transcript_version(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
    version_id: String,
    expected_revision: i64,
) -> Result<(), String> {
    let _guard = crate::diarization::operation_guard().await;
    select(
        state.db_manager.pool(),
        &meeting_id,
        &version_id,
        expected_revision,
    )
    .await
}

#[tauri::command]
pub async fn get_selected_transcript(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
) -> Result<SelectedTranscript, String> {
    selected(state.db_manager.pool(), &meeting_id).await
}

#[tauri::command]
pub async fn correct_transcript_text(
    state: tauri::State<'_, crate::state::AppState>,
    meeting_id: String,
    transcript_id: String,
    text: String,
    expected_revision: i64,
) -> Result<(), String> {
    correct_text(
        state.db_manager.pool(),
        &meeting_id,
        &transcript_id,
        &text,
        expected_revision,
    )
    .await
}
