//! Discord-specific monitor; deliberately independent of the Zoom/Windows detector.
//! One worker owns AX scans and lifecycle state for the entire native app lifetime.
mod evidence;
#[cfg(target_os = "macos")]
mod macos;

use evidence::{Presence, Tracker, Transition};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, Runtime};

static RUNNING: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Option<Status>> = Mutex::new(None);

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub state: &'static str,
    pub reason: &'static str,
    pub session_id: Option<String>,
    pub transition: &'static str,
    pub qualified: bool,
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
        }
    }
}

#[tauri::command]
pub fn get_discord_automation_status() -> Status {
    let mut status = STATUS.lock().unwrap().clone().unwrap_or_default();
    // Settings can be disabled while an AX scan is running. Never authorize a
    // pending recording from the previous worker snapshot after that change.
    status.enabled = crate::meeting_detection::discord_automation_enabled();
    status
}

pub fn initialize<R: Runtime>(app: &AppHandle<R>) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tracker = Tracker::default();
        let epoch = uuid::Uuid::new_v4().to_string();
        let clock = std::time::Instant::now();
        loop {
            let enabled = crate::meeting_detection::discord_automation_enabled();
            let probe = if enabled {
                #[cfg(target_os = "macos")]
                {
                    tokio::task::spawn_blocking(macos::probe)
                        .await
                        .unwrap_or(evidence::Probe {
                            presence: Presence::Unknown,
                            reason: "scan_failed",
                        })
                }
                #[cfg(not(target_os = "macos"))]
                {
                    evidence::Probe {
                        presence: Presence::Unknown,
                        reason: "macos_only",
                    }
                }
            } else {
                evidence::Probe {
                    presence: Presence::Unknown,
                    reason: "disabled",
                }
            };
            let enabled = enabled && crate::meeting_detection::discord_automation_enabled();
            let presence = if enabled {
                probe.presence
            } else {
                Presence::Unknown
            };
            let transition = tracker.observe(presence, clock.elapsed().as_millis() as u64);
            let session = match transition {
                Transition::End(id) => Some(id),
                _ => tracker.session,
            };
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
                // AX labels and leave-with-client-open are not physically
                // qualified. Never authorize capture from this candidate yet.
                qualified: false,
            };
            *STATUS.lock().unwrap() = Some(status.clone());
            // Repeated snapshots allow a reloaded main WebView to reconcile;
            // session identity prevents duplicate starts and respects manual Stop.
            let _ = app.emit("discord-call-state", status);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}
