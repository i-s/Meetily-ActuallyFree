use crate::priority::Priority;
use std::sync::Arc;
#[tokio::test]
async fn recording_reservation_cancels_owned_background_and_blocks_new_jobs_until_drain() {
    let p = Arc::new(Priority::default());
    let work = p.try_background("a").await.unwrap();
    let token = work.token.clone();
    let waiting = p.clone();
    let recording = tokio::spawn(async move { waiting.recording().await });
    token.cancelled().await;
    assert!(p.try_background("b").await.is_none());
    assert!(!recording.is_finished()); // in-flight native inference must drain first
    drop(work);
    let live = recording.await.unwrap();
    assert!(p.try_background("b").await.is_none());
    drop(live);
    assert!(p.try_background("b").await.is_some());
}
#[tokio::test]
async fn cancellation_is_scoped_to_the_owned_meeting() {
    let p = Arc::new(Priority::default());
    let work = p.try_background("a").await.unwrap();
    p.cancel("b");
    assert!(!work.token.is_cancelled());
    p.cancel("a");
    assert!(work.token.is_cancelled());
}

#[test]
fn offline_inference_cancellation_is_scoped_and_live_threads_are_unaffected() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static CANCELLED: AtomicBool = AtomicBool::new(false);
    fn cancelled() -> bool {
        CANCELLED.load(Ordering::SeqCst)
    }
    let scope = crate::batch_control::Scope::new(Some(cancelled));
    assert!(crate::batch_control::check().is_ok());
    CANCELLED.store(true, Ordering::SeqCst);
    assert!(crate::batch_control::check().is_err());
    std::thread::spawn(|| assert!(crate::batch_control::check().is_ok()))
        .join()
        .unwrap();
    drop(scope);
    assert!(crate::batch_control::check().is_ok());
    CANCELLED.store(false, Ordering::SeqCst);
}
