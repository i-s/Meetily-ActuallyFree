//! Platform probes never turn missing permission or failed inspection into a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Active,
    Inactive,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub presence: Presence,
    /// Fixed, nonprivate diagnostic text. Never window titles or message content.
    pub reason: &'static str,
}

#[derive(Clone, Debug)]
pub struct Observation {
    pub key: String,
    pub app: String,
    pub process: String,
    pub probe: Probe,
    pub active_media: bool,
}

#[derive(Clone, Debug)]
pub enum Transition {
    Started {
        observation: Observation,
        session_id: u64,
    },
    Ended {
        observation: Observation,
        session_id: u64,
    },
}

#[derive(Default)]
struct Session {
    confirmations: u8,
    active: Option<(u64, Observation)>,
    inactive_since: Option<u64>,
}
#[derive(Default)]
pub struct Tracker {
    sessions: std::collections::BTreeMap<String, Session>,
    sequence: u64,
}
impl Tracker {
    pub fn suspend(&mut self) {
        for session in self.sessions.values_mut() {
            session.confirmations = 0;
            session.inactive_since = None;
        }
    }
    pub fn update(&mut self, now_ms: u64, observations: &[Observation]) -> Vec<Transition> {
        let mut transitions = Vec::new();
        for observation in observations {
            let session = self.sessions.entry(observation.key.clone()).or_default();
            match observation.probe.presence {
                Presence::Active => {
                    session.inactive_since = None;
                    session.confirmations = session.confirmations.saturating_add(1);
                    if session.active.is_none() && session.confirmations >= 2 {
                        self.sequence += 1;
                        session.active = Some((self.sequence, observation.clone()));
                        transitions.push(Transition::Started {
                            observation: observation.clone(),
                            session_id: self.sequence,
                        });
                    }
                }
                Presence::Inactive => {
                    session.confirmations = 0;
                    let since = *session.inactive_since.get_or_insert(now_ms);
                    if now_ms.saturating_sub(since) >= 45_000 {
                        if let Some((session_id, observation)) = session.active.take() {
                            transitions.push(Transition::Ended {
                                observation,
                                session_id,
                            });
                        }
                    }
                }
                Presence::Unknown => {
                    // Permission loss, a hidden window or a timed-out UI probe is
                    // not evidence that the user left. Never auto-stop on it.
                    session.confirmations = 0;
                    session.inactive_since = None;
                }
            }
        }
        transitions
    }

    pub fn session_matches(&self, key: &str, id: u64) -> bool {
        self.sessions
            .get(key)
            .and_then(|s| s.active.as_ref())
            .is_some_and(|(current, _)| *current == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation(presence: Presence, process: &str) -> Observation {
        Observation {
            key: "zoom".into(),
            app: "Zoom".into(),
            process: process.into(),
            probe: Probe {
                presence,
                reason: "fixture",
            },
            active_media: true,
        }
    }
    #[test]
    fn idle_and_missing_permission_never_start() {
        let mut t = Tracker::default();
        for i in 0..10 {
            assert!(t
                .update(i * 15000, &[observation(Presence::Inactive, "zoom")])
                .is_empty());
            assert!(t
                .update(i * 15000, &[observation(Presence::Unknown, "zoom")])
                .is_empty());
        }
    }
    #[test]
    fn confirmed_call_prompts_once_despite_helper_or_settings_changes() {
        let mut t = Tracker::default();
        assert!(t
            .update(0, &[observation(Presence::Active, "zoom.us")])
            .is_empty());
        assert!(matches!(
            t.update(15000, &[observation(Presence::Active, "zoom.us")])
                .as_slice(),
            [Transition::Started { session_id: 1, .. }]
        ));
        for i in 2..10 {
            assert!(t
                .update(i * 15000, &[observation(Presence::Active, "Zoom Helper")])
                .is_empty());
        }
    }
    #[test]
    fn unknown_or_transient_loss_does_not_end_and_new_call_gets_new_session() {
        let mut t = Tracker::default();
        let active = observation(Presence::Active, "zoom");
        t.update(0, &[active.clone()]);
        t.update(15000, &[active.clone()]);
        assert!(t
            .update(30000, &[observation(Presence::Inactive, "zoom")])
            .is_empty());
        assert!(t
            .update(90000, &[observation(Presence::Unknown, "zoom")])
            .is_empty());
        assert!(t.update(120000, &[active.clone()]).is_empty());
        assert!(t
            .update(135000, &[observation(Presence::Inactive, "zoom")])
            .is_empty());
        assert!(matches!(
            t.update(180000, &[observation(Presence::Inactive, "zoom")])
                .as_slice(),
            [Transition::Ended { session_id: 1, .. }]
        ));
        t.update(195000, &[active.clone()]);
        assert!(matches!(
            t.update(210000, &[active]).as_slice(),
            [Transition::Started { session_id: 2, .. }]
        ));
    }
    #[test]
    fn independent_apps_do_not_replace_each_other() {
        let mut t = Tracker::default();
        let zoom = observation(Presence::Active, "zoom");
        let mut discord = zoom.clone();
        discord.key = "discord".into();
        discord.app = "Discord".into();
        t.update(0, &[zoom.clone(), discord.clone()]);
        assert_eq!(t.update(15000, &[discord.clone(), zoom.clone()]).len(), 2);
        assert!(t.update(30000, &[zoom, discord]).is_empty());
    }
    #[test]
    fn suspension_keeps_session_but_discards_unobserved_debounce_time() {
        let mut t = Tracker::default();
        let active = observation(Presence::Active, "zoom");
        let idle = observation(Presence::Inactive, "zoom");
        t.update(0, &[active.clone()]);
        t.suspend();
        assert!(t.update(60000, &[active.clone()]).is_empty());
        t.update(75000, &[active]);
        t.update(90000, &[idle.clone()]);
        t.suspend();
        assert!(t.session_matches("zoom", 1));
        assert!(t.update(300000, &[idle.clone()]).is_empty());
        assert_eq!(t.update(345000, &[idle]).len(), 1);
    }
}
