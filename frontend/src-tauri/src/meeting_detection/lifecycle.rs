//! Native lifecycle ownership survives route changes and monitor restarts.
//! Adapters supply independent source keys; no first-app-wins arbitration here.
use super::evidence::{Observation, Presence, Probe, Tracker, Transition};
use serde::Serialize;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

#[derive(Clone, Serialize)]
pub struct SourceState {
    source: String,
    app: String,
    process: String,
    presence: String,
    reason: String,
    session_id: Option<String>,
    qualified: bool,
}
#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    revision: u64,
    enabled: bool,
    automation_enabled: bool,
    sources: Vec<SourceState>,
}
struct State {
    tracker: Tracker,
    snapshot: Snapshot,
    epoch: String,
    clock: Instant,
    sessions: std::collections::BTreeMap<String, String>,
    monitor_generation: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            tracker: Tracker::default(), snapshot: Snapshot::default(),
            epoch: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos().to_string(),
            clock: Instant::now(), sessions: std::collections::BTreeMap::new(), monitor_generation: 0,
        }
    }
}
static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

pub fn suspend(enabled: bool) -> Snapshot {
    let mut state = STATE.lock().unwrap();
    state.tracker.suspend();
    state.monitor_generation = super::MONITOR_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
    state.snapshot.enabled = enabled;
    state.snapshot.automation_enabled = enabled && super::zoom_automation_enabled();
    state.snapshot.revision += 1;
    for source in &mut state.snapshot.sources {
        source.presence = "unknown".into();
        source.reason = if enabled { "monitor_restarting" } else { "detection_disabled" }.into();
        if source.source == "zoom" {
            super::zoom_observation::observe(&source.presence, &source.reason, source.session_id.as_deref());
        }
    }
    state.snapshot.clone()
}

pub fn observe_zoom(probe: Probe, generation: u64) -> Option<Snapshot> {
    observe_sources(generation, &[Observation {
        key: "zoom".into(), app: "Zoom".into(), process: "us.zoom.xos".into(),
        active_media: false, probe,
    }])
}

/// Supply one batch per poll, including Unknown for unavailable adapters. Other
/// qualified clients share the tracker without replacing another source.
pub fn observe_sources(generation: u64, observations: &[Observation]) -> Option<Snapshot> {
    let mut state = STATE.lock().unwrap();
    // Check under the state lock: an old blocking scan cannot revive a monitor
    // after Stop/settings restart. Revisions also reject reordered emissions.
    if super::MONITOR_GENERATION.load(std::sync::atomic::Ordering::SeqCst) != generation {
        return None;
    }
    state.monitor_generation = generation;
    let now = state.clock.elapsed().as_millis() as u64;
    Some(state.observe(now, observations))
}

impl State {
    fn observe(&mut self, now: u64, observations: &[Observation]) -> Snapshot {
        for transition in self.tracker.update(now, observations) {
            match transition {
                Transition::Started { session_id, observation } => {
                    let id = format!("{}:{}:{session_id}", self.epoch, observation.key);
                    self.sessions.insert(observation.key, id);
                }
                Transition::Ended { observation, .. } => { self.sessions.remove(&observation.key); },
            }
        }
        for source in &mut self.snapshot.sources {
            if !observations.iter().any(|o| o.key == source.source) {
                source.presence = "unknown".into();
                source.reason = "adapter_not_observed".into();
            }
        }
        for observation in observations {
            let probe = &observation.probe;
            let source = SourceState {
                source: observation.key.clone(), app: observation.app.clone(), process: observation.process.clone(),
                presence: match probe.presence { Presence::Active => "active", Presence::Inactive => "inactive", Presence::Unknown => "unknown" }.into(),
                reason: probe.reason.into(), session_id: self.sessions.get(&observation.key).cloned(),
                // No real AX/client/route class has passed physical qualification.
                // This is not persisted, and there is no production override.
                qualified: false,
            };
            self.snapshot.sources.retain(|s| s.source != observation.key);
            if source.source == "zoom" {
                super::zoom_observation::observe(&source.presence, &source.reason, source.session_id.as_deref());
            }
            self.snapshot.sources.push(source);
        }
        self.snapshot.enabled = true;
        self.snapshot.automation_enabled = super::zoom_automation_enabled();
        self.snapshot.revision += 1;
        self.snapshot.clone()
    }
}

#[tauri::command]
pub fn get_meeting_lifecycle_snapshot() -> Snapshot {
    let mut state = STATE.lock().unwrap();
    state.snapshot.automation_enabled = state.snapshot.enabled && super::zoom_automation_enabled();
    state.snapshot.clone()
}

pub fn validate_start(session: &str) -> Result<(), String> {
    let state = STATE.lock().unwrap();
    if state.monitor_generation == super::MONITOR_GENERATION.load(std::sync::atomic::Ordering::SeqCst)
        && crate::audio::recording_ownership::automation_enabled()
        && state.snapshot.enabled && state.snapshot.sources.iter()
        .any(|source| source.session_id.as_deref() == Some(session) && source.presence == "active" && source.qualified) {
        Ok(())
    } else {
        Err("Zoom automation is observation-only: this client has not passed physical lifecycle qualification, or the call/automation is no longer active. Use the manual recording controls; collect the Zoom qualification journal in Meeting Detection settings for the final combined Mac build.".into())
    }
}

