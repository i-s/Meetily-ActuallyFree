use crate::audio::{common, recording_commands as commands, recording_ownership as ownership, recording_start_authority as starts};
use std::sync::atomic::Ordering;
fn app() -> crate::AppHandle<()> { crate::AppHandle(std::marker::PhantomData) }
fn request(source: &str, token: &str) -> starts::StartRequest {
    let cap = starts::get_recording_start_capability();
    starts::StartRequest { version: cap.version, epoch: cap.epoch, sequence: cap.next_sequence,
        source: source.into(), session_id: format!("qualified-{source}-{token}"), token: token.into() }
}
async fn start(expected: starts::StartRequest) -> Result<ownership::RecordingLease, String> {
    commands::start_recording_with_lifecycle(app(), None, None, None, Some(expected.session_id.clone()), Some(expected)).await
}
async fn reset() {
    let _lock = common::acquire_engine_lifecycle_lock().await;
    ownership::claim_stop(None);
    ownership::set_meeting_automation_enabled(true);
    commands::IS_RECORDING.store(false, Ordering::SeqCst);
    commands::CAPTURES.store(0, Ordering::SeqCst);
    commands::HOLD.store(false, Ordering::SeqCst);
    commands::FAIL.store(false, Ordering::SeqCst);
}
#[tokio::test]
async fn cancelled_before_delayed_ipc_reload_and_late_callback_cannot_start_or_adopt_b() {
    reset().await;
    let a = request("zoom", "reload-A");
    // The JSON tuple is what the reloaded WebView must retain, not a fresh ticket.
    let a: starts::StartRequest = serde_json::from_slice(&serde_json::to_vec(&a).unwrap()).unwrap();
    let settled = commands::cancel_automated_recording_start(a.clone()).await.unwrap();
    assert_eq!(settled.outcome, starts::StartOutcome::Cancelled);
    assert!(settled.terminal && settled.no_future_commit && !settled.is_recording);
    let b = request("discord", "B");
    let lease = start(b.clone()).await.unwrap();
    assert!(start(a.clone()).await.is_err());
    assert!(start(b.clone()).await.is_err()); // same IPC retried: no second capture
    let again = commands::resolve_automated_recording_start(a).await.unwrap();
    assert_eq!(again.outcome, starts::StartOutcome::Cancelled);
    assert_eq!(again.recording_lease, Some(lease.clone()));
    assert!(again.is_recording);
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 1);
    let current = commands::resolve_automated_recording_start(b).await.unwrap();
    assert_eq!(current.committed_lease, Some(lease));
}
#[tokio::test]
async fn queued_start_is_fenced_under_same_engine_lock_not_by_idle_snapshot() {
    reset().await;
    let a = request("zoom", "queued");
    let lock = common::acquire_engine_lifecycle_lock().await;
    let expected = a.clone();
    let cancel = tokio::spawn(async move { commands::cancel_automated_recording_start(expected).await });
    tokio::task::yield_now().await;
    let queued = tokio::spawn(async move { start(a).await });
    tokio::task::yield_now().await;
    assert!(!commands::IS_RECORDING.load(Ordering::SeqCst)); // not cancellation proof
    assert!(!cancel.is_finished());
    drop(lock);
    assert_eq!(cancel.await.unwrap().unwrap().outcome, starts::StartOutcome::Cancelled);
    assert!(queued.await.unwrap().is_err());
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn held_capture_barrier_waits_then_returns_committed_lease_and_busy_b_cannot_capture() {
    reset().await;
    let a = request("zoom", "held");
    commands::HOLD.store(true, Ordering::SeqCst);
    let expected = a.clone();
    let running = tokio::spawn(async move { start(expected).await });
    commands::ENTERED.acquire().await.unwrap().forget();
    let barrier = tokio::spawn(async move { commands::cancel_automated_recording_start(a).await });
    tokio::task::yield_now().await;
    assert!(!barrier.is_finished());
    commands::RELEASE.add_permits(1);
    let lease = running.await.unwrap().unwrap();
    let result = barrier.await.unwrap().unwrap();
    assert_eq!(result.outcome, starts::StartOutcome::Committed);
    assert_eq!(result.committed_lease, Some(lease.clone()));
    assert_eq!(result.recording_lease, Some(lease.clone()));
    assert!(result.is_recording && result.no_future_commit);
    commands::HOLD.store(false, Ordering::SeqCst);
    let b = request("discord", "busy");
    assert!(start(b.clone()).await.is_err());
    let b = commands::resolve_automated_recording_start(b).await.unwrap();
    assert_eq!(b.outcome, starts::StartOutcome::Failed);
    assert_eq!(b.recording_lease, Some(lease));
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn failed_capture_settles_and_manual_take_is_never_cancelled() {
    reset().await;
    let a = request("zoom", "failed");
    commands::FAIL.store(true, Ordering::SeqCst);
    assert!(start(a.clone()).await.is_err());
    commands::FAIL.store(false, Ordering::SeqCst);
    let manual = {
        let _lock = common::acquire_engine_lifecycle_lock().await;
        commands::IS_RECORDING.store(true, Ordering::SeqCst);
        ownership::begin(None)
    };
    for _ in 0..2 {
        let result = commands::cancel_automated_recording_start(a.clone()).await.unwrap();
        assert_eq!(result.outcome, starts::StartOutcome::Failed);
        assert_eq!(result.committed_lease, None);
        assert_eq!(result.recording_lease, Some(manual.clone()));
        assert!(result.is_recording && result.no_future_commit);
    }
    let b = request("discord", "manual-busy");
    assert!(start(b).await.is_err());
    assert_eq!(ownership::current(), Some(manual));
}
#[tokio::test]
async fn future_sequence_has_no_receipt_then_after_advance_can_be_fenced() {
    reset().await;
    let first = request("zoom", "first");
    let mut future = first.clone();
    future.sequence = (first.sequence.parse::<u64>().unwrap() + 1).to_string();
    future.token = "future".into();
    assert!(commands::cancel_automated_recording_start(future.clone()).await.is_err());
    commands::resolve_automated_recording_start(first).await.unwrap();
    let terminal = commands::cancel_automated_recording_start(future.clone()).await.unwrap();
    assert!(terminal.terminal && terminal.no_future_commit);
    assert_eq!(terminal.expected, future);
    assert!(start(future).await.is_err());
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn actual_epoch_in_receipt_is_not_echo_and_old_epoch_cannot_touch_manual_lease() {
    reset().await;
    let mut old = request("zoom", "old-process");
    old.epoch = "previous-process".into();
    assert!(start(old.clone()).await.is_err());
    let manual = {
        let _lock = common::acquire_engine_lifecycle_lock().await;
        commands::IS_RECORDING.store(true, Ordering::SeqCst);
        ownership::begin(None)
    };
    let result = commands::resolve_automated_recording_start(old.clone()).await.unwrap();
    assert_eq!(result.expected, old);
    assert_eq!(result.outcome, starts::StartOutcome::EpochMismatch);
    assert_eq!(result.native_epoch, starts::get_recording_start_capability().epoch);
    assert_ne!(result.native_epoch, result.expected.epoch);
    assert_eq!(result.recording_lease, Some(manual));
    assert!(result.no_future_commit && result.is_recording);
}
#[tokio::test]
async fn qualification_binding_and_missing_authority_never_reach_capture() {
    reset().await;
    let mut unqualified = request("zoom", "unqualified");
    unqualified.session_id = "real-unqualified-observation".into();
    assert!(start(unqualified.clone()).await.is_err());
    assert_eq!(commands::resolve_automated_recording_start(unqualified).await.unwrap().outcome, starts::StartOutcome::Failed);
    let mut wrong = request("zoom", "binding"); wrong.source = "discord".into();
    assert!(start(wrong.clone()).await.is_err());
    assert_eq!(commands::resolve_automated_recording_start(wrong).await.unwrap().outcome, starts::StartOutcome::Failed);
    assert!(commands::start_recording_with_lifecycle(app(), None, None, None, Some("qualified-zoom-missing".into()), None).await.is_err());
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 0);
    // Genuine native manual intent remains possible without a lifecycle ticket.
    commands::start_recording_with_lifecycle(app(), None, None, None, None, None).await.unwrap();
    assert_eq!(commands::CAPTURES.load(Ordering::SeqCst), 1);
}
