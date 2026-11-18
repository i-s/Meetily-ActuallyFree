//! One native job per application; no capture/inference lock is held while a
//! child runs. Cancellation tombstones cover a WebView cancel beating IPC start.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Inner {
    active: Option<(String, CancellationToken)>,
    cancelled: VecDeque<String>,
}
#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<Inner>>);
pub struct Job {
    owner: Jobs,
    id: String,
    pub token: CancellationToken,
}
impl Jobs {
    pub fn begin(&self, id: &str) -> Result<Job, String> {
        if id.is_empty() || id.len() > 100 {
            return Err("Invalid request identity".into());
        }
        let mut state = self.0.lock().map_err(|_| "CLI job state unavailable")?;
        if state.cancelled.iter().any(|i| i == id) {
            return Err("Request cancelled. Nothing was sent.".into());
        }
        if state.active.is_some() {
            return Err(
                "Another Codex request is running. Cancel it or wait before retrying.".into(),
            );
        }
        let token = CancellationToken::new();
        state.active = Some((id.into(), token.clone()));
        Ok(Job {
            owner: self.clone(),
            id: id.into(),
            token,
        })
    }
    pub fn cancel(&self, id: &str) {
        if id.is_empty() || id.len() > 100 {
            return;
        }
        if let Ok(mut state) = self.0.lock() {
            if let Some((active, token)) = &state.active {
                if active == id {
                    token.cancel();
                }
            }
            if !state.cancelled.iter().any(|i| i == id) {
                state.cancelled.push_back(id.into());
            }
            while state.cancelled.len() > 64 {
                state.cancelled.pop_front();
            }
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.token.cancel();
        if let Ok(mut state) = self.owner.0.lock() {
            if state.active.as_ref().is_some_and(|(id, _)| id == &self.id) {
                state.active = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_before_start_and_old_cancel_cannot_reach_another_meeting() {
        let jobs = Jobs::default();
        jobs.cancel("aborted-before-ipc");
        assert!(jobs.begin("aborted-before-ipc").is_err());
        let first = jobs.begin("meeting-A-request").unwrap();
        assert!(jobs.begin("queued").is_err());
        jobs.cancel("unrelated");
        assert!(!first.token.is_cancelled());
        jobs.cancel("meeting-A-request");
        assert!(first.token.is_cancelled());
        drop(first);
        let second = jobs.begin("meeting-B-request").unwrap();
        jobs.cancel("meeting-A-request");
        assert!(!second.token.is_cancelled());
        drop(second);
        assert!(jobs.begin("fresh-request").is_ok());
    }
}
