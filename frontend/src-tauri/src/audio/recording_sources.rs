//! New-recording source inclusion, independent of archived room identity policy.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Value")]
pub struct RecordingSources {
    pub microphone_enabled: bool,
    pub system_audio_enabled: bool,
}

impl Default for RecordingSources {
    fn default() -> Self { Self { microphone_enabled: true, system_audio_enabled: true } }
}

impl TryFrom<Value> for RecordingSources {
    type Error = String;
    fn try_from(value: Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("Recording source preferences must be an object")?;
        let flag = |key: &str| -> Result<Option<bool>, String> {
            object.get(key).map(|v| v.as_bool().ok_or_else(|| format!("Invalid {key} recording source preference"))).transpose()
        };
        // A modern explicit choice wins. The old flag is read only for upgrade,
        // never retained as a parallel capture mode or emitted in preferences.
        let legacy_microphone_only = flag("in_person")?.unwrap_or(false);
        Ok(Self {
            microphone_enabled: flag("microphone_enabled")?.unwrap_or(true),
            system_audio_enabled: flag("system_audio_enabled")?.unwrap_or(!legacy_microphone_only),
        })
    }
}

impl RecordingSources {
    pub fn validate(self) -> Result<(), &'static str> {
        if !self.microphone_enabled && !self.system_audio_enabled {
            return Err("Enable the microphone or computer audio before recording.");
        }
        Ok(())
    }

    pub fn devices<T>(self, mic: Option<T>, system: Option<T>) -> (Option<T>, Option<T>) {
        (mic.filter(|_| self.microphone_enabled), system.filter(|_| self.system_audio_enabled))
    }

    pub fn captures_apps(self, per_app: bool) -> bool { self.system_audio_enabled && per_app }

    pub fn validate_available(self, mic_available: bool, system_available: bool, per_app: bool, targets: usize) -> Result<(), &'static str> {
        self.validate()?;
        if self.microphone_enabled && !mic_available { return Err("The selected microphone is unavailable."); }
        if self.system_audio_enabled {
            super::recording_mode::validate_sources(false, per_app, targets, system_available)?;
        }
        Ok(())
    }

    /// Update only source keys: unrelated and future store fields survive upgrade.
    pub fn migrate_preferences(value: &mut Value) -> Result<bool, String> {
        let sources = Self::try_from(value.clone())?;
        let object = value.as_object_mut().ok_or("Invalid recording preferences")?;
        let changed = object.contains_key("in_person") || !object.contains_key("microphone_enabled") || !object.contains_key("system_audio_enabled");
        object.remove("in_person");
        object.insert("microphone_enabled".into(), sources.microphone_enabled.into());
        object.insert("system_audio_enabled".into(), sources.system_audio_enabled.into());
        Ok(changed)
    }
}
