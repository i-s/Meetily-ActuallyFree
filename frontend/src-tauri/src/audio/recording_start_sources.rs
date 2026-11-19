//! Source-specific preflight. Neither an observed session nor a native ticket
//! grants qualification. Keep this dispatch independent of device/model setup.
use super::recording_ownership::LifecycleOwner;

pub fn validate(source: &str, session: &str) -> Result<LifecycleOwner, String> {
    if !super::recording_ownership::automation_enabled() {
        return Err("Meeting automation is disabled".into());
    }
    match source {
        "zoom" => {
            if !crate::meeting_detection::zoom_automation_enabled() {
                return Err("Zoom automatic recording is not opted in".into());
            }
            crate::meeting_detection::lifecycle::validate_start(session)?;
            crate::meeting_detection::lifecycle::session_owner(session)
                .filter(|owner| owner.source == "zoom")
                .ok_or_else(|| "Zoom session is no longer current".into())
        }
        "discord" => crate::discord_automation::qualified_owner(session),
        _ => Err("Unsupported automated recording source".into()),
    }
}

/// Capture selection is explicit too. A qualified client call must not silently
/// record a room microphone or a different selected application.
pub fn validate_sources(source: &str, in_person: bool, per_app: bool, executables: &[&str]) -> Result<(), &'static str> {
    match source {
        "zoom" => super::recording_mode::validate_zoom_sources(in_person, per_app, executables),
        "discord" => {
            if in_person { return Err("Automatic Discord recording requires explicitly enabled computer audio"); }
            if per_app && (executables.len() != 1 || !executables[0].eq_ignore_ascii_case("Discord")) {
                return Err("Automatic Discord recording requires Discord as the single selected app or explicit global capture");
            }
            Ok(())
        }
        _ => Err("Unsupported automated recording source"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clients_do_not_share_a_source_selection_or_room_shortcut() {
        assert!(validate_sources("discord", false, true, &["Discord"]).is_ok());
        assert!(validate_sources("discord", false, false, &[]).is_ok());
        assert!(validate_sources("discord", true, false, &[]).is_err());
        assert!(validate_sources("discord", false, true, &["zoom.us"]).is_err());
        assert!(validate_sources("zoom", false, true, &["Discord"]).is_err());
        assert!(validate_sources("other", false, false, &[]).is_err());
    }
}
