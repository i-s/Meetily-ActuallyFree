//! Session mode is frozen by native startup, not by a WebView preference edit.
use std::sync::atomic::{AtomicBool, Ordering};

static MIC_IDENTITY_UNVERIFIED: AtomicBool = AtomicBool::new(false);
static IN_PERSON: AtomicBool = AtomicBool::new(false);

pub fn start_session(in_person: bool, microphone_identity_unverified: bool) {
    IN_PERSON.store(in_person, Ordering::SeqCst);
    MIC_IDENTITY_UNVERIFIED.store(microphone_identity_unverified, Ordering::SeqCst);
}

pub fn in_person() -> bool {
    IN_PERSON.load(Ordering::SeqCst)
}

/// Frozen native provenance, not a platform-wide identity switch.
pub fn microphone_identity_unverified() -> bool { MIC_IDENTITY_UNVERIFIED.load(Ordering::SeqCst) }

/// Zoom's speaker/headset route is not qualified. A selected Zoom source or
/// Zoom present during global capture stays neutral even for manual recording.
/// An unavailable inventory with global capture is ambiguous, not proof of You.
pub fn zoom_identity_unverified(macos: bool, in_person: bool, per_app: bool,
    executables: &[&str], zoom_present: Option<bool>, automatic_zoom: bool) -> bool {
    if in_person { return true; }
    macos && (automatic_zoom || if per_app { executables.contains(&"zoom.us") }
        else { zoom_present != Some(false) })
}

pub fn microphone_label(in_person: bool) -> &'static str {
    microphone_label_for_policy(in_person, microphone_identity_unverified())
}

pub fn microphone_label_for_policy(in_person: bool, unverified: bool) -> &'static str {
    if in_person || unverified { "Microphone" } else { "You" }
}

/// Validate before opening any capture stream. A missing app choice must not
/// accidentally mean global capture; microphone-only is an explicit mode.
pub fn validate_sources(in_person: bool, per_app: bool, targets: usize, system_available: bool) -> Result<(), &'static str> {
    if !in_person && per_app {
        validate_app_count(cfg!(target_os = "macos"), targets)?;
    }
    if !in_person && !per_app && !system_available {
        return Err("System audio is unavailable. Connect an output device or enable only the microphone before recording.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unqualified_speaker_leakage_is_never_user_identity() {
        assert_eq!(microphone_label_for_policy(false, true), "Microphone");
        assert_eq!(microphone_label_for_policy(true, false), "Microphone");
        assert_eq!(microphone_label_for_policy(false, false), "You");
    }

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


pub fn validate_app_count(macos: bool, count: usize) -> Result<(), &'static str> {
    if count == 0 { return Err("No target applications selected. Choose an app before recording."); }
    if macos && count > 1 { return Err("macOS supports one app at a time. Remove extra selected apps before recording."); }
    Ok(())
}

/// Resolve the complete user selection, never silently drop unavailable apps.
/// The caller opens no streams until this succeeds.
pub fn resolve_app_targets<F>(macos: bool, targets: &[(String, String)], find_pid: F) -> Result<Vec<(String, u32)>, String>
where F: Fn(&str) -> Option<u32> {
    validate_app_count(macos, targets.len()).map_err(str::to_string)?;
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for (name, executable) in targets {
        if let Some(pid) = find_pid(executable) { resolved.push((name.clone(), pid)); }
        else { missing.push(name.clone()); }
    }
    if !missing.is_empty() {
        return Err(format!("Selected target applications are unavailable: {}. Open every chosen app or change the selection before recording.", missing.join(", ")));
    }
    Ok(resolved)
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    fn targets() -> Vec<(String, String)> { vec![("A".into(), "a".into()), ("B".into(), "b".into())] }
    #[test]
    fn macos_rejects_the_list_before_even_resolving_a_first_pid() {
        assert!(resolve_app_targets(true, &targets(), |_| panic!("must fail before any process lookup/capture")).is_err());
        assert_eq!(resolve_app_targets(true, &targets()[..1], |_| Some(42)).unwrap(), vec![("A".into(), 42)]);
    }
    #[test]
    fn missing_subset_is_not_silently_dropped_on_multi_app_platforms() {
        assert!(resolve_app_targets(false, &targets(), |exe| if exe == "a" { Some(42) } else { None }).unwrap_err().contains("B"));
        assert!(resolve_app_targets(true, &targets()[..1], |_| None).is_err());
        assert!(resolve_app_targets(false, &[], |_| None).is_err());
        assert_eq!(resolve_app_targets(false, &targets(), |_| Some(42)).unwrap().len(), 2);
    }
}

/// Automation must not reinterpret a saved room/other-app selection as a Zoom
/// call. Reject visibly; never broaden capture or rewrite user preferences.
pub fn validate_zoom_sources(in_person: bool, per_app: bool, executables: &[&str]) -> Result<(), &'static str> {
    if in_person {
        return Err("Zoom automation requires explicitly enabled computer audio. Enable it before recording.");
    }
    if per_app && (executables.len() != 1 || executables[0] != "zoom.us") {
        return Err("Zoom automation requires Zoom (zoom.us) as the one selected target application, or explicit all-system capture. Recording source preferences were not changed.");
    }
    Ok(())
}

#[cfg(test)]
mod zoom_source_tests {
    use super::*;
    #[test]
    fn automatic_zoom_never_records_another_selected_app_or_room_mode() {
        assert!(validate_zoom_sources(true, false, &[]).is_err());
        assert!(validate_zoom_sources(false, true, &["Discord"]).is_err());
        assert!(validate_zoom_sources(false, true, &[]).is_err());
        assert!(validate_zoom_sources(false, true, &["zoom.us", "Discord"]).is_err());
        assert!(validate_zoom_sources(false, true, &["zoom.us"]).is_ok());
        assert!(validate_zoom_sources(false, false, &[]).is_ok());
    }
}

#[cfg(test)]
mod scoped_zoom_tests {
    use super::*;
    #[test]
    fn manual_zoom_and_ambiguous_global_routes_are_neutral_not_all_macos() {
        assert!(zoom_identity_unverified(true, false, true, &["zoom.us"], Some(true), false));
        assert!(zoom_identity_unverified(true, false, false, &[], Some(true), false));
        assert!(zoom_identity_unverified(true, false, false, &[], None, false));
        assert!(!zoom_identity_unverified(true, false, true, &["Discord"], Some(true), false));
        assert!(!zoom_identity_unverified(true, false, false, &[], Some(false), false));
        assert!(!zoom_identity_unverified(false, false, false, &[], Some(true), false));
        assert!(zoom_identity_unverified(true, true, false, &[], Some(false), false));
    }
}
