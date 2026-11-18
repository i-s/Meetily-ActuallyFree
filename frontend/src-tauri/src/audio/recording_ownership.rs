//! Process-lifetime recording authority. Call only at the engine lifecycle lock
//! for start/stop; the short mutex also linearizes enable/disable and ownership
//! release against an automated stop. No WebView promise owns this state.
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};

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
}
impl Default for Authority {
    fn default() -> Self {
        Self { epoch: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_nanos().to_string(), sequence: 0, current: None, automation_enabled: false }
    }
}
impl Authority {
    pub fn begin(&mut self, owner: Option<LifecycleOwner>) -> RecordingLease {
        self.sequence += 1;
        let lease = RecordingLease { generation: format!("{}:{}", self.epoch, self.sequence), owner };
        self.current = Some(lease.clone());
        lease
    }
    /// An automated stop must carry the exact native lease, even for legacy
    /// automation with no client owner. A manual stop deliberately has no guard.
    pub fn claim_stop(&mut self, expected: Option<&RecordingLease>) -> bool {
        if let Some(expected) = expected {
            if !self.automation_enabled || self.current.as_ref() != Some(expected) { return false; }
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

#[tauri::command]
pub fn set_meeting_automation_enabled(enabled: bool) {
    // Not loaded from old browser settings. Every native process starts false.
    AUTHORITY.lock().unwrap().automation_enabled = enabled;
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
}
