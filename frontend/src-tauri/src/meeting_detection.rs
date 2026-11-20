//! Native call evidence and session tracking. A running app alone is never a call.
//! Probes inspect only local native state; diagnostics contain fixed reason codes,
//! not accessibility text, window titles, chat content, or audio.

mod evidence;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows_discord;
use evidence::{Observation, Presence, Probe, Tracker, Transition};
use std::sync::{atomic::AtomicBool, OnceLock};

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Runtime};

/// Generation token invalidates a stopped loop, including an in-flight scan.
static MONITOR_GENERATION: AtomicU64 = AtomicU64::new(0);
/// Current settings, shared with the running loop so changes apply live.
static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static TRACKER: OnceLock<Mutex<Tracker>> = OnceLock::new();
static SETTINGS: Mutex<Option<MeetingDetectionSettings>> = Mutex::new(None);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeetingDetectionSettings {
    /// Master on/off switch.
    pub enabled: bool,
    /// How often to scan the process list, in seconds.
    pub interval_secs: u64,
    /// Candidate application keywords. Native call evidence is required.
    pub meeting_apps: Vec<String>,
    /// Process-name keywords to always ignore (suppress false positives).
    pub ignored_apps: Vec<String>,
    /// Also raise a native OS notification (handled on the frontend) in
    /// addition to the in-app prompt.
    pub notify: bool,
}

impl Default for MeetingDetectionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 15,
            meeting_apps: default_meeting_apps(),
            ignored_apps: Vec::new(),
            notify: true,
        }
    }
}

