use crate::{app, AppHandle, StoreExt};
use crate::audio::recording_preferences::{load_recording_preferences, save_recording_preferences, set_recording_source_enabled};
use serde_json::{json, Value};
use std::future::Future;

fn fixture(legacy:bool)->(tempfile::TempDir,AppHandle<()>) {
    let folder=tempfile::tempdir().unwrap();let app=app();
    app.store.set("preferences",json!({"save_folder":folder.path(),"auto_save":true,"file_format":"mp4","in_person":legacy,
        "preferred_mic_device":"USB microphone","preferred_system_device":"saved output",
        "per_app_recording_enabled":true,"per_app_targets":[{"id":"zoom","name":"Zoom","executable":"zoom.us"}],"opaque":{"keep":7}}));
    (folder,app)
}

#[tokio::test]
async fn production_loader_migrates_durably_and_rolls_back_on_failed_disk_write() {
    let (_folder,app)=fixture(true);
    app.store.fail.store(true,std::sync::atomic::Ordering::SeqCst);
    assert!(load_recording_preferences(&app).await.is_err());
    assert_eq!(app.store.get("preferences").unwrap()["in_person"],true);
    app.store.fail.store(false,std::sync::atomic::Ordering::SeqCst);
    let prefs=load_recording_preferences(&app).await.unwrap();
    assert!(!prefs.sources.system_audio_enabled);
    let saved=app.store.get("preferences").unwrap();
    assert!(saved.get("in_person").is_none());assert_eq!(saved["opaque"],json!({"keep":7}));
    assert_eq!(saved["system_audio_enabled"],false);
}

#[tokio::test]
async fn explicit_source_patch_survives_stale_full_settings_save_and_failed_patch() {
    let (_folder,app)=fixture(false);
    let stale=load_recording_preferences(&app).await.unwrap();
    set_recording_source_enabled(app.clone(),"system".into(),false).await.unwrap();
    save_recording_preferences(&app,&stale).await.unwrap();
    assert!(!load_recording_preferences(&app).await.unwrap().sources.system_audio_enabled);
    app.store.fail.store(true,std::sync::atomic::Ordering::SeqCst);
    assert!(set_recording_source_enabled(app.clone(),"system".into(),true).await.is_err());
    assert_eq!(app.store.get("preferences").unwrap()["system_audio_enabled"],false);
    app.store.fail.store(false,std::sync::atomic::Ordering::SeqCst);
    assert!(set_recording_source_enabled(app.clone(),"unknown".into(),true).await.is_err());
    crate::audio::recording_commands::BUSY.store(true,std::sync::atomic::Ordering::SeqCst);
    assert!(set_recording_source_enabled(app.clone(),"system".into(),true).await.is_err());
    crate::audio::recording_commands::BUSY.store(false,std::sync::atomic::Ordering::SeqCst);
}

#[tokio::test]
async fn both_production_start_setups_skip_disabled_devices_and_stale_app_targets() {
    for custom in [false,true] {
        let (_folder,app)=fixture(true);
        crate::capture_setup::RESOLVED.lock().unwrap().clear();
        let (mut manager,mic,system)=if custom {
            crate::capture_setup::custom_start(app,Some("USB microphone".into()),Some("saved output".into()),None).await.unwrap()
        }else{crate::capture_setup::default_start(app).await.unwrap()};
        let (mic,system,per_app)=manager.boundary(mic,system).unwrap();
        assert!(mic.is_some());assert!(system.is_none());assert!(!per_app);
        assert!(!crate::audio::recording_mode::in_person());
        assert!(crate::audio::recording_mode::microphone_identity_unverified());
        assert_eq!(*crate::capture_setup::RESOLVED.lock().unwrap(),vec!["USB microphone"]);
    }
}

