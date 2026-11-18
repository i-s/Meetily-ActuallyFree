//! Process-local resource admission; SQLite owns durable job state. A recording
//! reservation raises priority BEFORE waiting for the current inference to drain.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub struct Priority {
    work: Arc<AsyncMutex<()>>,
    pending: AtomicUsize,
    active: Mutex<Option<(String, CancellationToken)>>,
}
struct Ticket(Arc<Priority>);
impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.pending.fetch_sub(1, Ordering::SeqCst);
    }
}
pub struct Recording {
    _ticket: Ticket,
    _work: OwnedMutexGuard<()>,
}
pub struct Background {
    owner: Arc<Priority>,
    _work: OwnedMutexGuard<()>,
    pub token: CancellationToken,
}
impl Drop for Background {
    fn drop(&mut self) {
        *self.owner.active.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
impl Priority {
    pub async fn recording(self: &Arc<Self>) -> Recording {
        self.pending.fetch_add(1, Ordering::SeqCst);
        // Ticket also releases priority if startup's future is dropped while waiting.
        let ticket = Ticket(self.clone());
        if let Some((_, token)) = self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            token.cancel();
        }
        let work = self.work.clone().lock_owned().await;
        Recording {
            _ticket: ticket,
            _work: work,
        }
    }
    pub async fn try_background(self: &Arc<Self>, meeting: &str) -> Option<Background> {
        if self.pending.load(Ordering::SeqCst) != 0 {
            return None;
        }
        let work = self.work.clone().try_lock_owned().ok()?;
        let token = CancellationToken::new();
        // Synchronize token publication with the recording cancellation check.
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if self.pending.load(Ordering::SeqCst) != 0 {
            return None;
        }
        *active = Some((meeting.into(), token.clone()));
        drop(active);
        Some(Background {
            owner: self.clone(),
            _work: work,
            token,
        })
    }
    pub fn cancel(&self, meeting: &str) {
        if let Some((id, token)) = self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            if id == meeting {
                token.cancel();
            }
        }
    }
}