/// Default keyword list. Deliberately excludes bare "meet" to avoid matching
/// this app's own process ("meetily") and unrelated software.
fn default_meeting_apps() -> Vec<String> {
    [
        "zoom",
        "discord",
        "teams",
        "msteams",
        "slack",
        "webex",
        "gotomeeting",
        "bluejeans",
        "chime",
        "skype",
        "ringcentral",
        "whereby",
        "jitsi",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Map a matched keyword to a friendly display name for the notification.
fn friendly_name(keyword: &str) -> String {
    match keyword {
        "zoom" => "Zoom",
        "discord" => "Discord",
        "teams" | "msteams" => "Microsoft Teams",
        "slack" => "Slack",
        "webex" => "Webex",
        "gotomeeting" => "GoToMeeting",
        "bluejeans" => "BlueJeans",
        "chime" => "Amazon Chime",
        "skype" => "Skype",
        "ringcentral" => "RingCentral",
        "whereby" => "Whereby",
        "jitsi" => "Jitsi",
        other => other,
    }
    .to_string()
}

fn config_path() -> std::path::PathBuf {
    crate::paths::install_data_root().join("meeting_detection.json")
}

fn load_settings_from_disk() -> MeetingDetectionSettings {
    let path = config_path();
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(mut settings) = serde_json::from_slice::<MeetingDetectionSettings>(&bytes) {
            sanitize_settings(&mut settings);
            return settings;
        }
    }
    MeetingDetectionSettings::default()
}

/// The settings UI exposes ignored_apps for opt-outs. Older versions forcibly
/// removed Discord, so restore it on upgrade while preserving explicit ignores.
fn sanitize_settings(settings: &mut MeetingDetectionSettings) {
    settings.interval_secs = settings.interval_secs.clamp(3, 3600);
    if !settings
        .meeting_apps
        .iter()
        .any(|k| k.eq_ignore_ascii_case("discord"))
    {
        settings.meeting_apps.push("discord".into());
    }
}

fn save_settings_to_disk(settings: &MeetingDetectionSettings) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Payload emitted when a meeting app is detected.
#[derive(Clone, Serialize)]
struct MeetingDetectedPayload {
    /// Friendly app name, e.g. "Zoom".
    app: String,
    /// Stable application key shared with automatic recording ownership.
    process: String,
    /// Whether the user asked for a native notification too.
    notify: bool,
    /// Confirmed call evidence (native UI for Zoom/Discord, media lease for others).
    active_media: bool,
    session_id: u64,
}

#[derive(Clone, Serialize)]
struct MeetingEndedPayload {
    app: String,
    process: String,
    session_id: u64,
}

/// True for our own process — never treat Meetily as a "meeting app".
fn is_self_process(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("meetily") || n.contains("meetily-actually")
}

/// Every application has a stable key, independent of process enumeration order,
/// helper PIDs, focus, notify settings, and microphone mute state.
fn scan_for_meetings(settings: &MeetingDetectionSettings, diagnostics: bool) -> Vec<Observation> {
    use sysinfo::System;
    let sys = System::new_all();
    let processes: Vec<(u32, String)> = sys
        .processes()
        .values()
        .map(|p| (p.pid().as_u32(), p.name().to_string_lossy().to_lowercase()))
        .collect();
    #[cfg(windows)]
    let media = windows_media_in_use_exes();
    let mut keys: Vec<String> = settings
        .meeting_apps
        .iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    let observations: Vec<_> = keys
        .into_iter()
        .filter(|key| !is_self_process(key))
        .map(|key| {
            let ignored = settings
                .ignored_apps
                .iter()
                .any(|s| !s.trim().is_empty() && key.contains(&s.trim().to_lowercase()));
            let pids: Vec<u32> = processes
                .iter()
                .filter(|(_, name)| {
                    !is_self_process(name)
                        && !settings.ignored_apps.iter().any(|s| {
                            !s.trim().is_empty() && name.contains(&s.trim().to_lowercase())
                        })
                        && match key.as_str() {
                            "zoom" => matches!(name.as_str(), "zoom.us" | "zoom" | "zoom.exe"),
                            "discord" => matches!(
                                name.as_str(),
                                "discord" | "discord.exe" | "discordptb.exe" | "discordcanary.exe"
                            ),
                            _ => process_matches_keyword(name, &key),
                        }
                })
                .map(|(pid, _)| *pid)
                .collect();
            let probe = if ignored {
                Probe {
                    presence: Presence::Inactive,
                    reason: "Ignored in settings",
                }
            } else if pids.is_empty() {
                Probe {
                    presence: Presence::Inactive,
                    reason: "Application is not running",
                }
            } else {
                platform_probe(
                    &key,
                    &pids,
                    diagnostics,
                    #[cfg(windows)]
                    &media,
                )
            };
            Observation {
                app: friendly_name(&key),
                process: key.clone(),
                key,
                active_media: probe.presence == Presence::Active,
                probe,
            }
        })
        .collect();
    #[cfg(windows)]
    let observations = {
        let mut observations = observations;
        let browser_pids = windows_browser_meeting_pids();
        let active = processes.iter().any(|(pid, name)| {
            is_browser(name)
                && browser_pids.contains(pid)
                && media.contains(name)
                && !settings
                    .ignored_apps
                    .iter()
                    .any(|s| !s.trim().is_empty() && name.contains(&s.trim().to_lowercase()))
        });
        observations.push(Observation {
            key: "browser".into(),
            app: "Browser meeting".into(),
            process: "browser".into(),
            active_media: active,
            probe: Probe {
                presence: if active {
                    Presence::Active
                } else if processes.iter().any(|(_, name)| is_browser(name)) {
                    Presence::Unknown
                } else {
                    Presence::Inactive
                },
                reason: "Browser meeting title and microphone/camera use",
            },
        });
        observations
    };
    observations
}

fn platform_probe(
    key: &str,
    pids: &[u32],
    diagnostics: bool,
    #[cfg(windows)] media: &std::collections::HashSet<String>,
) -> Probe {
    #[cfg(target_os = "macos")]
    if key == "zoom" {
        return macos::probe(pids, diagnostics);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = diagnostics;
    #[cfg(windows)]
    {
        if key == "discord" {
            return windows_discord::probe(pids);
        }
        if media
            .iter()
            .any(|name| process_matches_keyword(name, key) && !is_self_process(name))
        {
            return Probe {
                presence: Presence::Active,
                reason: "Windows reports microphone or camera use by this app",
            };
        }
        return Probe {
            presence: Presence::Unknown,
            reason: "No active microphone or camera evidence; process presence is insufficient",
        };
    }
    #[cfg(not(windows))]
    {
        let _ = (key, pids);
        Probe {
            presence: Presence::Unknown,
            reason: "Call detection for this app is not supported on this platform",
        }
    }
}

#[derive(Serialize)]
pub struct DetectionDiagnostic {
    app: String,
    state: &'static str,
    reason: &'static str,
}
#[derive(Serialize)]
pub struct DetectionDiagnostics {
    platform: &'static str,
    accessibility_required: bool,
    accessibility_granted: bool,
    apps: Vec<DetectionDiagnostic>,
}

fn diagnostic_reason(reason: &'static str) -> &'static str {
    match reason {
        "accessibility_permission_required" => "Allow Meetily in System Settings > Privacy & Security > Accessibility, then check again.",
        "zoom_enabled_meeting_command" | "zoom_enabled_call_controls" => "Zoom exposes enabled controls for an ongoing meeting.",
        "zoom_idle_home" => "Zoom shows its home screen with no active meeting.",
        "zoom_scan_timeout" => "Zoom's accessibility scan timed out. Refresh detection writes a technical report to the Meetily log.",
        "zoom_scan_limit" => "Zoom's accessibility scan reached its safety limit. Refresh detection writes a technical report to the Meetily log.",
        "zoom_ax_read_failed" => "Accessibility permission is granted, but reading Zoom's interface failed. Refresh detection writes the API error to the Meetily log.",
        "zoom_ax_incomplete" => "Zoom did not expose a required accessibility attribute. Refresh detection writes a technical report to the Meetily log.",
        "zoom_controls_not_found" => "Zoom's interface was read, but no supported call controls were found. Refresh detection writes a technical report to the Meetily log.",
        "zoom_ui_unknown" => "Zoom's call controls could not be confirmed. Keep Zoom open and check Accessibility permission.",
        "zoom_not_running" | "discord_process_absent" => "Application is not running.",
        "discord_call_ui_and_capture" => "Discord exposes call controls and an active microphone session.",
        "discord_call_ui" => "Discord exposes call controls; microphone activity is not required.",
        "discord_idle_ui" => "Discord shows its idle controls with no connected call.",
        "discord_capture_without_call_ui" => "Discord is using audio, but no call controls were found. A microphone test is not a call.",
        "discord_call_end_unconfirmed_while_app_open" => "No call controls are visible. Discord is still open, so call end cannot be confirmed; stop recording manually if needed.",
        "discord_probe_timeout" | "discord_probe_busy" => "Discord inspection did not finish in time. The current call state is unknown.",
        "discord_probe_worker_unavailable" | "discord_ui_unavailable" => "Discord accessibility inspection is unavailable. Open Discord and check again.",
        "discord_ui_unavailable_or_ambiguous" => "Discord's call controls could not be confirmed. Open its window and check again.",
        _ => reason,
    }
}

#[tauri::command]
pub async fn get_meeting_detection_diagnostics() -> Result<DetectionDiagnostics, String> {
    let settings = SETTINGS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(load_settings_from_disk);
    let observations = tokio::task::spawn_blocking(move || scan_for_meetings(&settings, true))
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let accessibility_granted = macos::accessibility_trusted();
    #[cfg(not(target_os = "macos"))]
    let accessibility_granted = true;
    Ok(DetectionDiagnostics {
        platform: std::env::consts::OS,
        accessibility_required: cfg!(target_os = "macos"),
        accessibility_granted,
        apps: observations
            .into_iter()
            .filter(|o| {
                o.probe.reason != "Application is not running"
                    || (cfg!(target_os = "macos") && o.key == "zoom")
                    || (cfg!(windows) && o.key == "discord")
            })
            .map(|o| DetectionDiagnostic {
                app: o.app,
                reason: diagnostic_reason(o.probe.reason),
                state: match o.probe.presence {
                    Presence::Active => "Call detected",
                    Presence::Inactive => "No call",
                    Presence::Unknown => "Unknown",
                },
            })
            .collect(),
    })
}

#[tauri::command]
pub async fn request_meeting_detection_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(macos::request_accessibility())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Accessibility permission is only required on macOS".into())
    }
}

/// Revalidate a displayed offer before starting, so a stale notification cannot
/// record a later unrelated session. This inspection does not advance debounce.
#[tauri::command]
pub async fn meeting_detection_session_exists(process: String, session_id: u64) -> bool {
    TRACKER
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .session_matches(&process, session_id)
}

#[tauri::command]
pub async fn validate_meeting_detection_session(
    process: String,
    session_id: u64,
) -> Result<bool, String> {
    let settings = SETTINGS.lock().unwrap().clone().unwrap_or_default();
    if !settings.enabled
        || !TRACKER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .session_matches(&process, session_id)
    {
        return Ok(false);
    }
    let observations = tokio::task::spawn_blocking(move || scan_for_meetings(&settings, false))
        .await
        .map_err(|e| e.to_string())?;
    Ok(observations
        .iter()
        .any(|o| o.key == process && o.probe.presence == Presence::Active)
        && TRACKER
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .session_matches(&process, session_id))
}

/// Windows: executables currently holding microphone or webcam via
/// CapabilityAccessManager NonPackaged consent-store entries
/// (LastUsedTimeStart > LastUsedTimeStop ⇒ in use).
#[cfg(windows)]
fn windows_media_in_use_exes() -> std::collections::HashSet<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let mut out = std::collections::HashSet::new();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for cap in ["microphone", "webcam"] {
        for suffix in ["NonPackaged", ""] {
            let path = format!(
                "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\{cap}\\{suffix}"
            );
            let Ok(root) = hkcu.open_subkey(&path) else {
                continue;
            };
            let Ok(keys) = root.enum_keys().collect::<Result<Vec<_>, _>>() else {
                continue;
            };
            for key_name in keys {
                if key_name.eq_ignore_ascii_case("NonPackaged") {
                    continue;
                }
                let Ok(sub) = root.open_subkey(&key_name) else {
                    continue;
                };
                // Values are FILETIME-like u64; Start > Stop means currently open.
                let start: u64 = sub.get_value("LastUsedTimeStart").unwrap_or(0);
                let stop: u64 = sub.get_value("LastUsedTimeStop").unwrap_or(0);
                if start == 0 {
                    continue;
                }
                // 0xFFFFFFFFFFFFFFFF stop means "still in use" on some builds;
                // otherwise start > stop.
                let in_use = stop == u64::MAX || start > stop;
                if !in_use {
                    continue;
                }
                // Key names look like C:#Program Files#...#Teams.exe
                let exe = key_name
                    .rsplit('#')
                    .next()
                    .unwrap_or(&key_name)
                    .to_lowercase();
                if exe.ends_with(".exe") {
                    out.insert(exe);
                } else if key_name.to_lowercase().contains("msteams") {
                    // Packaged Teams stores a package family name rather than an exe.
                    out.insert("ms-teams.exe".into());
                }
            }
        }
    }
    out
}

