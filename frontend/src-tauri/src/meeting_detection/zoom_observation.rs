//! Bounded observation-only qualification journal. Ground truth markers never
//! feed the detector or recording authority. No raw AX strings, audio or paths.
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Truth { Idle, Preview, Waiting, Joined, JoinedMuted, Silence, Reconnecting, DeviceChanged, LeftOpen, PermissionLost }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Machine { M4Pro48Gb, M1Pro32Gb, Other }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route { Speakers, Headset, Other, Unknown }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language { English, Russian, Other }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission { Granted, Denied, Unknown }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub machine: Machine, pub route: Route, pub client_version: String, pub macos_version: String,
    pub ui_language: Language, pub accessibility: Permission,
}
#[derive(Clone, Serialize)]
struct Entry {
    unix_ms: u64, elapsed_ms: u64, kind: &'static str,
    truth: Option<Truth>, presence: Option<String>, reason: Option<String>,
    session_id: Option<String>, qualified: bool,
}
#[derive(Serialize)]
struct Journal {
    schema: u8, source: &'static str, physical_acceptance: &'static str,
    metadata: Metadata, recording: bool, capacity_reached: bool, entries: Vec<Entry>,
    #[serde(skip)] clock: Instant,
}
const CAPACITY: usize = 3_600;
static JOURNAL: LazyLock<Mutex<Option<Journal>>> = LazyLock::new(|| Mutex::new(None));
fn unix_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
fn version(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_alphanumeric() || ".()-_ ".contains(c))
}
impl Journal {
    fn push(&mut self, kind: &'static str, truth: Option<Truth>, presence: Option<&str>, reason: Option<&str>, session_id: Option<&str>) {
        if !self.recording { return; }
        if self.entries.len() >= CAPACITY { self.capacity_reached = true; self.recording = false; return; }
        self.entries.push(Entry { unix_ms: unix_ms(), elapsed_ms: self.clock.elapsed().as_millis() as u64,
            kind, truth, presence: presence.map(str::to_string), reason: reason.map(str::to_string),
            session_id: session_id.map(str::to_string), qualified: false });
    }
}
#[tauri::command]
pub fn start_zoom_observation(metadata: Metadata) -> Result<(), String> {
    if !version(&metadata.client_version) || !version(&metadata.macos_version) { return Err("Enter client/macOS version numbers (max 64 version characters).".into()); }
    let mut guard = JOURNAL.lock().unwrap();
    // Do not silently destroy an earlier unexported journal.
    if guard.is_some() { return Err("Export and clear the existing Zoom journal before starting another.".into()); }
    let mut journal = Journal { schema: 1, source: "zoom", physical_acceptance: "not_run", metadata,
        recording: true, capacity_reached: false, entries: Vec::new(), clock: Instant::now() };
    journal.push("start", None, None, None, None);
    *guard = Some(journal); Ok(())
}
#[tauri::command]
pub fn mark_zoom_observation(truth: Truth) -> Result<(), String> {
    let mut guard = JOURNAL.lock().unwrap();
    let journal = guard.as_mut().ok_or("Start a Zoom observation journal first.")?;
    if !journal.recording { return Err("The observation journal is stopped or full. Export it first.".into()); }
    journal.push("truth", Some(truth), None, None, None); Ok(())
}
#[tauri::command]
pub fn stop_zoom_observation() { if let Some(journal) = JOURNAL.lock().unwrap().as_mut() { journal.push("stop", None, None, None, None); journal.recording = false; } }
#[tauri::command]
pub fn export_zoom_observation() -> Result<String, String> {
    let guard = JOURNAL.lock().unwrap();
    serde_json::to_string_pretty(guard.as_ref().ok_or("No Zoom observation journal to export.")?).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn clear_zoom_observation() { *JOURNAL.lock().unwrap() = None; }
/// Only fixed detector enums/reasons are accepted, not arbitrary AX text. Unknown
/// permission loss is logged as Unknown and can never become a leave marker.
pub fn observe(presence: &str, reason: &str, session: Option<&str>) {
    const REASONS: &[&str] = &["zoom_participant_and_leave_controls", "zoom_idle_home", "zoom_not_running",
        "accessibility_permission_required", "zoom_scan_timeout", "zoom_scan_limit", "zoom_ax_read_failed",
        "zoom_ax_incomplete", "zoom_controls_not_found", "zoom_inventory_unavailable", "zoom_adapter_disabled",
        "scan_failed", "monitor_restarting", "detection_disabled"];
    let presence = if ["active", "inactive", "unknown"].contains(&presence) { presence } else { "unknown" };
    let reason = if REASONS.contains(&reason) { reason } else { "unrecognized_reason" };
    if let Some(journal) = JOURNAL.lock().unwrap().as_mut() { journal.push("observation", None, Some(presence), Some(reason), session); }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_settings_ipc_metadata_uses_closed_enums_and_bounded_versions() {
        let metadata: Metadata = serde_json::from_value(serde_json::json!({
            "machine":"m4_pro48_gb", "route":"speakers", "client_version":"6.4.0", "macos_version":"15.1",
            "ui_language":"english", "accessibility":"unknown"
        })).unwrap();
        assert!(version(&metadata.client_version));
        assert!(!version(&"x".repeat(65)));
        assert!(!version("private\nwindow text"));
    }
    #[test]
    fn truth_and_permission_loss_are_separate_timestamped_observations_without_capture_authority() {
        clear_zoom_observation();
        start_zoom_observation(Metadata { machine: Machine::M4Pro48Gb, route: Route::Speakers, client_version: "6.0.1".into(), macos_version: "15.1".into(), ui_language: Language::English, accessibility: Permission::Granted }).unwrap();
        mark_zoom_observation(Truth::Preview).unwrap(); observe("unknown", "accessibility_permission_required", None);
        mark_zoom_observation(Truth::JoinedMuted).unwrap(); observe("active", "zoom_participant_and_leave_controls", Some("fixture"));
        observe("unknown", "private raw AX label", None);
        stop_zoom_observation();
        let text = export_zoom_observation().unwrap();
        assert!(!text.contains("private raw AX label"));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["physical_acceptance"], "not_run");
        for entry in json["entries"].as_array().unwrap() {
            assert_eq!(entry["qualified"], false);
            assert!(entry["unix_ms"].as_u64().unwrap() > 0);
            assert!(entry["elapsed_ms"].is_u64());
        }
        assert!(start_zoom_observation(Metadata { machine: Machine::Other, route: Route::Unknown, client_version: "1".into(), macos_version: "1".into(), ui_language: Language::Other, accessibility: Permission::Unknown }).is_err());
        clear_zoom_observation();
    }
}
