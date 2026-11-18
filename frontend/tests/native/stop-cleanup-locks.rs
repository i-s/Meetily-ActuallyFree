//! Real saver/cleanup paths with production lock helpers; only capture/Tauri are adapted.
use crate::audio::recording_state::{AudioChunk, DeviceType};
use crate::audio::{common, incremental_saver, recording_saver::RecordingSaver};
use std::time::Duration;

#[tokio::test]
async fn stop_save_completes_while_refinement_owns_offline_and_waits_for_lifecycle() {
    let root = tempfile::tempdir().unwrap();
    let mut saver = RecordingSaver::new();
    saver.set_recordings_folder(root.path().into());
    saver.set_meeting_name(Some("Generated lock-order take".into()));
    let sender = saver.start_accumulation(true).unwrap();
    sender
        .send(AudioChunk {
            data: vec![0.2; 4800],
            sample_rate: 48000,
            timestamp: 0.1,
            chunk_id: 0,
            device_type: DeviceType::Mixed,
        })
        .unwrap();
    drop(sender);
    // Actual Stop holds lifecycle before calling RecordingSaver::stop_and_save.
    let lifecycle = common::acquire_engine_lifecycle_lock().await;
    let (owns_offline, ready) = tokio::sync::oneshot::channel();
    let refinement = tokio::spawn(async move {
        let _offline = crate::diarization::operation_guard().await;
        owns_offline.send(()).unwrap();
        let _lifecycle = common::acquire_engine_lifecycle_lock().await;
    });
    ready.await.unwrap();
    let app = crate::AppHandle::<()>(std::marker::PhantomData);
    let saved =
        tokio::time::timeout(Duration::from_secs(3), saver.stop_and_save(&app, Some(0.1))).await;
    drop(lifecycle);
    refinement.await.unwrap();
    let audio = saved
        .expect("Stop must not wait for refinement's offline lock")
        .unwrap()
        .unwrap();
    let audio = std::path::Path::new(&audio);
    assert!(audio.is_file());
    assert!(audio.parent().unwrap().join("transcripts.json").is_file());
    assert!(!audio
        .parent()
        .unwrap()
        .join(".checkpoints/audio/audio_chunk_000.mp4")
        .exists());
    let metadata: serde_json::Value = serde_json::from_slice(
        &std::fs::read(audio.parent().unwrap().join("metadata.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata["status"], "completed");
    // Independent IPC stays admitted and idempotent after native finalization.
    let folder = audio.parent().unwrap().to_string_lossy().into_owned();
    incremental_saver::cleanup_checkpoints(folder.clone())
        .await
        .unwrap();
    incremental_saver::cleanup_checkpoints(folder)
        .await
        .unwrap();
}

#[tokio::test]
async fn independent_checkpoint_cleanup_still_excludes_an_offline_reader() {
    use std::{
        future::{poll_fn, Future},
        task::Poll,
    };
    let root = tempfile::tempdir().unwrap();
    let offline = crate::diarization::operation_guard().await;
    let mut cleanup = Box::pin(incremental_saver::cleanup_checkpoints(
        root.path().to_string_lossy().into_owned(),
    ));
    assert!(poll_fn(|cx| Poll::Ready(matches!(cleanup.as_mut().poll(cx), Poll::Pending))).await);
    drop(offline);
    tokio::time::timeout(Duration::from_secs(2), cleanup)
        .await
        .unwrap()
        .unwrap();
}
