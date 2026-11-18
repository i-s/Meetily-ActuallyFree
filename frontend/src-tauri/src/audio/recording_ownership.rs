//! Process-lifetime recording authority. Call only at the engine lifecycle lock
//! for start/stop; the short mutex also linearizes enable/disable and ownership
//! release against an automated stop. No WebView promise owns this state.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

/// Only native trackers publish End with the session they actually ended.
/// Inactive without that transition cannot manufacture an exit for a take.
#[derive(Clone, Copy)]
pub enum SourcePresence<'a> { Active(&'a str), End(&'a str), Inactive, Unknown }
#[derive(Default)]
struct SourceAuthority {
    enabled: bool,
    qualified: bool,
    active_session: Option<String>,
    ended_session: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleOwner {
    pub source: String,
    pub session_id: String,
    pub app: String,
    pub process: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordingLease {
    pub generation: String,
    pub owner: Option<LifecycleOwner>,
}
pub struct Authority {
    epoch: String,
    sequence: u64,
    current: Option<RecordingLease>,
    automation_enabled: bool,
    sources: BTreeMap<String, SourceAuthority>,
    stop_revoked: bool,
    exit_confirmed: bool,
}
impl Default for Authority {
    fn default() -> Self {
        Self { epoch: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos().to_string(), sequence: 0, current: None, automation_enabled: false,
            sources: BTreeMap::new(), stop_revoked: false, exit_confirmed: false }
    }
}
impl Authority {
    pub fn begin(&mut self, owner: Option<LifecycleOwner>) -> RecordingLease {
        self.sequence += 1;
        self.stop_revoked = owner.as_ref().is_some_and(|owner| !self.automation_enabled
            || !self.sources.get(&owner.source).is_some_and(|source| source.enabled && source.qualified
                && (source.active_session.as_deref() == Some(owner.session_id.as_str())
                    || source.ended_session.as_deref() == Some(owner.session_id.as_str()))));
        // Capture startup is awaited before this lease is published. Preserve a
        // qualified End that arrived in that interval for this exact owner.
        self.exit_confirmed = !self.stop_revoked && owner.as_ref().is_some_and(|owner|
            self.sources.get(&owner.source).is_some_and(|source|
                source.ended_session.as_deref() == Some(owner.session_id.as_str())));
        let lease = RecordingLease { generation: format!("{}:{}", self.epoch, self.sequence), owner };
        self.current = Some(lease.clone());
        lease
    }
    /// Permission loss is sticky for this take. Re-enabling an adapter or Labs
    /// must not resurrect a delayed exact Stop that was already revoked.
    fn set_source_enabled(&mut self, source: &str, enabled: bool) {
        if !matches!(source, "zoom" | "discord") { return; }
        let state = self.sources.entry(source.into()).or_default();
        state.enabled = enabled;
        if !enabled {
            state.active_session = None;
            state.ended_session = None;
            if self.current.as_ref().and_then(|lease| lease.owner.as_ref()).is_some_and(|owner| owner.source == source) {
                self.stop_revoked = true;
                self.exit_confirmed = false;
            }
        }
    }
    fn observe_source(&mut self, source: &str, enabled: bool, qualified: bool, presence: SourcePresence<'_>) {
        self.set_source_enabled(source, enabled);
        let Some(state) = self.sources.get_mut(source) else { return; };
        state.qualified = qualified;
        if enabled && qualified {
            state.active_session = match presence { SourcePresence::Active(session) => Some(session.into()), _ => None };
            match presence {
                SourcePresence::End(session) => state.ended_session = Some(session.into()),
                SourcePresence::Inactive => {},
                _ => state.ended_session = None,
            }
        } else {
            state.active_session = None;
            state.ended_session = None;
        }
        let Some(owner) = self.current.as_ref().and_then(|lease| lease.owner.as_ref()).filter(|owner| owner.source == source) else { return; };
        if !enabled || !qualified {
            self.stop_revoked = true;
            self.exit_confirmed = false;
            return;
        }
        match presence {
            SourcePresence::End(session) if session == owner.session_id => {
                self.exit_confirmed = !self.stop_revoked;
            },
            SourcePresence::Active(session) | SourcePresence::End(session) => {
                if self.exit_confirmed || session != owner.session_id { self.stop_revoked = true; }
                self.exit_confirmed = false;
            },
            SourcePresence::Unknown => {
                // Once a confirmed exit was withdrawn, a delayed publication
                // of that old End must not revive its queued Stop authority.
                if self.exit_confirmed { self.stop_revoked = true; }
                self.exit_confirmed = false;
            },
            SourcePresence::Inactive => {}, // only retains a previously confirmed owner End
        }
    }
    /// An automated stop needs the exact lease AND native owner-specific exit
    /// authority. Manual Stop deliberately supplies no guard and bypasses it.
    pub fn claim_stop(&mut self, expected: Option<&RecordingLease>) -> bool {
        if let Some(expected) = expected {
            if !self.automation_enabled || self.stop_revoked || !self.exit_confirmed
                || expected.owner.is_none() || self.current.as_ref() != Some(expected) { return false; }
        }
        if self.current.is_none() { return expected.is_none(); }
        self.current = None;
        true
    }
    pub fn release(&mut self, expected: &RecordingLease) -> bool {
        if self.current.as_ref() != Some(expected) { return false; }
        self.current.as_mut().unwrap().owner = None;
        true
    }
}
static AUTHORITY: LazyLock<Mutex<Authority>> = LazyLock::new(|| Mutex::new(Authority::default()));
pub fn begin(owner: Option<LifecycleOwner>) -> RecordingLease { AUTHORITY.lock().unwrap().begin(owner) }
pub fn current() -> Option<RecordingLease> { AUTHORITY.lock().unwrap().current.clone() }
pub fn claim_stop(expected: Option<&RecordingLease>) -> bool { AUTHORITY.lock().unwrap().claim_stop(expected) }
pub fn automation_enabled() -> bool { AUTHORITY.lock().unwrap().automation_enabled }
/// Callers publish settings/evidence while holding their settings/state locks;
/// claim_stop never reads those locks in reverse while holding this mutex.
pub fn set_source_enabled(source: &str, enabled: bool) { AUTHORITY.lock().unwrap().set_source_enabled(source, enabled); }
pub fn observe_source(source: &str, enabled: bool, qualified: bool, presence: SourcePresence<'_>) {
    AUTHORITY.lock().unwrap().observe_source(source, enabled, qualified, presence);
}

#[tauri::command]
pub fn set_meeting_automation_enabled(enabled: bool) {
    // Not loaded from old browser settings. Every native process starts false.
    let mut authority = AUTHORITY.lock().unwrap();
    authority.automation_enabled = enabled;
    if !enabled { authority.stop_revoked = true; authority.exit_confirmed = false; }
}
#[tauri::command]
pub fn release_automated_recording(expected: RecordingLease) -> bool {
    AUTHORITY.lock().unwrap().release(&expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> LifecycleOwner { LifecycleOwner { source: "zoom".into(), session_id: "call-a".into(), app: "Zoom".into(), process: "us.zoom.xos".into() } }
    #[test]
    fn delayed_a_stop_after_manual_b_starts_is_rejected_without_claiming_b() {
        let mut authority = Authority::default(); authority.automation_enabled = true;
        let a = authority.begin(Some(owner()));
        assert!(authority.claim_stop(None)); // manual Stop A
        let b = authority.begin(None); // manual Start B, before delayed route A
        assert!(!authority.claim_stop(Some(&a)));
        assert_eq!(authority.current, Some(b));
        assert!(authority.claim_stop(None)); // manual Stop B still works
    }
    #[test]
    fn disable_at_native_boundary_rejects_even_an_exact_lease() {
        let mut authority = Authority::default(); authority.automation_enabled = true;
        let a = authority.begin(Some(owner()));
        authority.automation_enabled = false;
        assert!(!authority.claim_stop(Some(&a)));
        assert_eq!(authority.current, Some(a));
        assert!(authority.claim_stop(None));
    }
    #[test]
    fn reload_reads_native_owner_and_keep_recording_releases_it_without_deleting_take() {
        let mut authority = Authority::default(); authority.automation_enabled = true;
        let a = authority.begin(Some(owner()));
        assert_eq!(authority.current.as_ref().unwrap().owner, Some(owner()));
        assert!(authority.release(&a));
        assert!(!authority.claim_stop(Some(&a))); // queued pre-release stop
        assert!(authority.current.is_some());
        assert!(authority.claim_stop(None));
    }
    #[test]
    fn withdrawn_end_cannot_be_resurrected_by_reordered_same_session_evidence() {
        for source in ["zoom", "discord"] {
            for interruption in [SourcePresence::Unknown, SourcePresence::Active("call-a")] {
                let mut authority = Authority::default(); authority.automation_enabled = true;
                let mut owner = owner(); owner.source = source.into();
                authority.observe_source(source, true, true, SourcePresence::Active("call-a"));
                let lease = authority.begin(Some(owner));
                authority.observe_source(source, true, true, SourcePresence::End("call-a"));
                authority.observe_source(source, true, true, interruption);
                authority.observe_source(source, true, true, SourcePresence::End("call-a"));
                assert!(!authority.claim_stop(Some(&lease)));
                assert!(authority.claim_stop(None));
            }
        }
    }
    #[test]
    fn unconfirmed_inactivity_never_authorizes_stop_and_other_source_disable_is_isolated() {
        for source in ["zoom", "discord"] {
            let mut authority = Authority::default(); authority.automation_enabled = true;
            let mut owner = owner(); owner.source = source.into();
            authority.observe_source(source, true, true, SourcePresence::Active("call-a"));
            let lease = authority.begin(Some(owner));
            authority.observe_source(source, true, true, SourcePresence::Inactive);
            assert!(!authority.claim_stop(Some(&lease)));
            authority.set_source_enabled(if source == "zoom" { "discord" } else { "zoom" }, false);
            authority.observe_source(source, true, true, SourcePresence::End("call-a"));
            authority.observe_source(source, true, true, SourcePresence::Inactive);
            assert!(authority.claim_stop(Some(&lease)));
        }
    }

}