#[tokio::test]
async fn both_production_start_setups_skip_microphone_for_explicit_system_only() {
    for custom in [false,true] {
        let (_folder,app)=fixture(false);
        set_recording_source_enabled(app.clone(),"microphone".into(),false).await.unwrap();
        crate::capture_setup::RESOLVED.lock().unwrap().clear();
        let (mut manager,mic,system)=if custom {
            crate::capture_setup::custom_start(app,Some("USB microphone".into()),Some("saved output".into()),None).await.unwrap()
        }else{crate::capture_setup::default_start(app).await.unwrap()};
        let (mic,system,per_app)=manager.boundary(mic,system).unwrap();
        assert!(mic.is_none());assert!(system.is_some());assert!(per_app);
        assert_eq!(*crate::capture_setup::RESOLVED.lock().unwrap(),vec!["saved output"]);
    }
}

#[tokio::test]
async fn both_off_or_invalid_persisted_sources_never_resolve_devices() {
    for custom in [false,true] {for invalid in [json!(false),Value::Null,json!("bad")] {
        let (_folder,app)=fixture(true);
        let mut value=app.store.get("preferences").unwrap();value["microphone_enabled"]=invalid;
        app.store.set("preferences",value);
        crate::capture_setup::RESOLVED.lock().unwrap().clear();
        let result=if custom {crate::capture_setup::custom_start(app,None,None,None).await}else{crate::capture_setup::default_start(app).await};
        assert!(result.is_err());assert!(crate::capture_setup::RESOLVED.lock().unwrap().is_empty());
    }}
}

