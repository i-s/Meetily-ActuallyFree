//! Tauri commands backing the Codex CLI section of Model Settings.

use serde::Serialize;
use tauri::State;

use crate::database::repositories::setting::SettingsRepository;
use crate::state::AppState;

use super::CodexCliStatus;
use crate::codex_cli;

/// Outcome of the "Test connection" button.
#[derive(Debug, Serialize)]
pub struct CodexCliTestResult {
    pub status: String,
    pub message: String,
}

/// Detect the CLI and report its version and sign-in state.
///
/// `path` lets Model Settings preview an override the user has typed but not
/// saved yet; when omitted the saved path (if any) is used.
#[tauri::command]
pub async fn codex_cli_get_status(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<CodexCliStatus, String> {
    let configured = match path {
        Some(path) => Some(path),
        None => SettingsRepository::get_codex_cli_path(state.db_manager.pool())
            .await
            .map_err(|e| format!("Failed to read the Codex CLI path: {}", e))?,
    };

    Ok(codex_cli::probe(configured.as_deref()).await)
}

/// Read the saved CLI path override, if the user set one.
#[tauri::command]
pub async fn codex_cli_get_path(state: State<'_, AppState>) -> Result<Option<String>, String> {
    SettingsRepository::get_codex_cli_path(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to read the Codex CLI path: {}", e))
}

/// Save an explicit CLI path, or clear it to resume auto-discovery.
#[tauri::command]
pub async fn codex_cli_save_path(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<(), String> {
    SettingsRepository::save_codex_cli_path(state.db_manager.pool(), path.as_deref())
        .await
        .map_err(|e| format!("Failed to save the Codex CLI path: {}", e))
}

/// Send a throwaway prompt through the CLI to confirm summaries will actually run.
///
/// This is the only check that costs subscription usage, so it stays behind an
/// explicit button rather than running with the status probe.
#[tauri::command]
pub async fn codex_cli_test_connection(
    state: State<'_, AppState>,
    path: Option<String>,
    model: Option<String>,
) -> Result<CodexCliTestResult, String> {
    let configured = match path {
        Some(path) => Some(path),
        None => SettingsRepository::get_codex_cli_path(state.db_manager.pool())
            .await
            .map_err(|e| format!("Failed to read the Codex CLI path: {}", e))?,
    };

    let model = model.unwrap_or_else(|| codex_cli::DEFAULT_MODEL.to_string());

    match codex_cli::generate(
        configured.as_deref(),
        &model,
        "Reply with exactly one word and nothing else.",
        "Reply with the single word: ready",
        None,
    )
    .await
    {
        Ok(response) => Ok(CodexCliTestResult {
            status: "success".to_string(),
            message: format!("Codex CLI responded: {}", response.trim()),
        }),
        Err(e) => Ok(CodexCliTestResult {
            status: "error".to_string(),
            message: e,
        }),
    }
}
