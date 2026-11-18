//! Isolated Discord worker and bounded observation journal. Never owns capture.
mod evidence;
mod journal;
#[cfg(target_os = "macos")]
mod macos;

use evidence::{Facts, Presence, Tracker, Transition};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use tauri::{AppHandle, Emitter, Runtime};

static RUNNING: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);
static JOURNAL: Mutex<Option<journal::Journal<Status>>> = Mutex::new(None);
static CLOCK: OnceLock<std::time::Instant> = OnceLock::new();
fn now_ms() -> u64 {
    CLOCK
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// Observation setting, not supported-call/recording state.
    pub enabled: bool,
    pub state: &'static str,
    pub reason: &'static str,
    pub session_id: Option<String>,
    pub transition: &'static str,
    pub qualified: bool,
    /// This OS has the reader compiled in; NOT a claim of call-class support.
    pub platform_supported: bool,
    pub recording_active: bool,
    pub client_present: Option<bool>,
    pub evidence: Value,
    /// A control-shape change for comparison only, never confirmed leave.
    pub cue_transition: &'static str,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            enabled: false,
            state: "unknown",
            reason: "disabled",
            session_id: None,
            transition: "none",
            qualified: false,
            platform_supported: cfg!(target_os = "macos"),
            recording_active: false,
            client_present: None,
            evidence: facts_json(&Facts::default()),
            cue_transition: "none",
        }
    }
}
fn facts_json(f: &Facts) -> Value {
    json!({"complete": f.complete, "connected": f.connected, "disconnect": f.disconnect,
        "reconnect": f.reconnect, "join": f.join, "connected_pair": f.connected_pair})
}

#[tauri::command]
pub fn get_discord_automation_status() -> Status {
    let mut status = STATUS.lock().unwrap().clone().unwrap_or_default();
    status.enabled = crate::meeting_detection::discord_automation_enabled();
    if !status.enabled {
        status.state = "unknown";
        status.reason = "disabled";
        status.client_present = None;
        status.evidence = facts_json(&Facts::default());
        status.cue_transition = "none";
        status.transition = "none";
    }
    status
}
/// No setting/journal/epoch ticket can substitute for physical qualification.
/// A future qualified adapter must supply its own current session, never Zoom's.
pub fn qualified_owner(session: &str) -> Result<crate::audio::recording_ownership::LifecycleOwner, String> {
    let status = get_discord_automation_status();
    if !status.enabled || !status.qualified || status.state != "active"
        || status.session_id.as_deref() != Some(session) {
        return Err("Discord automation is observation-only or its qualified session is no longer current".into());
    }
    Ok(crate::audio::recording_ownership::LifecycleOwner {
        source: "discord".into(), session_id: session.into(), app: "Discord".into(),
        process: format!("discord-macos:{session}"),
    })
}

fn journal_json(j: &journal::Journal<Status>) -> Value {
    json!({"schema": 1, "mode": "observation_only", "scenario": j.scenario,
        "build_version": env!("CARGO_PKG_VERSION"), "build_head": option_env!("MEETILY_BUILD_SHA"),
        "active": j.active, "truncated": j.truncated, "max_entries": journal::MAX_ENTRIES,
        "entries": j.entries.iter().enumerate().map(|(seq, e)| json!({
            "seq": seq, "at_ms": e.at_ms, "scan_from_ms": e.scan_from_ms,
            "truth": e.truth, "sample": e.sample,
        })).collect::<Vec<_>>()})
}

