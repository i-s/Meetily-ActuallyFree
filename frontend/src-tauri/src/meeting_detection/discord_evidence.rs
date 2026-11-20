//! Discord's microphone can be active for its settings mic test. Only call UI
//! evidence may start a call; muted/deafened calls must not require capture.
use super::super::evidence::{Presence, Probe};

#[derive(Clone, Copy)]
pub(super) enum ControlKind {
    Button,
    Toggle,
    Text,
    Other,
}

// Exact accessible names only: arbitrary message text is not a call action.
// These English/Russian labels need requalification when Discord changes UI.
pub(super) const LABELS: &[&str] = &[
    "Voice Connected",
    "Голосовая связь подключена",
    "Голос подключён",
    "Disconnect",
    "Отключиться",
    "Отключить",
    "Leave Call",
    "End Call",
    "Покинуть звонок",
    "Завершить звонок",
    "Mute",
    "Unmute",
    "Deafen",
    "Undeafen",
    "User Settings",
    "Выключить микрофон",
    "Включить микрофон",
    "Отключить звук",
    "Включить звук",
    "Настройки пользователя",
    "Voice & Video",
    "Голос и видео",
    "Let's Check",
    "Проверить",
    "Stop Testing",
    "Остановить проверку",
];

#[derive(Default)]
pub(super) struct UiEvidence {
    connected: bool,
    disconnect: bool,
    leave_call: bool,
    mute: bool,
    deafen: bool,
    user_settings: bool,
    settings_overlay: bool,
}

impl UiEvidence {
    pub(super) fn observe(&mut self, name: &str, kind: ControlKind) {
        let name = name.trim().to_lowercase();
        if matches!(kind, ControlKind::Button | ControlKind::Text) {
            self.connected |= matches!(
                name.as_str(),
                "voice connected" | "голосовая связь подключена" | "голос подключён"
            );
        }
        if matches!(kind, ControlKind::Other) {
            return;
        }
        // A settings overlay can hide a real call. It is unknown, not an end.
        self.settings_overlay |= matches!(
            name.as_str(),
            "voice & video"
                | "голос и видео"
                | "let's check"
                | "проверить"
                | "stop testing"
                | "остановить проверку"
        );
        if matches!(kind, ControlKind::Button) {
            self.disconnect |= matches!(name.as_str(), "disconnect" | "отключиться" | "отключить");
            self.leave_call |= matches!(
                name.as_str(),
                "leave call" | "end call" | "покинуть звонок" | "завершить звонок"
            );
            self.user_settings |=
                matches!(name.as_str(), "user settings" | "настройки пользователя");
        }
        if matches!(kind, ControlKind::Button | ControlKind::Toggle) {
            self.mute |= matches!(
                name.as_str(),
                "mute" | "unmute" | "выключить микрофон" | "включить микрофон"
            );
            self.deafen |= matches!(
                name.as_str(),
                "deafen" | "undeafen" | "отключить звук" | "включить звук"
            );
        }
    }

    fn active(&self) -> bool {
        self.leave_call || (self.connected && self.disconnect)
    }

    fn toolbar_without_call_controls(&self) -> bool {
        self.user_settings
            && self.mute
            && self.deafen
            && !self.settings_overlay
            && !self.connected
            && !self.disconnect
            && !self.leave_call
    }
}

