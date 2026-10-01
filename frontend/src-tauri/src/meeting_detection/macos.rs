//! Read-only Zoom Accessibility evidence. No focus changes or permission prompts.
//!
//! Adapted from YoungTeurus/meetily (MIT), call-detection/src/{lib,native/macos}.rs
//! at 4732a3ff2331e3b33e007313c06b7748625e9304:
//! https://github.com/YoungTeurus/meetily/pull/1
//! The adapter reads control labels only, never message bodies or AXValue.
use super::evidence::{Presence, Probe};

#[derive(Default)]
struct Control {
    labels: Vec<String>,
    enabled: bool,
}

#[derive(Default)]
struct Snapshot {
    windows: Vec<Vec<Control>>,
    menu: Option<Vec<Control>>,
    complete: bool,
    issues: Vec<ScanIssue>,
}

// Only fixed API attribute names and numeric errors are retained, never AX text.
#[derive(Clone, Copy, Debug)]
enum ScanIssue {
    Deadline,
    NodeLimit,
    ChildLimit,
    DepthLimit,
    Ax(&'static str, i32),
    Missing(&'static str),
    Invalid(&'static str),
}

impl ScanIssue {
    fn reason(&self) -> &'static str {
        match self {
            Self::Deadline => "zoom_scan_timeout",
            Self::NodeLimit | Self::ChildLimit | Self::DepthLimit => "zoom_scan_limit",
            Self::Ax(_, _) => "zoom_ax_read_failed",
            Self::Missing(_) | Self::Invalid(_) => "zoom_ax_incomplete",
        }
    }
}

fn diagnostic_summary(snapshot: &Snapshot) -> String {
    let controls: Vec<_> = snapshot
        .windows
        .iter()
        .flatten()
        .chain(snapshot.menu.iter().flatten())
        .collect();
    let count = |matches: fn(&str) -> bool| {
        controls
            .iter()
            .filter(|c| c.enabled && c.labels.iter().any(|s| matches(&normalized(s))))
            .count()
    };
    format!(
        "complete={} windows={} window_controls={} menu_controls={} meeting_commands={} generic_leave={} participants={} issues={:?}",
        snapshot.complete, snapshot.windows.len(),
        snapshot.windows.iter().map(Vec::len).sum::<usize>(),
        snapshot.menu.as_ref().map_or(0, Vec::len), count(meeting_command),
        count(generic_leave_command), count(participants_control), snapshot.issues,
    )
}

fn generic_leave_command(label: &str) -> bool {
    matches!(label, "leave" | "end" | "выйти" | "завершить")
}

fn participants_control(label: &str) -> bool {
    ["participants", "участники"].iter().any(|prefix| {
        label
            .strip_prefix(prefix)
            .map(|tail| tail.is_empty() || tail.starts_with([' ', '(']))
            .unwrap_or(false)
    })
}

fn normalized(label: &str) -> String {
    label
        .trim()
        .trim_end_matches(['…', '.'])
        .trim()
        .to_lowercase()
}

fn meeting_command(label: &str) -> bool {
    matches!(
        label,
        "leave meeting"
            | "end meeting"
            | "end meeting for all"
            | "покинуть конференцию"
            | "выйти из конференции"
            | "завершить конференцию"
            | "завершить конференцию для всех"
    )
}

fn enabled_labels(controls: &[Control]) -> Vec<String> {
    controls
        .iter()
        .filter(|c| c.enabled)
        .flat_map(|c| c.labels.iter().map(|s| normalized(s)))
        .collect()
}

fn classify(snapshot: &Snapshot) -> Probe {
    let menu = snapshot.menu.as_deref().unwrap_or_default();
    if enabled_labels(menu).iter().any(|s| meeting_command(s)) {
        return Probe {
            presence: Presence::Active,
            reason: "zoom_enabled_meeting_command",
        };
    }
    for window in &snapshot.windows {
        let labels = enabled_labels(window);
        let leave = labels.iter().any(|s| generic_leave_command(s));
        let participants = labels.iter().any(|s| participants_control(s));
        if labels.iter().any(|s| meeting_command(s)) || (leave && participants) {
            return Probe {
                presence: Presence::Active,
                reason: "zoom_enabled_call_controls",
            };
        }
    }
    // A hidden toolbar is not an observed exit. Require a complete, single Home
    // window and an explicitly disabled Leave command, as in the source adapter.
    if snapshot.complete && snapshot.windows.len() == 1 && snapshot.menu.is_some() {
        let labels = enabled_labels(&snapshot.windows[0]);
        let join = labels.iter().any(|s| {
            matches!(
                s.as_str(),
                "join" | "join meeting" | "войти" | "войти в конференцию"
            )
        });
        let new = labels
            .iter()
            .any(|s| matches!(s.as_str(), "new meeting" | "новая конференция"));
        let disabled_leave = menu.iter().any(|c| {
            !c.enabled
                && c.labels.iter().any(|s| {
                    matches!(
                        normalized(s).as_str(),
                        "leave meeting" | "покинуть конференцию" | "выйти из конференции"
                    )
                })
        });
        if join && new && disabled_leave {
            return Probe {
                presence: Presence::Inactive,
                reason: "zoom_idle_home",
            };
        }
    }
    Probe {
        presence: Presence::Unknown,
        reason: snapshot
            .issues
            .first()
            .map(ScanIssue::reason)
            .unwrap_or("zoom_controls_not_found"),
    }
}

#[cfg(target_os = "macos")]
pub use native::{accessibility_trusted, probe, request_accessibility};

#[cfg(test)]
mod tests {
    use super::*;

    fn control(label: &str, enabled: bool) -> Control {
        Control {
            labels: vec![label.into()],
            enabled,
        }
    }
    fn snapshot(windows: &[&[(&str, bool)]], menu: Option<&[(&str, bool)]>) -> Snapshot {
        Snapshot {
            windows: windows
                .iter()
                .map(|w| w.iter().map(|(s, e)| control(s, *e)).collect())
                .collect(),
            menu: menu.map(|m| m.iter().map(|(s, e)| control(s, *e)).collect()),
            complete: true,
            issues: Vec::new(),
        }
    }

    #[test]
    fn idle_home_requires_readable_disabled_leave_command() {
        let home = &[("Join", true), ("New meeting", true)][..];
        assert_eq!(
            classify(&snapshot(&[home], Some(&[("Leave meeting", false)]))).presence,
            Presence::Inactive
        );
        assert_eq!(
            classify(&snapshot(&[home], None)).presence,
            Presence::Unknown
        );
    }

    #[test]
    fn settings_and_disabled_call_controls_never_confirm_a_call() {
        for window in [
            vec![("Test Mic", true), ("Test Speaker", true)],
            vec![("Leave meeting", false), ("Participants", true)],
            vec![("Join meeting", true), ("Start Video", true)],
        ] {
            assert_ne!(
                classify(&snapshot(&[&window], Some(&[("End meeting", false)]))).presence,
                Presence::Active
            );
        }
    }

    #[test]
    fn muted_call_in_background_window_is_active() {
        assert_eq!(
            classify(&snapshot(
                &[
                    &[("Join", true), ("New Meeting", true)],
                    &[
                        ("Unmute", true),
                        ("Leave", true),
                        ("Participants (2)", true)
                    ],
                ],
                None
            ))
            .presence,
            Presence::Active
        );
    }

    #[test]
    fn enabled_english_and_russian_meeting_commands_confirm_without_windows() {
        for label in [
            "Leave Meeting…",
            " End Meeting... ",
            "Покинуть конференцию",
            "Выйти из конференции",
            "Завершить конференцию для всех",
        ] {
            assert_eq!(
                classify(&snapshot(&[], Some(&[(label, true)]))).presence,
                Presence::Active,
                "{label}"
            );
        }
    }

    #[test]
    fn russian_muted_toolbar_and_home_are_recognized() {
        assert_eq!(
            classify(&snapshot(
                &[&[
                    ("Включить звук", true),
                    ("Выйти", true),
                    ("Участники (2)", true)
                ]],
                None
            ))
            .presence,
            Presence::Active
        );
        assert_eq!(
            classify(&snapshot(
                &[&[("Войти", true), ("Новая конференция", true)]],
                Some(&[("Покинуть конференцию", false)])
            ))
            .presence,
            Presence::Inactive
        );
    }

    #[test]
    fn generic_controls_must_belong_to_the_same_window() {
        assert_eq!(
            classify(&snapshot(
                &[&[("Leave", true)], &[("Participants", true)]],
                None
            ))
            .presence,
            Presence::Unknown
        );
        assert_eq!(
            classify(&snapshot(&[], Some(&[("End", true)]))).presence,
            Presence::Unknown
        );
    }

    #[test]
    fn home_alongside_unclassified_window_does_not_prove_exit() {
        assert_eq!(
            classify(&snapshot(
                &[&[("Join", true), ("New Meeting", true)], &[]],
                Some(&[("Leave meeting", false)])
            ))
            .presence,
            Presence::Unknown
        );
    }

    #[test]
    fn incomplete_or_unreadable_scan_does_not_prove_exit() {
        let mut data = snapshot(
            &[&[("Join", true), ("New Meeting", true)]],
            Some(&[("Leave meeting", false)]),
        );
        data.complete = false;
        assert_eq!(classify(&data).presence, Presence::Unknown);
        data.windows.push(vec![control("Leave Meeting", true)]);
        assert_eq!(classify(&data).presence, Presence::Active);
    }

    #[test]
    fn disabled_home_controls_do_not_prove_exit() {
        assert_eq!(
            classify(&snapshot(
                &[&[("Join", false), ("New Meeting", false)]],
                Some(&[("Leave meeting", false)])
            ))
            .presence,
            Presence::Unknown
        );
    }

    #[test]
    fn unknown_diagnostics_distinguish_read_failure_and_scan_limits() {
        let mut data = snapshot(&[&[]], None);
        assert_eq!(classify(&data).reason, "zoom_controls_not_found");
        for (issue, reason) in [
            (ScanIssue::Deadline, "zoom_scan_timeout"),
            (ScanIssue::NodeLimit, "zoom_scan_limit"),
            (ScanIssue::Ax("AXChildren", -25204), "zoom_ax_read_failed"),
            (ScanIssue::Missing("AXRole"), "zoom_ax_incomplete"),
        ] {
            data.issues = vec![issue];
            data.complete = false;
            let result = classify(&data);
            assert_eq!(result.presence, Presence::Unknown);
            assert_eq!(result.reason, reason);
        }
        // Diagnostic failures cannot override positive controls already read.
        data.windows.push(vec![control("Leave Meeting", true)]);
        assert_eq!(classify(&data).presence, Presence::Active);
    }

    #[test]
    fn diagnostic_report_counts_controls_without_exposing_their_text() {
        let data = snapshot(
            &[&[("Private participant name", true), ("Leave", true)]],
            None,
        );
        let report = diagnostic_summary(&data);
        assert!(
            report.contains("generic_leave=1 participants=0"),
            "{report}"
        );
        assert!(!report.contains("Private participant name"));
        assert!(!report.contains("Leave"));
    }
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::{
        ffi::{c_char, c_void, CString},
        ptr,
        time::{Duration, Instant},
    };

    type CFRef = *const c_void;
    const UTF8: u32 = 0x08000100;
    const MAX_WINDOWS: usize = 16;
    const MAX_CHILDREN: usize = 128;
    const MAX_NODES: usize = 768;
    const MAX_DEPTH: usize = 14;
    const SCAN_TIME: Duration = Duration::from_millis(700);
    const MESSAGE_TIMEOUT: f32 = 0.05;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFRef;
        fn AXUIElementCreateApplication(pid: i32) -> CFRef;
        fn AXUIElementGetTypeID() -> usize;
        fn AXUIElementCopyAttributeValue(
            element: CFRef,
            attribute: CFRef,
            value: *mut CFRef,
        ) -> i32;
        fn AXUIElementSetMessagingTimeout(element: CFRef, timeout: f32) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(value: CFRef);
        fn CFRetain(value: CFRef) -> CFRef;
        fn CFGetTypeID(value: CFRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFStringCreateWithCString(
            allocator: CFRef,
            string: *const c_char,
            encoding: u32,
        ) -> CFRef;
        fn CFStringGetCString(
            string: CFRef,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        fn CFArrayGetTypeID() -> usize;
        fn CFArrayGetCount(array: CFRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(value: CFRef) -> bool;
        static kCFBooleanTrue: CFRef;
        fn CFDictionaryCreate(
            allocator: CFRef,
            keys: *const CFRef,
            values: *const CFRef,
            count: isize,
            key_callbacks: CFRef,
            value_callbacks: CFRef,
        ) -> CFRef;
    }

    // Every Copy/Create or retained borrowed reference has exactly one release.
    // These references never leave the blocking scan thread.
    struct Owned(CFRef);
    impl Drop for Owned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) }
            }
        }
    }

    pub fn accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// Only the explicit permission button may call this; scans never prompt.
    /// macOS displays its request asynchronously, so `false` can mean pending.
    pub fn request_accessibility() -> bool {
        unsafe {
            let key = kAXTrustedCheckOptionPrompt;
            let value = kCFBooleanTrue;
            // Both constants outlive this synchronous dictionary/use. Null
            // callbacks deliberately avoid claiming ownership of them.
            let options = Owned(CFDictionaryCreate(
                ptr::null(),
                &key,
                &value,
                1,
                ptr::null(),
                ptr::null(),
            ));
            !options.0.is_null() && AXIsProcessTrustedWithOptions(options.0)
        }
    }

    struct Budget {
        deadline: Instant,
        remaining: usize,
    }
    impl Budget {
        fn check(&self) -> Result<(), ScanIssue> {
            if Instant::now() >= self.deadline {
                Err(ScanIssue::Deadline)
            } else if self.remaining == 0 {
                Err(ScanIssue::NodeLimit)
            } else {
                Ok(())
            }
        }
    }

    unsafe fn attribute(
        element: CFRef,
        name: &'static str,
        budget: &Budget,
    ) -> Result<Option<Owned>, ScanIssue> {
        budget.check()?;
        // AX timeouts belong to this exact reference: setting the application
        // root's timeout does not cover its children (AXUIElement.h). Configure
        // each target before IPC without changing the process-wide AX timeout.
        let timeout_status = AXUIElementSetMessagingTimeout(element, MESSAGE_TIMEOUT);
        if timeout_status != 0 {
            return Err(ScanIssue::Ax("SetMessagingTimeout", timeout_status));
        }
        let key_name = CString::new(name).map_err(|_| ScanIssue::Invalid(name))?;
        let key = Owned(CFStringCreateWithCString(
            ptr::null(),
            key_name.as_ptr(),
            UTF8,
        ));
        if key.0.is_null() {
            return Err(ScanIssue::Invalid(name));
        }
        let mut raw = ptr::null();
        let status = AXUIElementCopyAttributeValue(element, key.0, &mut raw);
        let value = Owned(raw);
        match status {
            0 if !raw.is_null() => Ok(Some(value)),
            // Unsupported and no-value are normal for optional AX attributes.
            -25205 | -25212 => Ok(None),
            _ => Err(ScanIssue::Ax(name, status)),
        }
    }

    unsafe fn text(
        element: CFRef,
        name: &'static str,
        budget: &Budget,
    ) -> Result<Option<String>, ScanIssue> {
        let Some(value) = attribute(element, name, budget)? else {
            return Ok(None);
        };
        if CFGetTypeID(value.0) != CFStringGetTypeID() {
            return Err(ScanIssue::Invalid(name));
        }
        // Control names have a fixed bound; document or message text is never read.
        let mut bytes = [0u8; 2048];
        if !CFStringGetCString(
            value.0,
            bytes.as_mut_ptr().cast(),
            bytes.len() as isize,
            UTF8,
        ) {
            return Err(ScanIssue::Invalid(name));
        }
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or(ScanIssue::Invalid(name))?;
        String::from_utf8(bytes[..end].to_vec())
            .map(Some)
            .map_err(|_| ScanIssue::Invalid(name))
    }

    unsafe fn enabled(element: CFRef, budget: &Budget) -> Result<bool, ScanIssue> {
        let value =
            attribute(element, "AXEnabled", budget)?.ok_or(ScanIssue::Missing("AXEnabled"))?;
        if CFGetTypeID(value.0) != CFBooleanGetTypeID() {
            return Err(ScanIssue::Invalid("AXEnabled"));
        }
        Ok(CFBooleanGetValue(value.0))
    }

    unsafe fn elements(
        array: &Owned,
        max: usize,
        name: &'static str,
    ) -> Result<Vec<Owned>, ScanIssue> {
        if CFGetTypeID(array.0) != CFArrayGetTypeID() {
            return Err(ScanIssue::Invalid(name));
        }
        let count = CFArrayGetCount(array.0);
        if count < 0 {
            return Err(ScanIssue::Invalid(name));
        }
        if count as usize > max {
            return Err(ScanIssue::ChildLimit);
        }
        let mut result = Vec::with_capacity(count as usize);
        for index in 0..count {
            let child = CFArrayGetValueAtIndex(array.0, index);
            if child.is_null() || CFGetTypeID(child) != AXUIElementGetTypeID() {
                return Err(ScanIssue::Invalid(name));
            }
            result.push(Owned(CFRetain(child)));
        }
        Ok(result)
    }

    unsafe fn tree(
        root: Owned,
        menu: bool,
        budget: &mut Budget,
        controls: &mut Vec<Control>,
    ) -> Result<(), ScanIssue> {
        let mut stack = vec![(root, 0)];
        while let Some((node, depth)) = stack.pop() {
            budget.check()?;
            let role = text(node.0, "AXRole", budget)?.ok_or(ScanIssue::Missing("AXRole"))?;
            if (menu && role == "AXMenuItem")
                || (!menu && matches!(role.as_str(), "AXButton" | "AXMenuButton"))
            {
                let mut labels = Vec::new();
                for name in ["AXTitle", "AXDescription", "AXHelp"] {
                    if let Some(label) = text(node.0, name, budget)? {
                        labels.push(label);
                    }
                }
                if !labels.is_empty() {
                    let is_enabled = enabled(node.0, budget)?;
                    let confirmed =
                        is_enabled && labels.iter().any(|s| meeting_command(&normalized(s)));
                    controls.push(Control {
                        labels,
                        enabled: is_enabled,
                    });
                    if confirmed {
                        return Ok(());
                    }
                }
            }
            // Never descend into chat/text/document content. The positive idle
            // test remains deliberately narrow even for a fully traversed tree.
            if !matches!(
                role.as_str(),
                "AXTextArea" | "AXTextField" | "AXList" | "AXTable"
            ) {
                if let Some(children) = attribute(node.0, "AXChildren", budget)? {
                    let children = elements(&children, MAX_CHILDREN, "AXChildren")?;
                    if !children.is_empty() && depth >= MAX_DEPTH {
                        return Err(ScanIssue::DepthLimit);
                    }
                    if stack.len() + children.len() > MAX_NODES {
                        return Err(ScanIssue::NodeLimit);
                    }
                    for child in children.into_iter().rev() {
                        stack.push((child, depth + 1));
                    }
                } else if matches!(
                    role.as_str(),
                    "AXWindow"
                        | "AXGroup"
                        | "AXToolbar"
                        | "AXWebArea"
                        | "AXScrollArea"
                        | "AXSplitGroup"
                        | "AXMenuBar"
                        | "AXMenu"
                        | "AXMenuBarItem"
                ) {
                    return Err(ScanIssue::Missing("AXChildren"));
                }
            }
            budget.remaining = budget.remaining.saturating_sub(1);
        }
        Ok(())
    }

    unsafe fn inspect(pid: u32, budget: &mut Budget, diagnostics: bool) -> Snapshot {
        let mut snapshot = Snapshot {
            complete: true,
            ..Snapshot::default()
        };
        let Ok(pid) = i32::try_from(pid) else {
            return Snapshot::default();
        };
        if pid <= 0 {
            return Snapshot::default();
        }
        if let Err(issue) = budget.check() {
            snapshot.complete = false;
            snapshot.issues.push(issue);
            return snapshot;
        }
        let root = Owned(AXUIElementCreateApplication(pid));
        if root.0.is_null() {
            snapshot.complete = false;
            snapshot.issues.push(ScanIssue::Invalid("Application"));
            return snapshot;
        }
        // Menu evidence survives hidden/background toolbars. Reserve most of the
        // total budget for windows if this optional menu tree is unresponsive.
        let mut menu_budget = Budget {
            deadline: budget
                .deadline
                .min(Instant::now() + Duration::from_millis(200)),
            remaining: budget.remaining.min(192),
        };
        let menu_start = menu_budget.remaining;
        let mut commands = Vec::new();
        let menu_result = attribute(root.0, "AXMenuBar", &menu_budget)
            .and_then(|m| m.ok_or(ScanIssue::Missing("AXMenuBar")))
            .and_then(|menu| tree(menu, true, &mut menu_budget, &mut commands));
        budget.remaining = budget
            .remaining
            .saturating_sub(menu_start - menu_budget.remaining);
        // Positive evidence already read remains useful even if a later branch
        // fails; an incomplete menu must never supply negative evidence.
        if let Err(issue) = menu_result {
            if diagnostics {
                log::info!("Zoom AX diagnostic: stage=menu issue={issue:?}");
            }
            snapshot.issues.push(issue);
            snapshot.complete = false;
            snapshot.menu = Some(commands.into_iter().filter(|c| c.enabled).collect());
        } else {
            snapshot.menu = Some(commands);
        }
        if classify(&snapshot).presence == Presence::Active {
            return snapshot;
        }
        let windows = attribute(root.0, "AXWindows", budget)
            .and_then(|w| w.ok_or(ScanIssue::Missing("AXWindows")))
            .and_then(|w| elements(&w, MAX_WINDOWS, "AXWindows"));
        let windows = match windows {
            Ok(windows) => windows,
            Err(issue) => {
                if diagnostics {
                    log::info!("Zoom AX diagnostic: stage=windows issue={issue:?}");
                }
                snapshot.complete = false;
                snapshot.issues.push(issue);
                return snapshot;
            }
        };
        for (index, window) in windows.into_iter().enumerate() {
            let mut controls = Vec::new();
            if let Err(issue) = tree(window, false, budget, &mut controls) {
                if diagnostics {
                    log::info!("Zoom AX diagnostic: stage=window index={index} issue={issue:?}");
                }
                snapshot.issues.push(issue);
                snapshot.complete = false;
            }
            snapshot.windows.push(controls);
        }
        snapshot
    }

    /// PIDs must already be identified as Zoom's main desktop process by caller.
    /// One shared deadline bounds all processes; a single AX request can exceed
    /// it by at most the configured 50 ms messaging timeout (OS scheduling aside).
    pub fn probe(pids: &[u32], diagnostics: bool) -> Probe {
        if pids.is_empty() {
            return Probe {
                presence: Presence::Inactive,
                reason: "zoom_not_running",
            };
        }
        if !accessibility_trusted() {
            return Probe {
                presence: Presence::Unknown,
                reason: "accessibility_permission_required",
            };
        }
        let mut budget = Budget {
            deadline: Instant::now() + SCAN_TIME,
            remaining: MAX_NODES,
        };
        let mut unknown_reason = (pids.len() > 8).then_some("zoom_scan_limit");
        for pid in pids.iter().take(8) {
            let snapshot = unsafe { inspect(*pid, &mut budget, diagnostics) };
            let observed = classify(&snapshot);
            if diagnostics {
                log::info!(
                    "Zoom AX diagnostic: pid={pid} {} state={:?} reason={}",
                    diagnostic_summary(&snapshot),
                    observed.presence,
                    observed.reason
                );
            }
            match observed.presence {
                Presence::Active => return observed,
                Presence::Unknown => {
                    unknown_reason.get_or_insert(observed.reason);
                }
                Presence::Inactive => (),
            }
        }
        if let Some(reason) = unknown_reason {
            Probe {
                presence: Presence::Unknown,
                reason,
            }
        } else {
            Probe {
                presence: Presence::Inactive,
                reason: "zoom_idle_home",
            }
        }
    }
}
