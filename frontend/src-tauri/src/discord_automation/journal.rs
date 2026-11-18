//! Bounded, volatile observation log. Only typed signal snapshots and finite manual
//! truth markers enter it; no free-text/AX tree, filesystem or recording operations.
pub const MAX_ENTRIES: usize = 4096;
pub const SCENARIOS: &[&str] = &["guild_voice", "dm", "group_dm"];
pub const TRUTHS: &[&str] = &[
    "idle",
    "settings",
    "mic_test",
    "ringing",
    "joined",
    "joined_muted",
    "muted",
    "unmuted",
    "silence",
    "channel_changed",
    "device_changed",
    "reconnecting",
    "reconnected",
    "left_app_open",
    "client_quit",
    "permission_lost",
    "permission_restored",
];

pub struct Entry<T> {
    pub at_ms: u64,
    pub scan_from_ms: Option<u64>,
    pub truth: Option<&'static str>,
    pub sample: Option<T>,
}
pub struct Journal<T> {
    pub scenario: &'static str,
    pub started_ms: u64,
    pub active: bool,
    pub truncated: bool,
    pub entries: Vec<Entry<T>>,
}
impl<T> Journal<T> {
    pub fn new(scenario: &str, now_ms: u64) -> Result<Self, String> {
        let scenario = SCENARIOS
            .iter()
            .copied()
            .find(|s| *s == scenario)
            .ok_or("Choose guild_voice, dm or group_dm")?;
        Ok(Self {
            scenario,
            started_ms: now_ms,
            active: true,
            truncated: false,
            entries: Vec::new(),
        })
    }
    fn append(&mut self, entry: Entry<T>) -> Result<(), String> {
        if !self.active {
            return Err("Observation journal is stopped".into());
        }
        if self.entries.len() >= MAX_ENTRIES {
            self.active = false;
            self.truncated = true;
            return Err(
                "Observation journal full; export it before starting a new scenario".into(),
            );
        }
        self.entries.push(entry);
        Ok(())
    }
    pub fn mark(&mut self, truth: &str, now_ms: u64) -> Result<(), String> {
        let truth = TRUTHS
            .iter()
            .copied()
            .find(|s| *s == truth)
            .ok_or("Unknown manual truth marker")?;
        self.append(Entry {
            at_ms: now_ms.saturating_sub(self.started_ms),
            scan_from_ms: None,
            truth: Some(truth),
            sample: None,
        })
    }
    pub fn sample(&mut self, sample: T, from_ms: u64, now_ms: u64) {
        // A scan begun before the user started this journal is not its evidence.
        if from_ms < self.started_ms || !self.active {
            return;
        }
        let _ = self.append(Entry {
            at_ms: now_ms.saturating_sub(self.started_ms),
            scan_from_ms: Some(from_ms - self.started_ms),
            truth: None,
            sample: Some(sample),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_scenarios_and_truth_never_retain_free_text() {
        assert!(Journal::<bool>::new("private channel name", 0).is_err());
        let mut j = Journal::new("dm", 100).unwrap();
        assert!(j.mark("private message text", 101).is_err());
        j.mark("joined_muted", 102).unwrap();
        j.sample(true, 103, 105);
        assert_eq!(j.entries[0].truth, Some("joined_muted"));
        assert_eq!(j.entries[1].scan_from_ms, Some(3));
        assert_eq!(j.entries[1].at_ms, 5);
    }
    #[test]
    fn overflow_stops_without_evicting_truth_and_stop_preserves_export() {
        let mut j = Journal::new("guild_voice", 0).unwrap();
        for i in 0..MAX_ENTRIES {
            j.sample(false, i as u64, i as u64);
        }
        assert_eq!(j.entries.len(), MAX_ENTRIES);
        assert!(j.mark("idle", 5000).is_err());
        assert!(j.truncated);
        assert!(!j.active);
        j.sample(true, 6000, 6000);
        assert_eq!(j.entries.len(), MAX_ENTRIES);
    }
    #[test]
    fn old_scans_and_stopped_journals_do_not_append() {
        let mut j = Journal::new("group_dm", 100).unwrap();
        j.sample(true, 99, 110);
        assert!(j.entries.is_empty());
        j.active = false;
        j.sample(true, 101, 110);
        assert!(j.mark("idle", 120).is_err());
        assert!(j.entries.is_empty());
    }
}
