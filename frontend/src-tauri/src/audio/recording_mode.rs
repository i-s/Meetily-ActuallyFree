//! Session mode is frozen by native startup, not by a WebView preference edit.
use std::sync::atomic::{AtomicBool, Ordering};

static IN_PERSON: AtomicBool = AtomicBool::new(false);

pub fn start_session(in_person: bool) {
    IN_PERSON.store(in_person, Ordering::SeqCst);
}

pub fn in_person() -> bool {
    IN_PERSON.load(Ordering::SeqCst)
}

pub fn microphone_label(in_person: bool) -> &'static str {
    if in_person { "Microphone" } else { "You" }
}

/// Validate before opening any capture stream. A missing app choice must not
/// accidentally mean global capture; microphone-only is an explicit mode.
pub fn validate_sources(in_person: bool, per_app: bool, targets: usize, system_available: bool) -> Result<(), &'static str> {
    if !in_person && per_app && targets == 0 {
        return Err("No target applications selected. Choose an app before recording.");
    }
    if !in_person && !per_app && !system_available {
        return Err("System audio is unavailable. Connect an output device or explicitly choose In-person meeting in Recording settings.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_selected_apps_never_becomes_global_audio() {
        assert!(validate_sources(false, true, 0, true).is_err());
        assert!(validate_sources(false, true, 1, false).is_ok());
    }

    #[test]
    fn microphone_only_requires_explicit_in_person_mode() {
        assert!(validate_sources(false, false, 0, false).is_err());
        assert!(validate_sources(true, true, 0, false).is_ok());
        assert_eq!(microphone_label(true), "Microphone");
    }
}
