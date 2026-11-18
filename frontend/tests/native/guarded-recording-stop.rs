use crate::audio::{recording_commands as commands, recording_ownership as ownership, common};
use std::sync::atomic::Ordering;
fn app() -> crate::AppHandle<()> { crate::AppHandle(std::marker::PhantomData) }
fn owner() -> ownership::LifecycleOwner {
    ownership::LifecycleOwner { source: "zoom".into(), session_id: "synthetic-qualified-A".into(), app: "Zoom".into(), process: "us.zoom.xos".into() }
}
#[tokio::test]
async fn actual_stop_entrypoint_rejects_delayed_a_after_manual_b_starts_and_restores_owner_on_reload() {
    ownership::set_meeting_automation_enabled(true);
    let a = {
        let _engine = common::acquire_engine_lifecycle_lock().await;
        commands::IS_RECORDING.store(true, Ordering::SeqCst);
        ownership::begin(Some(owner()))
    };
    let reloaded = commands::get_recording_state().await;
    let reloaded_owner = reloaded["recording_lease"].clone();
    let original_owner = serde_json::to_value(&a).unwrap();
    assert_eq!(commands::stop_recording(app(), commands::RecordingArgs { save_path: String::new(), expected: None, manual_generation: None }).await.unwrap(), commands::StopOutcome::Completed);
    let b = {
        let _engine = common::acquire_engine_lifecycle_lock().await;
        commands::IS_RECORDING.store(true, Ordering::SeqCst);
        ownership::begin(None)
    };
    commands::EFFECTS.store(0, Ordering::SeqCst);
    assert_ne!(commands::stop_recording(app(), commands::RecordingArgs { save_path: String::new(), expected: Some(a), manual_generation: None }).await.unwrap(), commands::StopOutcome::Completed);
    assert_eq!(commands::EFFECTS.load(Ordering::SeqCst), 0);
    assert!(commands::IS_RECORDING.load(Ordering::SeqCst));
    assert_eq!(ownership::current(), Some(b));
    assert_eq!(reloaded_owner, original_owner);
    assert_eq!(commands::stop_recording(app(), commands::RecordingArgs { save_path: String::new(), expected: None, manual_generation: None }).await.unwrap(), commands::StopOutcome::Completed);
}
#[tokio::test]
async fn actual_stop_queued_behind_engine_lock_rechecks_native_disable_before_any_effects() {
    ownership::set_meeting_automation_enabled(true);
    let engine = common::acquire_engine_lifecycle_lock().await;
    let a = ownership::begin(Some(owner()));
    commands::IS_RECORDING.store(true, Ordering::SeqCst);
    commands::EFFECTS.store(0, Ordering::SeqCst);
    let expected = a.clone();
    let queued = tokio::spawn(async move {
        commands::stop_recording(app(), commands::RecordingArgs { save_path: String::new(), expected: Some(expected), manual_generation: None }).await.unwrap()
    });
    tokio::task::yield_now().await;
    ownership::set_meeting_automation_enabled(false);
    drop(engine);
    assert_ne!(queued.await.unwrap(), commands::StopOutcome::Completed);
    assert_eq!(commands::EFFECTS.load(Ordering::SeqCst), 0);
    assert_eq!(ownership::current(), Some(a));
    assert!(commands::IS_RECORDING.load(Ordering::SeqCst));
    assert_eq!(commands::stop_recording(app(), commands::RecordingArgs { save_path: String::new(), expected: None, manual_generation: None }).await.unwrap(), commands::StopOutcome::Completed);
}


#[tokio::test]
async fn routed_manual_generation_bypasses_permission_but_cannot_stop_the_next_take() {
    ownership::set_meeting_automation_enabled(false);
    let b = ownership::begin(None);
    commands::IS_RECORDING.store(true, Ordering::SeqCst);
    commands::EFFECTS.store(0, Ordering::SeqCst);
    assert_eq!(commands::stop_recording(app(), commands::RecordingArgs {save_path:String::new(),expected:None,manual_generation:Some(b.generation.clone())}).await.unwrap(), commands::StopOutcome::Completed);
    let c = ownership::begin(None); commands::IS_RECORDING.store(true, Ordering::SeqCst);
    let before = commands::EFFECTS.load(Ordering::SeqCst);
    assert_ne!(commands::stop_recording(app(), commands::RecordingArgs {save_path:String::new(),expected:None,manual_generation:Some(b.generation)}).await.unwrap(), commands::StopOutcome::Completed);
    assert_eq!(commands::EFFECTS.load(Ordering::SeqCst),before); assert_eq!(ownership::current(),Some(c));
    assert_eq!(commands::stop_recording(app(), commands::RecordingArgs {save_path:String::new(),expected:None,manual_generation:None}).await.unwrap(), commands::StopOutcome::Completed);
}