#[tauri::command]
pub async fn start_discord_observation(scenario: String) -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Err("Discord AX observation is macOS-only".into());
    }
    if !crate::meeting_detection::discord_automation_enabled() {
        return Err("Enable master detection and Discord observation first".into());
    }
    if crate::audio::recording_commands::is_recording().await {
        return Err("Stop recording before collecting observation-only evidence".into());
    }
    let mut guard = JOURNAL.lock().unwrap();
    // Never overwrite evidence, even a stopped scenario, without explicit Clear.
    if guard.is_some() {
        return Err("Export and clear the previous scenario first".into());
    }
    let j = journal::Journal::new(&scenario, now_ms())?;
    let result = journal_json(&j);
    *guard = Some(j);
    Ok(result)
}
#[tauri::command]
pub fn mark_discord_observation(truth: String) -> Result<Value, String> {
    let mut guard = JOURNAL.lock().unwrap();
    let j = guard.as_mut().ok_or("Start a scenario first")?;
    j.mark(&truth, now_ms())?;
    Ok(
        json!({"active": j.active, "entries": j.entries.len(), "last_at_ms": j.entries.last().map(|e| e.at_ms)}),
    )
}
#[tauri::command]
pub fn stop_discord_observation() -> Result<Value, String> {
    let mut guard = JOURNAL.lock().unwrap();
    let j = guard.as_mut().ok_or("No observation journal")?;
    j.active = false;
    Ok(journal_json(j))
}
#[tauri::command]
pub fn get_discord_observation() -> Option<Value> {
    JOURNAL.lock().unwrap().as_ref().map(journal_json)
}
#[tauri::command]
pub fn clear_discord_observation() -> Result<(), String> {
    let mut guard = JOURNAL.lock().unwrap();
    if guard.as_ref().is_some_and(|j| j.active) {
        return Err("Stop the observation before clearing it".into());
    }
    *guard = None;
    Ok(())
}

pub fn initialize<R: Runtime>(app: &AppHandle<R>) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tracker = Tracker::default();
        let epoch = uuid::Uuid::new_v4().to_string();
        let mut previous_pair = false;
        loop {
            let from_ms = now_ms();
            let enabled = crate::meeting_detection::discord_automation_enabled();
            let unknown = |reason| evidence::Probe {
                presence: Presence::Unknown,
                reason,
                client_present: None,
                facts: Facts::default(),
            };
            let probe = if enabled {
                #[cfg(target_os = "macos")]
                {
                    tokio::task::spawn_blocking(macos::probe)
                        .await
                        .unwrap_or_else(|_| unknown("scan_failed"))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    unknown("macos_only")
                }
            } else {
                unknown("disabled")
            };
            let enabled = enabled && crate::meeting_detection::discord_automation_enabled();
            let presence = if enabled {
                probe.presence
            } else {
                Presence::Unknown
            };
            let transition = tracker.observe(presence, now_ms());
            let session = match transition {
                Transition::End(id) => Some(id),
                _ => tracker.session,
            };
            let cue = if !enabled {
                "none"
            } else if probe.facts.reconnect {
                "reconnect_cue"
            } else if previous_pair && probe.facts.complete && !probe.facts.connected_pair {
                "connected_controls_disappeared_unknown"
            } else if !previous_pair && probe.facts.connected_pair {
                "connected_controls_appeared_candidate"
            } else {
                "none"
            };
            if enabled && probe.facts.complete {
                previous_pair = probe.facts.connected_pair;
            } else {
                previous_pair = false;
            }
            let status = Status {
                enabled,
                state: match presence {
                    Presence::Active => "active",
                    Presence::Inactive => "inactive",
                    Presence::Unknown => "unknown",
                },
                reason: if enabled { probe.reason } else { "disabled" },
                session_id: session.map(|id| format!("{epoch}:{id}")),
                transition: match transition {
                    Transition::Start(_) => "start",
                    Transition::End(_) => "end",
                    Transition::None => "none",
                },
                // No qualification policy exists yet. Neither journal commands
                // nor configuration can unlock recording from candidate signals.
                qualified: false,
                platform_supported: cfg!(target_os = "macos"),
                recording_active: crate::audio::recording_commands::is_recording().await,
                client_present: if enabled { probe.client_present } else { None },
                evidence: if enabled {
                    facts_json(&probe.facts)
                } else {
                    facts_json(&Facts::default())
                },
                cue_transition: cue,
            };
            *STATUS.lock().unwrap() = Some(status.clone());
            if let Some(j) = JOURNAL.lock().unwrap().as_mut() {
                j.sample(status.clone(), from_ms, now_ms());
            }
            let _ = app.emit("discord-call-state", status);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}