pub fn session_owner(session: &str) -> Option<crate::audio::recording_ownership::LifecycleOwner> {
    STATE.lock().unwrap().snapshot.sources.iter()
        .find(|source| source.session_id.as_deref() == Some(session))
        .map(|source| crate::audio::recording_ownership::LifecycleOwner {
            source: source.source.clone(), session_id: session.into(),
            app: source.app.clone(), process: source.process.clone(),
        })
}

pub fn session_source(session: &str) -> Option<String> {
    STATE.lock().unwrap().snapshot.sources.iter()
        .find(|source| source.session_id.as_deref() == Some(session))
        .map(|source| source.source.clone())
}

#[cfg(target_os = "macos")]
pub fn scan_zoom(settings: &super::MeetingDetectionSettings) -> Probe {
    if !settings.meeting_apps.iter().any(|s| s.trim().eq_ignore_ascii_case("zoom"))
        || settings.ignored_apps.iter().any(|s| ["zoom", "zoom.us", "us.zoom.xos"].contains(&s.trim().to_lowercase().as_str())) {
        return Probe { presence: Presence::Unknown, reason: "zoom_adapter_disabled" };
    }
    super::zoom::probe_running(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(key: &str, presence: Presence) -> Observation {
        Observation { key: key.into(), app: key.into(), process: key.into(),
            probe: Probe { presence, reason: "synthetic" }, active_media: false }
    }
    #[test]
    fn native_snapshot_tracks_independent_sources_and_confirmed_exit_ids() {
        let mut state = State::default();
        let zoom = sample("zoom", Presence::Active);
        let discord = sample("discord", Presence::Active);
        assert!(state.observe(0, &[zoom.clone(), discord.clone()]).sources.iter().all(|s| s.session_id.is_none()));
        let joined = state.observe(3_000, &[zoom.clone(), discord.clone()]);
        let zoom_id = joined.sources.iter().find(|s| s.source == "zoom").unwrap().session_id.clone().unwrap();
        let discord_id = joined.sources.iter().find(|s| s.source == "discord").unwrap().session_id.clone().unwrap();
        assert_ne!(zoom_id, discord_id);
        for now in (6_000..=60_000).step_by(3_000) {
            let snapshot = state.observe(now, &[sample("zoom", Presence::Unknown), discord.clone()]);
            assert_eq!(snapshot.sources[0].session_id.as_deref(), Some(zoom_id.as_str()));
        }
        // Home/exit evidence is continuous for the full grace, not a single
        // failed probe, EOF or time spent asleep.
        for now in (63_000..=108_000).step_by(3_000) {
            state.observe(now, &[sample("zoom", Presence::Inactive), discord.clone()]);
        }
        assert!(!state.sessions.contains_key("zoom"));
        assert_eq!(state.sessions.get("discord"), Some(&discord_id));
        state.observe(111_000, &[zoom.clone(), discord.clone()]);
        state.observe(114_000, &[zoom, discord]);
        assert_ne!(state.sessions.get("zoom"), Some(&zoom_id));
        let json = serde_json::to_value(&state.snapshot).unwrap();
        assert_eq!(json["sources"][0]["source"], "zoom");
        assert!(json["revision"].as_u64().unwrap() > joined.revision);
    }
    #[test]
    fn omitted_adapter_and_sleep_cannot_confirm_exit_or_join() {
        let mut state = State::default();
        let active = sample("zoom", Presence::Active);
        state.observe(0, &[active.clone()]);
        state.observe(60_000, &[active.clone()]);
        assert!(state.sessions.is_empty());
        state.observe(63_000, &[active]);
        let id = state.sessions["zoom"].clone();
        state.observe(66_000, &[sample("zoom", Presence::Inactive)]);
        let absent = state.observe(120_000, &[]);
        assert_eq!(absent.sources[0].presence, "unknown");
        state.observe(123_000, &[sample("zoom", Presence::Inactive)]);
        assert_eq!(state.sessions["zoom"], id);
    }
    #[test]
    fn native_command_snapshot_and_stale_start_validation_share_one_owner() {
        // Only this test uses the process-global state; other tests are local.
        let id = {
            let mut state = STATE.lock().unwrap();
            *state = State::default();
            state.observe(0, &[sample("zoom", Presence::Active)]);
            state.observe(3_000, &[sample("zoom", Presence::Active)]);
            state.snapshot.sources[0].qualified = true; // synthetic fixture ONLY
            state.sessions["zoom"].clone()
        };
        crate::audio::recording_ownership::set_meeting_automation_enabled(true);
        assert!(validate_start(&id).is_ok());
        assert_eq!(get_meeting_lifecycle_snapshot().sources[0].session_id.as_deref(), Some(id.as_str()));
        suspend(false);
        assert!(validate_start(&id).is_err());
        suspend(true);
        assert!(validate_start("old-session").is_err());
        assert!(validate_start(&id).is_err()); // restart is Unknown until read again
        let generation = super::super::MONITOR_GENERATION.load(std::sync::atomic::Ordering::SeqCst);
        assert!(observe_sources(generation.wrapping_add(1), &[sample("zoom", Presence::Active)]).is_none());
    }
    #[test]
    fn r1_legacy_enabled_candidate_does_not_authorize_capture() {
        let id = {
            let mut state = STATE.lock().unwrap();
            *state = State::default();
            state.observe(0, &[sample("zoom", Presence::Active)]);
            state.observe(3_000, &[sample("zoom", Presence::Active)]);
            state.sessions["zoom"].clone()
        };
        assert!(validate_start(&id).is_err(), "unqualified physical AX candidate authorized recording");
    }

}
