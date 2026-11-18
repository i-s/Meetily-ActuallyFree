//! Legacy mic-derived voiceprints are not qualified identity for a flagged
//! Zoom take (nor a room mic). Keep existing enrollment on disk, but do not read,
//! match, infer or write user identity through those shortcuts for this take.
#[derive(Clone, Copy, Debug)]
pub struct IdentityPolicy { pub trust_legacy_user: bool }
impl IdentityPolicy {
    pub fn from_metadata(value: &serde_json::Value) -> Self {
        if !value.is_object() { return Self { trust_legacy_user: false }; }
        let flag = |key: &str| match value.get(key) {
            Some(v) => v.as_bool().unwrap_or(true), // invalid provenance fails closed
            None => false, // historical unflagged recordings retain behavior
        };
        Self { trust_legacy_user: !(flag("in_person") || flag("microphone_identity_unverified")) }
    }
    pub fn for_recording(path: &std::path::Path) -> Result<Self, String> {
        let metadata = path.parent().map(|p| p.join("metadata.json"));
        let value = match metadata.map(std::fs::read) {
            Some(Ok(bytes)) => serde_json::from_slice(&bytes).map_err(|e| format!("Invalid recording provenance: {e}"))?,
            Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => return Err(format!("Cannot read recording provenance: {e}")),
            _ => serde_json::json!({}),
        };
        Ok(Self::from_metadata(&value))
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
