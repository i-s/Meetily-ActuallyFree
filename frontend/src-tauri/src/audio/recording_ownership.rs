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
struct AdmittedStop {
    id: u64,
    owner: LifecycleOwner,
    revoked: bool,
    exit_confirmed: bool,
}

/// Owns the provisional Stop authority from before capture awaits until commit
/// or destruction. Failure/cancellation releases only this admission's state.
pub struct StartAdmission { id: u64 }
impl StartAdmission {
    pub fn commit(self) -> RecordingLease {
        let mut authority = AUTHORITY.lock().unwrap();
        let pending = authority.admitted.take().filter(|pending| pending.id == self.id)
            .expect("start admission must remain owned until commit");
        let lease = authority.begin(Some(pending.owner));
        authority.stop_revoked = pending.revoked;
        authority.exit_confirmed = pending.exit_confirmed && !pending.revoked;
        lease
    }
}
impl Drop for StartAdmission {
    fn drop(&mut self) {
        let mut authority = AUTHORITY.lock().unwrap();
        if authority.admitted.as_ref().is_some_and(|pending| pending.id == self.id) {
            authority.admitted = None;
        }
    }
}

pub struct Authority {
    epoch: String,
    sequence: u64,
    current: Option<RecordingLease>,
    automation_enabled: bool,
    sources: BTreeMap<String, SourceAuthority>,
    stop_revoked: bool,
    exit_confirmed: bool,
    admission_sequence: u64,
    admitted: Option<AdmittedStop>,
}
impl Default for Authority {
    fn default() -> Self {
        Self { epoch: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos().to_string(), sequence: 0, current: None, automation_enabled: false,
            sources: BTreeMap::new(), stop_revoked: false, exit_confirmed: false,
            admission_sequence: 0, admitted: None }
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
            if let Some(pending) = self.admitted.as_mut().filter(|pending| pending.owner.source == source) {
                pending.revoked = true;
                pending.exit_confirmed = false;
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
        if let Some(owner) = self.current.as_ref().and_then(|lease| lease.owner.as_ref()).filter(|owner| owner.source == source) {
            (self.stop_revoked, self.exit_confirmed) = Self::observe_stop(owner, self.stop_revoked,
                self.exit_confirmed, enabled, qualified, presence);
        }
        if let Some(pending) = self.admitted.as_mut().filter(|pending| pending.owner.source == source) {
            (pending.revoked, pending.exit_confirmed) = Self::observe_stop(&pending.owner, pending.revoked,
                pending.exit_confirmed, enabled, qualified, presence);
        }
    }
    fn observe_stop(owner: &LifecycleOwner, mut revoked: bool, mut exit: bool,
        enabled: bool, qualified: bool, presence: SourcePresence<'_>) -> (bool, bool) {
        if !enabled || !qualified { return (true, false); }
        match presence {
            SourcePresence::End(session) if session == owner.session_id => exit = !revoked,
            SourcePresence::Active(session) | SourcePresence::End(session) => {
                if exit || session != owner.session_id { revoked = true; }
                exit = false;
            },
            SourcePresence::Unknown => {
                if exit { revoked = true; }
                exit = false;
            },
            SourcePresence::Inactive => {},
        }
        (revoked, exit)
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
pub fn admit(owner: LifecycleOwner) -> Result<StartAdmission, String> {
    let mut authority = AUTHORITY.lock().unwrap();
    if authority.admitted.is_some() || !authority.automation_enabled
        || !authority.sources.get(&owner.source).is_some_and(|source| source.enabled && source.qualified
            && source.active_session.as_deref() == Some(owner.session_id.as_str())) {
        return Err("Automated capture admission is no longer qualified or permitted".into());
    }
    authority.admission_sequence = authority.admission_sequence.checked_add(1).ok_or("Start admission exhausted")?;
    let id = authority.admission_sequence;
    authority.admitted = Some(AdmittedStop { id, owner, revoked: false, exit_confirmed: false });
    Ok(StartAdmission { id })
}
pub fn current() -> Option<RecordingLease> { AUTHORITY.lock().unwrap().current.clone() }
pub fn claim_stop(expected: Option<&RecordingLease>) -> bool { AUTHORITY.lock().unwrap().claim_stop(expected) }
/// Routed manual intent is generation-bound but independent of source/Labs
/// permission. Check both modes under one authority lock; never downgrade one.
pub fn claim_stop_request(expected: Option<&RecordingLease>, manual_generation: Option<&str>) -> bool {
    let mut authority = AUTHORITY.lock().unwrap();
    if let Some(generation) = manual_generation {
        if expected.is_some() || authority.current.as_ref().map(|lease| lease.generation.as_str()) != Some(generation) {
            return false;
        }
    }
    authority.claim_stop(expected)
}
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
    if !enabled {
        authority.stop_revoked = true; authority.exit_confirmed = false;
        if let Some(pending) = authority.admitted.as_mut() { pending.revoked = true; pending.exit_confirmed = false; }
    }
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