#[cfg(windows)]
fn is_browser(name: &str) -> bool {
    matches!(name, "chrome.exe" | "msedge.exe" | "firefox.exe")
}

#[cfg(windows)]
fn windows_browser_meeting_pids() -> std::collections::HashSet<u32> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    };
    unsafe extern "system" fn visit(hwnd: HWND, state: isize) -> i32 {
        let matches = &mut *(state as *mut std::collections::HashSet<u32>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 || len > 1024 {
            return 1;
        }
        let mut title = vec![0u16; len as usize + 1];
        let copied = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
        if copied <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&title[..copied as usize]).to_lowercase();
        if browser_title_is_meeting(&title) {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != 0 {
                matches.insert(pid);
            }
        }
        1
    }
    let mut matches = std::collections::HashSet::new();
    unsafe {
        EnumWindows(Some(visit), &mut matches as *mut _ as isize);
    }
    matches
}

#[cfg(windows)]
fn browser_title_is_meeting(title: &str) -> bool {
    let title = title.to_lowercase();
    [
        "google meet",
        "meet.google.com",
        "zoom meeting",
        "zoom.us/j/",
        "meeting | microsoft teams",
        "teams.microsoft.com",
        "slack huddle",
    ]
    .iter()
    .any(|needle| title.contains(needle))
}

