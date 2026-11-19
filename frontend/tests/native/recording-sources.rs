use crate::audio::recording_sources::RecordingSources;
use serde_json::{json, Value};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Preferences {
    preferred_mic_device: String,
    per_app_targets: Vec<String>,
    #[serde(flatten)] sources: RecordingSources,
}

#[test]
fn legacy_microphone_only_becomes_explicit_sources_not_a_hidden_mode() {
    let prefs: Preferences = serde_json::from_value(json!({"preferred_mic_device":"USB", "per_app_targets":["zoom.us"], "in_person":true})).unwrap();
    assert_eq!(prefs.sources, RecordingSources { microphone_enabled: true, system_audio_enabled: false });
    let saved = serde_json::to_value(prefs).unwrap();
    assert!(saved.get("in_person").is_none());
    assert_eq!(saved["system_audio_enabled"], false);
    assert_eq!(saved["per_app_targets"], json!(["zoom.us"]));
    assert_eq!(saved["preferred_mic_device"], "USB");
}

#[test]
fn explicit_modern_choice_wins_over_leftover_legacy_flag() {
    let selection: RecordingSources = serde_json::from_value(json!({"in_person":true,"microphone_enabled":false,"system_audio_enabled":true})).unwrap();
    assert_eq!(selection, RecordingSources { microphone_enabled:false, system_audio_enabled:true });
    assert!(selection.validate().is_ok());
    for value in [json!({}), json!({"in_person":false})] {
        assert_eq!(serde_json::from_value::<RecordingSources>(value).unwrap(), RecordingSources::default());
    }
}

#[test]
fn invalid_flags_and_both_off_never_broaden_capture() {
    for value in [Value::Null,json!([]),json!({"system_audio_enabled":null}),json!({"microphone_enabled":"true"}),json!({"in_person":1})] {
        assert!(serde_json::from_value::<RecordingSources>(value).is_err());
    }
    assert!(RecordingSources { microphone_enabled:false, system_audio_enabled:false }.validate().is_err());
}

#[test]
fn inclusion_filters_devices_and_app_capture_before_stream_setup() {
    let mic = RecordingSources { microphone_enabled:true, system_audio_enabled:false };
    assert_eq!(mic.devices(Some("mic"),Some("system")), (Some("mic"),None));
    assert!(!mic.captures_apps(true));
    assert!(mic.validate_available(true, false, false, 0).is_ok());
    assert!(mic.validate_available(false, false, false, 0).is_err());
    let system = RecordingSources { microphone_enabled:false, system_audio_enabled:true };
    assert_eq!(system.devices(Some("mic"),Some("system")), (None,Some("system")));
    assert!(system.validate_available(false, false, false, 0).is_err());
    assert!(system.validate_available(false, false, true, 1).is_ok());
    assert!(system.validate_available(false, true, true, 0).is_err());
    assert!(system.captures_apps(true));
}

#[test]
fn migrating_only_the_source_fields_preserves_unknown_preferences() {
    let mut old = json!({"in_person":true,"per_app_targets":["zoom.us"],"future":{"opaque":7}});
    assert!(RecordingSources::migrate_preferences(&mut old).unwrap());
    assert!(old.get("in_person").is_none());
    assert_eq!(old["system_audio_enabled"],false);
    assert_eq!(old["future"],json!({"opaque":7}));
    assert!(!RecordingSources::migrate_preferences(&mut old).unwrap());
}