#[tokio::test]
async fn automated_source_validation_cannot_broaden_microphone_only_selection() {
    for source in ["zoom","discord"] {
        let (_folder,app)=fixture(true);
        crate::capture_setup::RESOLVED.lock().unwrap().clear();
        assert!(crate::capture_setup::custom_start(app,None,None,Some(crate::capture_setup::Owner{source:source.into()})).await.is_err());
        assert!(crate::capture_setup::RESOLVED.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn direct_unmute_cannot_claim_an_excluded_source_is_now_captured() {
    use crate::audio::recording_commands::*;
    use std::sync::atomic::{AtomicBool,Ordering};
    for captured in [(true,false),(false,true)] {
        *RECORDING_MANAGER.lock().unwrap()=Some(MuteManager{captured,mic:AtomicBool::new(true),system:AtomicBool::new(true)});
        BUSY.store(true,Ordering::SeqCst);
        assert_eq!(set_microphone_muted(app(),false).await.is_ok(),captured.0);
        assert_eq!(set_system_audio_muted(app(),false).await.is_ok(),captured.1);
        assert!(set_microphone_muted(app(),true).await.is_ok());
        assert!(set_system_audio_muted(app(),true).await.is_ok());
    }
    BUSY.store(false,Ordering::SeqCst);
    *RECORDING_MANAGER.lock().unwrap()=None;
}


#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn read_triggered_legacy_migration_cannot_restore_a_successfully_disabled_microphone() {
    use std::sync::{atomic::Ordering,mpsc};
    let (_folder,app)=fixture(true);
    // Pause the reader AFTER its migration snapshot is copied, not while holding
    // the fake store's internal value lock. Setter uses the ordinary IPC body.
    let (entered,at_snapshot)=mpsc::channel();let (resume,continued)=mpsc::channel();
    app.store.pause_read.store(2,Ordering::SeqCst);
    *app.store.entered.lock().unwrap()=Some(entered);*app.store.resume.lock().unwrap()=Some(continued);
    let reader_app=app.clone();
    let reader=tokio::spawn(async move{load_recording_preferences(&reader_app).await});
    tokio::task::spawn_blocking(move||at_snapshot.recv().unwrap()).await.unwrap();
    let setter_app=app.clone();
    let mut setter=Box::pin(set_recording_source_enabled(setter_app,"microphone".into(),false));
    // Poll once: old code finishes despite the reader's stale snapshot; fixed
    // code queues behind the read-modify-write owner. No timing/sleep assumption.
    let early=std::future::poll_fn(|cx|std::task::Poll::Ready(match setter.as_mut().poll(cx){std::task::Poll::Ready(result)=>Some(result),std::task::Poll::Pending=>None})).await;
    resume.send(()).unwrap();reader.await.unwrap().unwrap();
    if let Some(result)=early{result.unwrap();}else{setter.await.unwrap();}
    assert!(!load_recording_preferences(&app).await.unwrap().sources.microphone_enabled);
    assert_eq!(app.store.get("preferences").unwrap()["microphone_enabled"],false);
}


#[tokio::test]
async fn source_click_does_not_wait_for_background_engine_lock() {
    let (_folder,app)=fixture(false);
    let engine=crate::audio::common::acquire_engine_lifecycle_lock().await;
    let mut click=Box::pin(set_recording_source_enabled(app.clone(),"system".into(),false));
    let result=std::future::poll_fn(|cx|std::task::Poll::Ready(match click.as_mut().poll(cx){std::task::Poll::Ready(result)=>Some(result),std::task::Poll::Pending=>None})).await;
    drop(engine);
    assert!(result.is_some(),"short source edit waited on background inference");
    result.unwrap().unwrap();
    assert!(!load_recording_preferences(&app).await.unwrap().sources.system_audio_enabled);
}


#[tokio::test]
async fn start_admission_freezes_sources_without_holding_engine_or_cancelling_background_work() {
    let (_folder,app)=fixture(false);
    let engine=crate::audio::common::acquire_engine_lifecycle_lock().await;
    set_recording_source_enabled(app.clone(),"system".into(),false).await.unwrap();
    let starting=crate::audio::recording_source_admission::SourceStart::begin().await;
    assert!(set_recording_source_enabled(app.clone(),"system".into(),true).await.is_err());
    assert!(!load_recording_preferences(&app).await.unwrap().sources.system_audio_enabled);
    drop(engine);drop(starting);
    set_recording_source_enabled(app.clone(),"system".into(),true).await.unwrap();
}


#[tokio::test]
async fn reconnect_matches_hardware_direction_even_when_names_and_order_collide() {
 use crate::capture_setup::*;
 for requested in [DeviceMonitorType::Microphone,DeviceMonitorType::SystemAudio]{for reverse in [false,true]{
  let mut available=vec![AudioDevice{name:"shared".into(),device_type:DeviceType::Input},AudioDevice{name:"shared".into(),device_type:DeviceType::Output}];
  if reverse{available.reverse();}*AVAILABLE.lock().unwrap()=available;OPENED.lock().unwrap().clear();
  let mut manager=RecordingManager::new();
  assert!(manager.attempt_device_reconnect("shared",requested).await.unwrap());
  let opened=OPENED.lock().unwrap().clone();
  let expected=match requested{DeviceMonitorType::Microphone=>(Some(DeviceType::Input),None),DeviceMonitorType::SystemAudio=>(None,Some(DeviceType::Output))};
  assert_eq!(opened,vec![expected]);
 }}
}

#[tokio::test]
async fn reconnect_opposite_only_or_chosen_app_never_opens_an_unrequested_stream() {
 use crate::capture_setup::*;
 for (requested,opposite) in [(DeviceMonitorType::Microphone,DeviceType::Output),(DeviceMonitorType::SystemAudio,DeviceType::Input)]{
  *AVAILABLE.lock().unwrap()=vec![AudioDevice{name:"shared".into(),device_type:opposite}];OPENED.lock().unwrap().clear();
  let mut manager=RecordingManager::new();
  assert!(!manager.attempt_device_reconnect("shared",requested).await.unwrap());
  assert!(OPENED.lock().unwrap().is_empty());
  manager.per_app_enabled=true;
  assert!(manager.attempt_device_reconnect("shared",requested).await.is_err());
  assert!(OPENED.lock().unwrap().is_empty());
 }
}


#[tokio::test]
async fn background_busy_click_then_routed_start_raises_real_priority_and_rejects_late_setter() {
 use crate::post_call::worker::PRIORITY;
 for custom in [false,true]{
  let (_folder,app)=fixture(false);
  let background=PRIORITY.try_background("generated-meeting").await.unwrap();
  let engine=crate::audio::common::acquire_engine_lifecycle_lock().await;
  set_recording_source_enabled(app.clone(),"system".into(),false).await.unwrap();
  assert!(!background.token.is_cancelled(),"editing alone cancelled background ASR");
  let start_app=app.clone();
  let mut start=Box::pin(async move {if custom{crate::capture_setup::custom_start(start_app,None,None,None).await}else{crate::capture_setup::default_start(start_app).await}});
  assert!(std::future::poll_fn(|cx|std::task::Poll::Ready(start.as_mut().poll(cx).is_pending())).await);
  assert!(background.token.is_cancelled(),"routed Start did not reach priority before inference drain");
  assert!(set_recording_source_enabled(app.clone(),"system".into(),true).await.is_err());
  drop(engine);drop(background);
  let (mut manager,mic,sys)=start.await.unwrap();
  let (_,sys,apps)=manager.boundary(mic,sys).unwrap();assert!(sys.is_none());assert!(!apps);
  assert!(!load_recording_preferences(&app).await.unwrap().sources.system_audio_enabled);
 }
}

#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn failing_legacy_reader_rollback_cannot_undo_a_successful_source_write() {
 use std::sync::{atomic::Ordering,mpsc};
 let (_folder,app)=fixture(true);
 let (entered,at_snapshot)=mpsc::channel();let (resume,continued)=mpsc::channel();
 app.store.pause_read.store(2,Ordering::SeqCst);
 *app.store.entered.lock().unwrap()=Some(entered);*app.store.resume.lock().unwrap()=Some(continued);
 let reader_app=app.clone();let reader=tokio::spawn(async move{load_recording_preferences(&reader_app).await});
 tokio::task::spawn_blocking(move||at_snapshot.recv().unwrap()).await.unwrap();
 let mut setter=Box::pin(set_recording_source_enabled(app.clone(),"microphone".into(),false));
 let early=std::future::poll_fn(|cx|std::task::Poll::Ready(match setter.as_mut().poll(cx){std::task::Poll::Ready(result)=>Some(result),std::task::Poll::Pending=>None})).await;
 app.store.fail_once.store(true,Ordering::SeqCst);resume.send(()).unwrap();
 assert!(reader.await.unwrap().is_err());
 if let Some(result)=early{result.unwrap();}else{setter.await.unwrap();}
 assert!(!load_recording_preferences(&app).await.unwrap().sources.microphone_enabled);
}


#[tokio::test]
async fn generated_bundle_folder_repair_preserves_sources_unknown_fields_and_rolls_back_failure() {
 use std::sync::atomic::Ordering;
 let (folder,app)=fixture(true);
 let mut value=app.store.get("preferences").unwrap();
 value["save_folder"]=serde_json::to_value(folder.path().join("generated-app.app").join("recordings")).unwrap();
 app.store.set("preferences",value.clone());
 app.store.fail_once.store(true,Ordering::SeqCst);
 assert!(load_recording_preferences(&app).await.is_err());
 assert_eq!(app.store.get("preferences").unwrap(),value);
 let repaired=load_recording_preferences(&app).await.unwrap();
 assert!(!repaired.sources.system_audio_enabled);
 assert_ne!(serde_json::to_value(&repaired.save_folder).unwrap(),value["save_folder"]);
 let saved=app.store.get("preferences").unwrap();
 assert_eq!(saved["opaque"],serde_json::json!({"keep":7}));
 assert_eq!(saved["system_audio_enabled"],false);
}
