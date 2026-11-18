//! Normal live caller used by the Tauri transport and portable fake-CLI tests.
//! Native ownership checks are separate from model text. No transcript is sent
//! until both the session tuple and committed Notes revision are verified.
use super::{
    cli,
    context::{CanonicalLine, LiveAnswer, QuestionSnapshot, UserNotes},
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn ask(
    request: QuestionSnapshot,
    current_lines: &[CanonicalLine],
    committed_notes: &UserNotes,
    configured: Option<&str>,
    auth_home: &std::path::Path,
    token: &CancellationToken,
    is_current: impl Fn() -> bool,
) -> Result<LiveAnswer, String> {
    const STALE: &str = "Recording stopped or changed. Request cancelled; no answer was attached to another meeting.";
    if !is_current() {
        return Err(STALE.into());
    }
    request.validate_current(current_lines)?;
    if request.user_notes.revision != committed_notes.revision
        || request.user_notes.markdown != committed_notes.markdown
        || committed_notes.source != "user_notes"
    {
        return Err("Notes changed or are not committed. Open Notes and retry after saving; nothing was sent.".into());
    }
    let prompt = request.prompt()?;
    let watch = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if !is_current() {
                token.cancel();
                return Err::<LiveAnswer, String>(STALE.into());
            }
        }
    };
    tokio::select! {
        biased;
        _ = token.cancelled() => Err("Request cancelled. Recording, transcription and Notes continue.".into()),
        stale = watch => stale,
        answer = cli::generate(configured, auth_home, cli::DEFAULT_MODEL, "Text-only current meeting Q&A. Return the specified JSON.", &prompt, Some(token)) => {
            if token.is_cancelled() { return Err("Request cancelled".into()); }
            if !is_current() { return Err(STALE.into()); }
            request.answer(&answer?)
        }
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
