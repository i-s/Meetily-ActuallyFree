//! Legacy mic-derived voiceprints are not qualified identity for a flagged
//! Zoom take (nor a room mic). Keep existing enrollment on disk, but do not read,
//! match, infer or write user identity through those shortcuts for this take.
#[derive(Clone, Copy, Debug)]
pub struct IdentityPolicy { pub trust_legacy_user: bool, pub in_person: bool }
impl IdentityPolicy {
    pub fn from_metadata(value: &serde_json::Value) -> Self {
        if !value.is_object() { return Self { trust_legacy_user: false, in_person: false }; }
        let flag = |key: &str| match value.get(key) {
            Some(v) => v.as_bool().unwrap_or(true), // invalid provenance fails closed
            None => false, // historical unflagged recordings retain behavior
        };
        Self { trust_legacy_user: !(flag("in_person") || flag("microphone_identity_unverified")),
            in_person: value.get("in_person").and_then(|v| v.as_bool()).unwrap_or(false) }
    }
    pub fn for_recording(path: &std::path::Path) -> Result<Self, String> {
        Self::for_folder(path.parent().ok_or("Recording has no provenance folder")?)
    }
    /// Resolve the ORIGINAL session folder, not a temporary decoded WAV parent.
    /// Missing file/fields are documented legacy; malformed data is not legacy.
    pub fn for_folder(folder: &std::path::Path) -> Result<Self, String> {
        Self::validated_metadata(&Self::metadata_for_folder(folder)?)
    }
    pub fn metadata_for_folder(folder: &std::path::Path) -> Result<serde_json::Value, String> {
        let value: serde_json::Value = match std::fs::read(folder.join("metadata.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| format!("Invalid recording provenance: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
            Err(e) => return Err(format!("Cannot read recording provenance: {e}")),
        };
        Self::validated_metadata(&value)?;
        Ok(value)
    }
    pub fn validated_metadata(value: &serde_json::Value) -> Result<Self, String> {
        if !value.is_object() { return Err("Recording provenance must be a JSON object".into()); }
        for key in ["in_person", "microphone_identity_unverified"] {
            if value.get(key).is_some_and(|flag| !flag.is_boolean()) {
                return Err(format!("Invalid {key} recording provenance"));
            }
        }
        Ok(Self::from_metadata(value))
    }
    /// Current files and frozen snapshots may only restrict each other. Missing
    /// fields mean legacy, but explicit null/nonobject stored provenance is invalid.
    pub fn with_settings(mut self, settings: &serde_json::Value) -> Result<Self, String> {
        if !settings.is_object() { return Err("Transcript settings must be a JSON object".into()); }
        for key in ["recording_metadata", "identity_policy"] {
            if let Some(metadata) = settings.get(key) {
                let other = Self::validated_metadata(metadata)?;
                self.trust_legacy_user &= other.trust_legacy_user;
                self.in_person |= other.in_person;
            }
        }
        Ok(self)
    }
    /// Store effective authority separately, without rewriting opaque metadata.
    pub fn snapshot(self) -> serde_json::Value {
        serde_json::json!({"in_person":self.in_person,"microphone_identity_unverified":!self.trust_legacy_user})
    }
    pub fn inferred_user(&self, candidate: Option<usize>) -> Option<usize> {
        if self.trust_legacy_user { candidate } else { None }
    }
    /// The selected engine receives the policy BEFORE it can consult enrolled
    /// identity. Clear its verdict as defense in depth against a future adapter.
    pub fn run<T>(&self, engine: impl FnOnce(bool) -> T) -> T { engine(self.trust_legacy_user) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_guest_and_polluted_legacy_enrollment_cannot_become_you() {
        let policy = IdentityPolicy::from_metadata(&serde_json::json!({"microphone_identity_unverified":true}));
        let polluted_enrollment_would_match = Some(0);
        let mut reads = 0;
        let verdict = policy.run(|allow_legacy| {
            if allow_legacy { reads += 1; polluted_enrollment_would_match } else { None }
        });
        assert_eq!(reads, 0); assert_eq!(verdict, None);
        assert_eq!(policy.inferred_user(polluted_enrollment_would_match), None);
        // Includes remote-only rows originally labeled Guest/system and stale
        // You rows: neither cluster numbering nor row overlap proves identity.
        for source in ["Guest", "System audio", "Microphone", "You"] {
            assert_eq!(policy.inferred_user(Some(0)), None, "{source}");
        }
    }
    #[test]
    fn unrelated_legacy_and_manual_names_are_not_deleted() {
        assert!(IdentityPolicy::from_metadata(&serde_json::json!({})).trust_legacy_user);
        assert!(!IdentityPolicy::from_metadata(&serde_json::json!({"in_person":true})).trust_legacy_user);
        assert!(!IdentityPolicy::from_metadata(&serde_json::json!({"microphone_identity_unverified":"bad"})).trust_legacy_user);
        assert!(crate::speaker_labels::is_manual_label("Alice"));
        assert_eq!(crate::speaker_labels::manual_names("Microphone + Alice"), vec!["Alice"]);
    }
}
