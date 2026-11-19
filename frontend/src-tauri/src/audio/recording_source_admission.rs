//! Short source-edit/Start admission, independent of model inference lifetime.
use std::sync::atomic::{AtomicUsize, Ordering};

static STARTING: AtomicUsize = AtomicUsize::new(0);
pub static SOURCE_ADMISSION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct SourceStart;
impl SourceStart {
    pub async fn begin() -> Self {
        let _admission = SOURCE_ADMISSION.lock().await;
        STARTING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for SourceStart {
    fn drop(&mut self) { STARTING.fetch_sub(1, Ordering::SeqCst); }
}
pub fn is_starting() -> bool { STARTING.load(Ordering::SeqCst) != 0 }
