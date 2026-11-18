//! Conservative Discord AX evidence. No audio/process heuristic may produce Active.
//! This module is device-free so the production classifier/tracker run in Linux tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Active,
    Inactive,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub presence: Presence,
    pub reason: &'static str,
    pub client_present: Option<bool>,
    pub facts: Facts,
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub complete: bool,
    pub connected: bool,
    pub disconnect: bool,
    pub reconnect: bool,
    pub join: bool,
    pub connected_pair: bool,
}

#[derive(Default)]
pub struct Control {
    pub labels: Vec<String>,
    pub enabled: bool,
}
#[derive(Default)]
pub struct Snapshot {
    pub windows: Vec<Vec<Control>>,
    pub complete: bool,
    pub issues: Vec<ScanIssue>,
}
#[derive(Debug)]
pub enum ScanIssue {
    Deadline,
    NodeLimit,
    ChildLimit,
    DepthLimit,
    Ax(&'static str, i32),
    Missing(&'static str),
    Invalid(&'static str),
}

fn normalized(label: &str) -> String {
    label
        .trim()
        .trim_end_matches(['…', '.'])
        .trim()
        .to_lowercase()
}

/// Filter before snapshot retention: unrelated control labels never enter it.
pub fn observation_cue(label: &str) -> bool {
    matches!(
        normalized(label).as_str(),
        "voice connected"
            | "голосовая связь подключена"
            | "disconnect"
            | "отключиться"
            | "rtc connecting"
            | "connecting"
            | "reconnecting"
            | "подключение"
            | "переподключение"
            | "join voice"
            | "start voice call"
            | "присоединиться к голосовому каналу"
    )
}

pub fn classify(snapshot: &Snapshot) -> Probe {
    let mut facts = Facts {
        complete: snapshot.complete && snapshot.issues.is_empty(),
        ..Facts::default()
    };
    // Only finite exact control cues survive this function. Labels are dropped.
    for window in &snapshot.windows {
        let labels: Vec<_> = window
            .iter()
            .filter(|c| c.enabled)
            .flat_map(|c| c.labels.iter().map(|s| normalized(s)))
            .collect();
        let connected = labels
            .iter()
            .any(|s| matches!(s.as_str(), "voice connected" | "голосовая связь подключена"));
        let disconnect = labels
            .iter()
            .any(|s| matches!(s.as_str(), "disconnect" | "отключиться"));
        let reconnect = labels.iter().any(|s| {
            matches!(
                s.as_str(),
                "rtc connecting"
                    | "connecting"
                    | "reconnecting"
                    | "подключение"
                    | "переподключение"
            )
        });
        facts.connected |= connected;
        facts.disconnect |= disconnect;
        facts.reconnect |= reconnect;
        facts.join |= labels.iter().any(|s| {
            matches!(
                s.as_str(),
                "join voice" | "start voice call" | "присоединиться к голосовому каналу"
            )
        });
        facts.connected_pair |= connected && disconnect && !reconnect;
    }
    let reason = if let Some(issue) = snapshot.issues.first() {
        match issue {
            ScanIssue::Deadline => "ax_scan_timeout",
            ScanIssue::NodeLimit => "ax_node_limit",
            ScanIssue::ChildLimit => "ax_child_limit",
            ScanIssue::DepthLimit => "ax_depth_limit",
            ScanIssue::Ax(_, _) => "ax_read_failed",
            ScanIssue::Missing(_) => "ax_attribute_missing",
            ScanIssue::Invalid(_) => "ax_attribute_invalid",
        }
    } else if !snapshot.complete {
        "ax_incomplete"
    } else if facts.reconnect {
        "reconnect_controls_candidate"
    } else if facts.connected_pair {
        "connected_controls_candidate"
    } else if facts.disconnect {
        "disconnect_without_connected_unknown"
    } else if facts.join {
        "join_controls_only_unknown"
    } else {
        "controls_not_confirmed"
    };
    Probe {
        presence: if facts.complete && facts.connected_pair && !facts.reconnect {
            Presence::Active
        } else {
            Presence::Unknown
        },
        reason,
        client_present: Some(true),
        facts,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    None,
    Start(u64),
    End(u64),
}
#[derive(Default)]
pub struct Tracker {
    pub session: Option<u64>,
    next: u64,
    candidate_since: Option<u64>,
    inactive_since: Option<u64>,
}
impl Tracker {
    /// Monotonic milliseconds, supplied by the worker (never wall clock).
    pub fn observe(&mut self, presence: Presence, now_ms: u64) -> Transition {
        match presence {
            Presence::Active => {
                self.inactive_since = None;
                if self.session.is_none() {
                    let since = *self.candidate_since.get_or_insert(now_ms);
                    if now_ms.saturating_sub(since) >= 3_000 {
                        self.next += 1;
                        self.session = Some(self.next);
                        self.candidate_since = None;
                        return Transition::Start(self.next);
                    }
                }
            }
            Presence::Inactive => {
                self.candidate_since = None;
                if let Some(session) = self.session {
                    let since = *self.inactive_since.get_or_insert(now_ms);
                    if now_ms.saturating_sub(since) >= 45_000 {
                        self.session = None;
                        self.inactive_since = None;
                        return Transition::End(session);
                    }
                }
            }
            Presence::Unknown => {
                self.candidate_since = None;
                // Unknown cannot accumulate a leave grace period.
                self.inactive_since = None;
            }
        }
        Transition::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_retains_only_finite_cues_and_specific_unknown_reasons() {
        assert!(!observation_cue("Private unrelated channel name"));
        assert!(observation_cue("Voice Connected"));
        let mut data = snapshot(&[&[
            ("Private unrelated channel name", true),
            ("Disconnect", true),
        ]]);
        let p = classify(&data);
        assert_eq!(p.reason, "disconnect_without_connected_unknown");
        assert!(p.facts.disconnect);
        assert!(!p.facts.connected_pair);
        for (issue, reason) in [
            (ScanIssue::Deadline, "ax_scan_timeout"),
            (ScanIssue::NodeLimit, "ax_node_limit"),
            (ScanIssue::Ax("AXRole", -25204), "ax_read_failed"),
        ] {
            data.complete = false;
            data.issues = vec![issue];
            assert_eq!(classify(&data).reason, reason);
            assert_eq!(classify(&data).presence, Presence::Unknown);
        }
    }
    fn snapshot(windows: &[&[(&str, bool)]]) -> Snapshot {
        Snapshot {
            complete: true,
            issues: vec![],
            windows: windows
                .iter()
                .map(|w| {
                    w.iter()
                        .map(|(s, e)| Control {
                            labels: vec![s.to_string()],
                            enabled: *e,
                        })
                        .collect()
                })
                .collect(),
        }
    }
    #[test]
    fn ringing_idle_settings_and_mic_are_not_join() {
        for labels in [
            vec![("Disconnect", true)],
            vec![("Start Voice Call", true)],
            vec![("Test Mic", true), ("Unmute", true)],
            vec![("Join Voice", true)],
            vec![],
        ] {
            assert_eq!(classify(&snapshot(&[&labels])).presence, Presence::Unknown);
        }
    }
    #[test]
    fn mute_silence_device_and_channel_names_do_not_change_presence() {
        for label in ["Mute", "Unmute", "Deafen", "USB", "Different channel", ""] {
            assert_eq!(
                classify(&snapshot(&[&[
                    ("Voice Connected", true),
                    ("Disconnect", true),
                    (label, true)
                ]]))
                .presence,
                Presence::Active
            );
        }
    }
    #[test]
    fn exact_enabled_same_window_complete_evidence_only() {
        for data in [
            snapshot(&[&[("Voice Connected", true)], &[("Disconnect", true)]]),
            snapshot(&[&[("Voice Connected", false), ("Disconnect", true)]]),
            snapshot(&[&[
                ("Private message: Voice Connected", true),
                ("Disconnect", true),
            ]]),
            snapshot(&[&[
                ("Voice Connected", true),
                ("Disconnect", true),
                ("RTC Connecting", true),
            ]]),
        ] {
            assert_eq!(classify(&data).presence, Presence::Unknown);
        }
        let mut data = snapshot(&[&[("Voice Connected", true), ("Disconnect", true)]]);
        data.complete = false;
        assert_eq!(classify(&data).presence, Presence::Unknown);
    }
    #[test]
    fn unknown_never_ends_and_reconnect_does_not_split() {
        let mut t = Tracker::default();
        assert_eq!(t.observe(Presence::Active, 0), Transition::None);
        assert_eq!(t.observe(Presence::Active, 3_000), Transition::Start(1));
        assert_eq!(t.observe(Presence::Inactive, 4_000), Transition::None);
        assert_eq!(t.observe(Presence::Unknown, 60_000), Transition::None);
        assert_eq!(t.observe(Presence::Active, 90_000), Transition::None);
        assert_eq!(t.session, Some(1));
        t.observe(Presence::Inactive, 91_000);
        assert_eq!(t.observe(Presence::Inactive, 136_000), Transition::End(1));
        t.observe(Presence::Active, 140_000);
        assert_eq!(t.observe(Presence::Active, 143_000), Transition::Start(2));
    }
    #[test]
    fn unstable_join_and_unknown_leave_reset_grace() {
        let mut t = Tracker::default();
        t.observe(Presence::Active, 0);
        t.observe(Presence::Unknown, 3_000);
        assert_eq!(t.observe(Presence::Active, 4_000), Transition::None);
        assert_eq!(t.observe(Presence::Active, 7_000), Transition::Start(1));
        t.observe(Presence::Inactive, 8_000);
        t.observe(Presence::Unknown, 40_000);
        assert_eq!(t.observe(Presence::Inactive, 80_000), Transition::None);
        assert_eq!(t.observe(Presence::Inactive, 124_999), Transition::None);
        assert_eq!(t.observe(Presence::Inactive, 125_000), Transition::End(1));
    }
}
