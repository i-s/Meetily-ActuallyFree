use super::{
    cli,
    context::{CanonicalLine, LiveAnswer, QuestionSnapshot, UserNotes},
    jobs::Jobs,
    service,
};
use crate::{audio::recording_commands, database::repositories::notes_document, state::AppState};
use std::sync::LazyLock;

static JOBS: LazyLock<Jobs> = LazyLock::new(Jobs::default);
pub(crate) async fn isolated_auth_home() -> Result<std::path::PathBuf, String> {
    let root = crate::paths::install_data_root();
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|_| "Cannot create live CLI data root")?;
    let root = tokio::fs::canonicalize(root)
        .await
        .map_err(|_| "Cannot resolve live CLI data root")?;
    Ok(root.join("codex-live-auth"))
}
fn path_file() -> std::path::PathBuf {
    crate::paths::install_data_root().join("codex-cli-path.txt")
}

#[tauri::command]
pub async fn codex_cli_get_path() -> Result<Option<String>, String> {
    match tokio::fs::read_to_string(path_file()).await {
        Ok(s) => Ok(Some(s.trim().to_owned()).filter(|s| !s.is_empty())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot read the saved Codex executable path".into()),
    }
}
#[tauri::command]
pub async fn codex_cli_save_path(path: Option<String>) -> Result<(), String> {
    let path = match path.filter(|p| !p.trim().is_empty()) {
        Some(p) => cli::resolve_binary(Some(&p))?
            .to_string_lossy()
            .into_owned(),
        None => String::new(),
    };
    let file = path_file();
    tokio::fs::create_dir_all(file.parent().ok_or("Invalid configuration directory")?)
        .await
        .map_err(|_| "Cannot create configuration directory")?;
    tokio::fs::write(file, path)
        .await
        .map_err(|_| "Cannot save Codex executable path".into())
}
/// Installation/auth-only check; no model request or meeting text is sent.
#[tauri::command]
pub async fn codex_cli_get_status(path: Option<String>) -> Result<cli::CodexCliStatus, String> {
    let path = match path {
        Some(p) => Some(p),
        None => codex_cli_get_path().await?,
    };
    let home = isolated_auth_home().await?;
    Ok(cli::probe(path.as_deref(), &home).await)
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCutoff {
    pub target: String,
    pub as_of_seconds: f64,
}
/// Captured at native acceptance of the user's request, not a stale display
/// timer or latest ASR event. Returns no audio/text and starts no model job.
#[tauri::command]
pub async fn codex_live_cutoff() -> Result<LiveCutoff, String> {
    let (target, as_of_seconds) = recording_commands::current_text_clock()?;
    Ok(LiveCutoff {
        target,
        as_of_seconds,
    })
}

#[tauri::command]
pub fn codex_live_cancel(request_id: String) {
    JOBS.cancel(&request_id);
}

#[tauri::command]
pub async fn codex_live_ask(
    state: tauri::State<'_, AppState>,
    request_id: String,
    mut request: QuestionSnapshot,
) -> Result<LiveAnswer, String> {
    let job = JOBS.begin(&request_id)?;
    let (target, clock, segments) = recording_commands::current_text_context()?;
    if target != request.target
        || !request.as_of_seconds.is_finite()
        || request.as_of_seconds > clock + 1.0
    {
        return Err(
            "Recording changed or request cutoff is in the future. Nothing was sent.".into(),
        );
    }
    let canonical: Vec<_> = segments
        .iter()
        .map(|s| CanonicalLine {
            sequence_id: s.sequence_id,
            time: s.audio_start_time,
            end_time: s.audio_end_time,
            text: s.text.clone(),
        })
        .collect();
    request.validate_current(&canonical)?;
    // Preserve native mic/system provenance independently of user-edited labels.
    // A model's channel number is never promoted to a persistent identity.
    for line in &mut request.lines {
        line.source = segments
            .iter()
            .find(|s| s.sequence_id == line.sequence_id)
            .and_then(|s| s.speaker.clone());
    }
    let notes = notes_document::read(state.db_manager.pool(), &target)
        .await
        .map_err(|_| "Cannot verify committed Notes. Nothing was sent; open Notes and retry.")?;
    let committed = UserNotes {
        source: "user_notes".into(),
        markdown: notes
            .as_ref()
            .and_then(|n| n.markdown.clone())
            .unwrap_or_default(),
        revision: notes.map(|n| n.revision).unwrap_or(0),
    };
    let path = codex_cli_get_path().await?;
    let home = isolated_auth_home().await?;
    service::ask(
        request,
        &canonical,
        &committed,
        path.as_deref(),
        &home,
        &job.token,
        || recording_commands::recording_target_is_current(&target),
    )
    .await
}
