use crate::{app, AppHandle, StoreExt};
use crate::audio::recording_preferences::{load_recording_preferences, save_recording_preferences, set_recording_source_enabled};
use serde_json::{json, Value};

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
