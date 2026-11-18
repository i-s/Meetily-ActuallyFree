//! Read-only macOS AX reader adapted from prior fork 2169cab macos.rs.
//! It never reads AXValue, messages, credentials, sockets, or activates a window.
use super::evidence::*;
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
    fn AXUIElementCreateApplication(pid: i32) -> CFRef;
    fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementCopyAttributeValue(element: CFRef, attribute: CFRef, value: *mut CFRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFRef, timeout: f32) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFRef);
    fn CFRetain(value: CFRef) -> CFRef;
    fn CFGetTypeID(value: CFRef) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithCString(allocator: CFRef, string: *const c_char, encoding: u32) -> CFRef;
    fn CFStringGetCString(string: CFRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFArrayGetTypeID() -> usize;
    fn CFArrayGetCount(array: CFRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFRef, index: isize) -> CFRef;
    fn CFBooleanGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CFRef) -> bool;
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
    let value = attribute(element, "AXEnabled", budget)?.ok_or(ScanIssue::Missing("AXEnabled"))?;
    if CFGetTypeID(value.0) != CFBooleanGetTypeID() {
        return Err(ScanIssue::Invalid("AXEnabled"));
    }
    Ok(CFBooleanGetValue(value.0))
}

unsafe fn elements(array: &Owned, max: usize, name: &'static str) -> Result<Vec<Owned>, ScanIssue> {
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
    budget: &mut Budget,
    controls: &mut Vec<Control>,
) -> Result<(), ScanIssue> {
    let mut stack = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        budget.check()?;
        let role = text(node.0, "AXRole", budget)?.ok_or(ScanIssue::Missing("AXRole"))?;
        if matches!(role.as_str(), "AXButton" | "AXMenuButton") {
            let mut labels = Vec::new();
            for name in ["AXTitle", "AXDescription", "AXHelp"] {
                if let Some(label) = text(node.0, name, budget)? {
                    labels.push(label);
                }
            }
            if !labels.is_empty() {
                let is_enabled = enabled(node.0, budget)?;
                controls.push(Control {
                    labels,
                    enabled: is_enabled,
                });
            }
        }
        // Never descend into text input or list/table content. Only control
        // labels are read; no labels are logged or persisted.
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

unsafe fn inspect(pid: u32, budget: &mut Budget) -> Snapshot {
    let mut snapshot = Snapshot {
        complete: true,
        ..Snapshot::default()
    };
    let root = Owned(AXUIElementCreateApplication(pid as i32));
    if root.0.is_null() {
        snapshot.complete = false;
        return snapshot;
    }
    let windows = attribute(root.0, "AXWindows", budget)
        .and_then(|w| w.ok_or(ScanIssue::Missing("AXWindows")))
        .and_then(|w| elements(&w, MAX_WINDOWS, "AXWindows"));
    match windows {
        Ok(windows) => {
            for window in windows {
                let mut controls = Vec::new();
                if let Err(issue) = tree(window, budget, &mut controls) {
                    snapshot.complete = false;
                    snapshot.issues.push(issue);
                }
                snapshot.windows.push(controls);
            }
        }
        Err(issue) => {
            snapshot.complete = false;
            snapshot.issues.push(issue);
        }
    }
    snapshot
}

pub fn probe() -> Probe {
    // Exact main-bundle executable paths, not Helper/PID/mic substring guesses.
    // Stable desktop only. PTB, Canary and browser clients are unqualified.
    // Only refresh executable identity, not process environment/command lines.
    let mut system = sysinfo::System::new();
    let refreshed = system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::new().with_exe(sysinfo::UpdateKind::Always),
    );
    if refreshed == 0 {
        return Probe {
            presence: Presence::Unknown,
            reason: "process_inventory_unavailable",
        };
    }
    let pids: Vec<_> = system
        .processes()
        .values()
        .filter(|p| {
            p.exe()
                .is_some_and(|p| p.ends_with("Discord.app/Contents/MacOS/Discord"))
        })
        .map(|p| p.pid().as_u32())
        .collect();
    if pids.is_empty() {
        if system.processes().values().any(|p| {
            p.exe().is_none() && p.name().to_string_lossy().eq_ignore_ascii_case("discord")
        }) {
            return Probe {
                presence: Presence::Unknown,
                reason: "client_identity_unreadable",
            };
        }
        return Probe {
            presence: Presence::Inactive,
            reason: "stable_client_not_running",
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
    if pids.len() != 1 {
        return Probe {
            presence: Presence::Unknown,
            reason: "multiple_clients_unqualified",
        };
    }
    classify(&unsafe { inspect(pids[0], &mut budget) })
}
