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

pub fn classify(snapshot: &Snapshot) -> Probe {
    // Exact control labels only, never channel titles, participants or messages.
    // Do not broaden this to Disconnect alone: ringing can expose a hangup button.
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
        if connected && disconnect && !reconnect && snapshot.complete {
            return Probe {
                presence: Presence::Active,
                reason: "connected_controls_candidate",
            };
        }
    }
    // An absent toolbar is NOT leave. No idle AX shape has been qualified on a Mac.
    // In particular, Join Voice/Start Call elsewhere in the client are not proof.
    Probe {
        presence: Presence::Unknown,
        reason: if snapshot.issues.is_empty() {
            "controls_not_confirmed"
        } else {
            "ax_incomplete_or_limited"
        },
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