/// Does a process name match a meeting-app keyword?
///
/// We deliberately do NOT use a raw substring test. Naive `contains("teams")`
/// famously matches `steamservice.exe` (Steam) — `s[teams]ervice` — and reports
/// a phantom "Microsoft Teams meeting." Instead we split the process name into
/// alphanumeric tokens (so `.exe`, `-`, `_`, spaces, and digits are boundaries)
/// and require a *token* to start with the keyword. This keeps forgiving
/// matches that should work — `ms-teams.exe` -> ["ms","teams"], `webexmta.exe`
/// -> ["webexmta"] — while rejecting keywords buried mid-word like the Steam
/// service.
fn process_matches_keyword(process_name: &str, keyword: &str) -> bool {
    process_name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| !token.is_empty() && token.starts_with(keyword))
}

#[cfg(test)]
mod tests {
    use super::{is_self_process, process_matches_keyword};

    #[test]
    fn never_treats_meetily_as_a_meeting() {
        assert!(is_self_process("meetily.exe"));
        assert!(is_self_process("Meetily - Actually Free.exe"));
        assert!(!is_self_process("ms-teams.exe"));
    }

    #[test]
    fn matches_real_meeting_apps() {
        assert!(process_matches_keyword("teams.exe", "teams"));
        assert!(process_matches_keyword("ms-teams.exe", "teams"));
        assert!(process_matches_keyword("msteams.exe", "msteams"));
        assert!(process_matches_keyword("zoom.exe", "zoom"));
        assert!(process_matches_keyword("webexmta.exe", "webex"));
        assert!(process_matches_keyword("slack.exe", "slack"));
    }

    #[test]
    fn rejects_mid_word_false_positives() {
        // The reported bug: Steam's service is not a Teams meeting.
        assert!(!process_matches_keyword("steamservice.exe", "teams"));
        assert!(!process_matches_keyword("steam.exe", "teams"));
        assert!(!process_matches_keyword("steamwebhelper.exe", "teams"));
    }
}