pub(super) fn classify(running: bool, windows: &[UiEvidence], capture: Presence) -> Probe {
    if !running {
        return Probe {
            presence: Presence::Inactive,
            reason: "discord_process_absent",
        };
    }
    if windows.iter().any(UiEvidence::active) {
        return Probe {
            presence: Presence::Active,
            reason: if capture == Presence::Active {
                "discord_call_ui_and_capture"
            } else {
                "discord_call_ui"
            },
        };
    }
    // A successful UIA query does not certify Chromium exposed its complete
    // tree. The persistent user toolbar also exists during calls: missing
    // voice controls cannot prove a call ended. Only process exit is a
    // qualified negative signal until physical testing finds an explicit end.
    if !windows.is_empty()
        && windows
            .iter()
            .all(UiEvidence::toolbar_without_call_controls)
    {
        return Probe {
            presence: Presence::Unknown,
            reason: "discord_call_end_unconfirmed_while_app_open",
        };
    }
    Probe {
        presence: Presence::Unknown,
        reason: if capture == Presence::Active {
            "discord_capture_without_call_ui"
        } else {
            "discord_ui_unavailable_or_ambiguous"
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui(buttons: &[&str], text: &[&str]) -> UiEvidence {
        let mut ui = UiEvidence::default();
        for name in buttons {
            ui.observe(name, ControlKind::Button);
        }
        for name in text {
            ui.observe(name, ControlKind::Text);
        }
        ui
    }

    fn presence(windows: &[UiEvidence], capture: Presence) -> Presence {
        classify(true, windows, capture).presence
    }

    #[test]
    fn toolbar_without_call_controls_cannot_start_or_end_a_call() {
        for capture in [Presence::Active, Presence::Inactive, Presence::Unknown] {
            assert_eq!(
                presence(&[ui(&["User Settings", "Mute", "Deafen"], &[])], capture),
                Presence::Unknown
            );
        }
    }

    #[test]
    fn connected_call_requires_disconnect_control() {
        assert_eq!(
            presence(
                &[ui(&["Disconnect"], &["Voice Connected"])],
                Presence::Active
            ),
            Presence::Active
        );
        assert_eq!(
            presence(&[ui(&[], &["Voice Connected"])], Presence::Active),
            Presence::Unknown
        );
    }

    #[test]
    fn muted_deafened_call_is_active_without_capture() {
        assert_eq!(
            presence(
                &[ui(
                    &["Disconnect", "Unmute", "Undeafen"],
                    &["Voice Connected"]
                )],
                Presence::Inactive
            ),
            Presence::Active
        );
    }

    #[test]
    fn russian_voice_panel_is_supported() {
        assert_eq!(
            presence(
                &[ui(&["Отключиться"], &["Голосовая связь подключена"])],
                Presence::Unknown
            ),
            Presence::Active
        );
    }

    #[test]
    fn call_specific_leave_button_does_not_require_microphone() {
        assert_eq!(
            presence(&[ui(&["Leave Call"], &[])], Presence::Inactive),
            Presence::Active
        );
    }

    #[test]
    fn settings_mic_check_and_generic_disconnect_do_not_start() {
        assert_eq!(
            presence(
                &[ui(
                    &[
                        "Disconnect",
                        "User Settings",
                        "Mute",
                        "Deafen",
                        "Voice & Video",
                        "Let's Check"
                    ],
                    &[]
                )],
                Presence::Active
            ),
            Presence::Unknown
        );
    }

    #[test]
    fn labels_in_chat_text_do_not_become_call_buttons() {
        assert_eq!(
            presence(
                &[ui(&[], &["Leave Call", "Disconnect", "Voice Connected"])],
                Presence::Active
            ),
            Presence::Unknown
        );
    }

    #[test]
    fn missing_background_accessibility_does_not_fabricate_call_or_end() {
        assert_eq!(presence(&[], Presence::Active), Presence::Unknown);
        assert_eq!(
            presence(&[ui(&[], &[])], Presence::Inactive),
            Presence::Unknown
        );
    }

    #[test]
    fn disconnect_in_unrelated_window_does_not_pair_with_voice_status() {
        assert_eq!(
            presence(
                &[ui(&["Disconnect"], &[]), ui(&[], &["Voice Connected"])],
                Presence::Active
            ),
            Presence::Unknown
        );
    }

    #[test]
    fn exit_is_inactive_even_if_previous_media_state_was_active() {
        assert_eq!(
            classify(false, &[], Presence::Active).presence,
            Presence::Inactive
        );
    }
}
