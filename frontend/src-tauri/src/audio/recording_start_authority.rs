//! Native start tickets fence delayed IPC across WebView reloads. A single
//! monotonic sequence is the durable-in-process rejection watermark; bounded
//! history is diagnostic detail, not the cancellation fence.
use super::recording_ownership::RecordingLease;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

pub const VERSION: u32 = 1;
const HISTORY_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartRequest {
    pub version: u32,
    pub epoch: String,
    /// Decimal string, not a JavaScript number (no 53-bit rounding).
    pub sequence: String,
    pub source: String,
    pub session_id: String,
    pub token: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct StartCapability {
    pub version: u32,
    pub epoch: String,
    pub next_sequence: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartOutcome { Committed, Cancelled, Failed, Rejected, Retired, EpochMismatch }
#[derive(Clone, Debug, Serialize)]
pub struct StartResolution {
    pub version: u32,
    pub native_epoch: String,
    pub expected: StartRequest,
    pub terminal: bool,
    pub no_future_commit: bool,
    pub outcome: StartOutcome,
    /// Historical lease for this request when retained; NOT the current take.
    pub committed_lease: Option<RecordingLease>,
    pub recording_lease: Option<RecordingLease>,
    pub is_recording: bool,
    pub is_stopping: bool,
}
#[derive(Clone)]
struct Terminal {
    request: StartRequest,
    outcome: StartOutcome,
    lease: Option<RecordingLease>,
}
pub struct StartAuthority {
    epoch: String,
    next: u64,
    inflight: Option<StartRequest>,
    history: VecDeque<Terminal>,
}
impl Default for StartAuthority {
    fn default() -> Self {
        Self { epoch: uuid::Uuid::new_v4().to_string(), next: 1, inflight: None, history: VecDeque::new() }
    }
}
impl StartAuthority {
    pub fn capability(&self) -> StartCapability {
        StartCapability { version: VERSION, epoch: self.epoch.clone(), next_sequence: self.next.to_string() }
    }
    fn sequence(request: &StartRequest) -> Result<u64, String> {
        if request.version != VERSION { return Err("Unsupported recording start authority version".into()); }
        for (value, limit) in [(&request.epoch, 64), (&request.source, 128), (&request.session_id, 256), (&request.token, 256)] {
            if value.is_empty() || value.len() > limit { return Err("Invalid recording start request binding".into()); }
        }
        let sequence = request.sequence.parse::<u64>().map_err(|_| "Invalid recording start sequence")?;
        if sequence == 0 || sequence == u64::MAX || sequence.to_string() != request.sequence {
            return Err("Invalid recording start sequence".into());
        }
        Ok(sequence)
    }
    /// Non-disruptive eligibility read, not admission or a terminal receipt.
    /// A later claim must repeat this under the engine lifecycle lock.
    pub fn preflight(&self, request: &StartRequest) -> Result<(), String> {
        let sequence = Self::sequence(request)?;
        if request.epoch != self.epoch { return Err("Recording start process epoch changed".into()); }
        if sequence != self.next || self.inflight.is_some() { return Err("Recording start ticket is stale, conflicting or out of order".into()); }
        Ok(())
    }
    pub fn claim(&mut self, request: &StartRequest) -> Result<(), String> {
        self.preflight(request)?;
        self.next += 1;
        self.inflight = Some(request.clone());
        Ok(())
    }
    fn remember(&mut self, terminal: Terminal) {
        if self.history.len() == HISTORY_LIMIT { self.history.pop_front(); }
        self.history.push_back(terminal);
    }
    fn finish(&mut self, request: &StartRequest, lease: Option<RecordingLease>) {
        if self.inflight.as_ref() != Some(request) { return; }
        self.inflight = None;
        self.remember(Terminal { request: request.clone(), outcome: if lease.is_some() { StartOutcome::Committed } else { StartOutcome::Failed }, lease });
    }
    /// Call under the engine lifecycle lock, AFTER an admitted start has settled.
    /// A future sequence is not fenced: never return a false terminal receipt.
    pub fn settle(&mut self, request: &StartRequest) -> Result<StartResolution, String> {
        let sequence = Self::sequence(request)?;
        let (outcome, committed_lease) = if request.epoch != self.epoch {
            (StartOutcome::EpochMismatch, None)
        } else if self.inflight.as_ref().is_some_and(|r| r.sequence == request.sequence) {
            return Err("Recording start is still in flight; no terminal receipt".into());
        } else if sequence > self.next {
            return Err("Future recording start sequence is not fenced; no terminal receipt".into());
        } else if sequence == self.next {
            if self.inflight.is_some() { return Err("Another start is unsettled; no terminal receipt".into()); }
            self.next += 1;
            self.remember(Terminal { request: request.clone(), outcome: StartOutcome::Cancelled, lease: None });
            (StartOutcome::Cancelled, None)
        } else if let Some(result) = self.history.iter().find(|t| t.request.sequence == request.sequence) {
            if result.request == *request { (result.outcome, result.lease.clone()) }
            else { (StartOutcome::Rejected, None) }
        } else {
            // Only detail was evicted. Every sequence below next is forever
            // unusable; Retired must not be interpreted as "never recorded".
            (StartOutcome::Retired, None)
        };
        Ok(StartResolution { version: VERSION, native_epoch: self.epoch.clone(), expected: request.clone(),
            terminal: true, no_future_commit: true, outcome, committed_lease,
            recording_lease: None, is_recording: false, is_stopping: false })
    }
}
static STARTS: LazyLock<Mutex<StartAuthority>> = LazyLock::new(|| Mutex::new(StartAuthority::default()));

#[tauri::command]
pub fn get_recording_start_capability() -> StartCapability { STARTS.lock().unwrap().capability() }

/// Owned by the native start future, never the initiating WebView. Early return
/// or future destruction settles failure; a barrier still waits for engine lock.
pub struct StartClaim { expected: StartRequest }
pub fn preflight(expected: &StartRequest) -> Result<(), String> {
    STARTS.lock().unwrap().preflight(expected)
}
/// Retire only a still-next well-formed ticket after preflight rejection. This
/// grants no setup/capture ownership. Reuse the existing watermark/history under
/// one mutex; malformed/future/stale/in-flight tickets cannot fence other work.
pub fn reject_unadmitted(expected: &StartRequest) {
    let mut starts = STARTS.lock().unwrap();
    if starts.claim(expected).is_ok() {
        starts.finish(expected, None);
    }
}
pub fn claim(expected: &StartRequest) -> Result<StartClaim, String> {
    STARTS.lock().unwrap().claim(expected)?;
    Ok(StartClaim { expected: expected.clone() })
}
impl StartClaim {
    pub fn commit(self, lease: RecordingLease) {
        STARTS.lock().unwrap().finish(&self.expected, Some(lease));
    }
}
impl Drop for StartClaim {
    fn drop(&mut self) { STARTS.lock().unwrap().finish(&self.expected, None); }
}
pub fn settle(expected: &StartRequest) -> Result<StartResolution, String> { STARTS.lock().unwrap().settle(expected) }

#[cfg(test)]
mod tests {
    use super::*;
    fn request(authority: &StartAuthority, token: &str) -> StartRequest {
        let cap = authority.capability();
        StartRequest { version: cap.version, epoch: cap.epoch, sequence: cap.next_sequence,
            source: "zoom".into(), session_id: "synthetic".into(), token: token.into() }
    }
    #[test]
    fn future_error_is_not_a_fence_until_sequence_becomes_next() {
        let mut authority = StartAuthority::default();
        let first = request(&authority, "a");
        let mut future = first.clone(); future.sequence = "2".into(); future.token = "b".into();
        assert!(authority.settle(&future).is_err());
        assert_eq!(authority.next, 1);
        assert_eq!(authority.settle(&first).unwrap().outcome, StartOutcome::Cancelled);
        // The previous error did NOT fence B: B is now a valid next request.
        authority.claim(&future).unwrap();
        assert!(authority.settle(&future).is_err());
        authority.finish(&future, None);
        assert_eq!(authority.settle(&future).unwrap().outcome, StartOutcome::Failed);
        let mut next = future.clone(); next.sequence = "3".into();
        assert_eq!(authority.settle(&next).unwrap().outcome, StartOutcome::Cancelled);
        assert!(authority.claim(&next).is_err());
    }
    #[test]
    fn bounded_detail_eviction_never_reopens_old_sequence() {
        let mut authority = StartAuthority::default();
        let first = request(&authority, "first");
        for n in 0..HISTORY_LIMIT + 5 {
            let request = request(&authority, &format!("token-{n}"));
            let receipt = authority.settle(&request).unwrap();
            assert!(receipt.terminal && receipt.no_future_commit);
        }
        assert_eq!(authority.history.len(), HISTORY_LIMIT);
        assert_eq!(authority.settle(&first).unwrap().outcome, StartOutcome::Retired);
        assert!(authority.claim(&first).is_err());
        assert_eq!(authority.history.len(), HISTORY_LIMIT);
    }
    #[test]
    fn conflicting_binding_and_restart_cannot_adopt_another_request() {
        let mut original = StartAuthority::default();
        let a = request(&original, "a");
        original.settle(&a).unwrap();
        let mut conflict = a.clone(); conflict.token = "b".into();
        assert_eq!(original.settle(&conflict).unwrap().outcome, StartOutcome::Rejected);
        assert!(original.claim(&conflict).is_err());
        let mut restarted = StartAuthority::default();
        let receipt = restarted.settle(&a).unwrap();
        assert_eq!(receipt.outcome, StartOutcome::EpochMismatch);
        assert_eq!(receipt.expected, a);
        assert_ne!(receipt.native_epoch, receipt.expected.epoch);
        assert_eq!(restarted.next, 1);
        assert!(restarted.claim(&a).is_err());
        let round_trip: StartRequest = serde_json::from_value(serde_json::to_value(&receipt).unwrap()["expected"].clone()).unwrap();
        assert_eq!(round_trip, a);
    }
}