/// Settings updates share this loop and Tracker; changing notify/interval must
/// not manufacture another call. Restarting a stopped loop retains session IDs.
fn start_monitor<R: Runtime>(app: &AppHandle<R>) {
    if !SETTINGS.lock().unwrap().as_ref().is_some_and(|s| s.enabled) {
        return;
    }
    if MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    TRACKER
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .suspend();
    let generation = MONITOR_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let clock = std::time::Instant::now();
        // Global monotonic origin survives monitor restarts alongside Tracker.
        static ORIGIN: OnceLock<std::time::Instant> = OnceLock::new();
        let origin = *ORIGIN.get_or_init(|| clock);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        while MONITOR_GENERATION.load(Ordering::SeqCst) == generation {
            let current = SETTINGS.lock().unwrap().clone().unwrap_or_default();
            if !current.enabled {
                break;
            }
            let scan_settings = current.clone();
            let scan =
                tokio::task::spawn_blocking(move || scan_for_meetings(&scan_settings, false)).await;
            if MONITOR_GENERATION.load(Ordering::SeqCst) != generation {
                break;
            }
            match scan {
                Ok(observations) => {
                    let transitions = TRACKER
                        .get_or_init(Default::default)
                        .lock()
                        .unwrap()
                        .update(origin.elapsed().as_millis() as u64, &observations);
                    for transition in transitions {
                        match transition {
                            Transition::Started {
                                observation,
                                session_id,
                            } => {
                                log::info!(
                                    "Meeting call confirmed: {} session={} reason={}",
                                    observation.key,
                                    session_id,
                                    observation.probe.reason
                                );
                                let _ = app.emit(
                                    "meeting-detected",
                                    MeetingDetectedPayload {
                                        app: observation.app,
                                        process: observation.process,
                                        notify: current.notify,
                                        active_media: observation.active_media,
                                        session_id,
                                    },
                                );
                            }
                            Transition::Ended {
                                observation,
                                session_id,
                            } => {
                                log::info!(
                                    "Meeting call ended: {} session={}",
                                    observation.key,
                                    session_id
                                );
                                let _ = app.emit(
                                    "meeting-ended",
                                    MeetingEndedPayload {
                                        app: observation.app,
                                        process: observation.process,
                                        session_id,
                                    },
                                );
                            }
                        }
                    }
                }
                Err(error) => {
                    TRACKER
                        .get_or_init(Default::default)
                        .lock()
                        .unwrap()
                        .suspend();
                    log::warn!("Meeting detection inspection failed: {error}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(
                current.interval_secs.clamp(3, 3600),
            ))
            .await;
        }
        if MONITOR_GENERATION.load(Ordering::SeqCst) == generation {
            MONITOR_RUNNING.store(false, Ordering::SeqCst);
        }
    });
}

/// Called once at app startup to load persisted settings and start the monitor
/// if the user had it enabled.
pub fn initialize<R: Runtime>(app: &AppHandle<R>) {
    let settings = load_settings_from_disk();
    {
        let mut guard = SETTINGS.lock().unwrap();
        *guard = Some(settings.clone());
    }
    if settings.enabled {
        start_monitor(app);
    }
}

// ============================================================================
// Tauri commands
// ============================================================================

#[tauri::command]
pub async fn get_meeting_detection_settings() -> Result<MeetingDetectionSettings, String> {
    let guard = SETTINGS.lock().unwrap();
    Ok(guard.clone().unwrap_or_else(load_settings_from_disk))
}

#[tauri::command]
pub async fn set_meeting_detection_settings<R: Runtime>(
    app: AppHandle<R>,
    settings: MeetingDetectionSettings,
) -> Result<(), String> {
    // Sanity-clamp the interval.
    let mut settings = settings;
    sanitize_settings(&mut settings);

    save_settings_to_disk(&settings)?;
    {
        let mut guard = SETTINGS.lock().unwrap();
        *guard = Some(settings.clone());
    }

    // Apply settings without restarting an already running monitor.
    if settings.enabled {
        start_monitor(&app);
    } else {
        MONITOR_GENERATION.fetch_add(1, Ordering::SeqCst);
        MONITOR_RUNNING.store(false, Ordering::SeqCst);
    }
    Ok(())
}

/// Ensure a monitor is running using current settings.
#[tauri::command]
pub async fn start_meeting_detection<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    start_monitor(&app);
    Ok(())
}

/// Manually stop the monitor.
#[tauri::command]
pub async fn stop_meeting_detection() -> Result<(), String> {
    MONITOR_GENERATION.fetch_add(1, Ordering::SeqCst);
    MONITOR_RUNNING.store(false, Ordering::SeqCst);
    Ok(())
}
