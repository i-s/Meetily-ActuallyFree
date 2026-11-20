//! Native Discord call evidence. WASAPI capture is corroboration, never a call
//! on its own (Discord's microphone test opens the same capture sessions).
//!
//! UI Automation inspects only exact call/toolbar labels in Discord-owned
//! windows; no message content is retained. A minimized/tray-only window,
//! inaccessible Chromium tree, unfamiliar locale, or timeout is Unknown.
//! The single MTA worker owns every COM object. A stuck provider can disable
//! this probe, but cannot block the monitor or spawn unbounded replacement workers.
use super::evidence::{Presence, Probe};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};
use windows::core::{Interface, Result, VARIANT};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::Media::Audio::{
    eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
    IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation2, PropertyConditionFlags_IgnoreCase, TreeScope_Descendants,
    UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_IsEnabledPropertyId, UIA_IsOffscreenPropertyId, UIA_NamePropertyId,
    UIA_ProcessIdPropertyId, UIA_TextControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};

#[path = "discord_evidence.rs"]
mod classifier;
use classifier::{classify, ControlKind, UiEvidence, LABELS};

const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_WINDOWS: usize = 8;
const MAX_MATCHED_CONTROLS: i32 = 128;

struct Request {
    pids: Vec<u32>,
    deadline: Instant,
    reply: mpsc::SyncSender<Probe>,
}

fn unknown(reason: &'static str) -> Probe {
    Probe {
        presence: Presence::Unknown,
        reason,
    }
}

/// `pids` must contain only exact native Discord/DiscordPTB/DiscordCanary
/// executable matches from the current process snapshot, including helpers.
pub fn probe(pids: &[u32]) -> Probe {
    if pids.is_empty() {
        return classify(false, &[], Presence::Inactive);
    }
    static WORKER: OnceLock<Option<mpsc::SyncSender<Request>>> = OnceLock::new();
    let Some(sender) = WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        std::thread::Builder::new()
            .name("discord-call-evidence".into())
            .spawn(move || worker(receiver))
            .ok()
            .map(|_| sender)
    }) else {
        return unknown("discord_probe_worker_unavailable");
    };
    let (reply, result) = mpsc::sync_channel(1);
    let request = Request {
        pids: pids.to_vec(),
        deadline: Instant::now() + PROBE_TIMEOUT,
        reply,
    };
    match sender.try_send(request) {
        Ok(()) => {}
        Err(mpsc::TrySendError::Full(_)) => return unknown("discord_probe_busy"),
        Err(mpsc::TrySendError::Disconnected(_)) => {
            return unknown("discord_probe_worker_unavailable");
        }
    }
    match result.recv_timeout(PROBE_TIMEOUT) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => unknown("discord_probe_timeout"),
        Err(mpsc::RecvTimeoutError::Disconnected) => unknown("discord_probe_worker_unavailable"),
    }
}

fn worker(receiver: mpsc::Receiver<Request>) {
    // COM apartments are thread-local: create, use and release here. A failed
    // CoInitializeEx must not be balanced with CoUninitialize.
    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
        return;
    }
    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _guard = ComGuard;
    for request in receiver {
        if Instant::now() >= request.deadline {
            continue;
        }
        let result = unsafe { probe_native(&request.pids, request.deadline) };
        // Never publish a result that outlived this request. Each invocation
        // also owns its reply channel, so a late result cannot cross polls.
        if Instant::now() < request.deadline {
            let _ = request.reply.try_send(result);
        }
    }
}

unsafe fn capture_presence(pids: &[u32], deadline: Instant) -> Result<Presence> {
    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
    let count = devices.GetCount()?;
    // Endpoint/session counts are bounded defensively; partial scans cannot
    // establish inactivity. No sample capture or audio-level polling is needed.
    if count > 64 {
        return Ok(Presence::Unknown);
    }
    let mut incomplete = false;
    for index in 0..count {
        if Instant::now() >= deadline {
            return Ok(Presence::Unknown);
        }
        let device = devices.Item(index)?;
        let manager = match device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) {
            Ok(manager) => manager,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        let sessions = manager.GetSessionEnumerator()?;
        let count = sessions.GetCount()?;
        if !(0..=256).contains(&count) {
            return Ok(Presence::Unknown);
        }
        for index in 0..count {
            if Instant::now() >= deadline {
                return Ok(Presence::Unknown);
            }
            let session = sessions.GetSession(index)?;
            let control = session.cast::<IAudioSessionControl2>()?;
            if pids.contains(&control.GetProcessId()?)
                && session.GetState()? == AudioSessionStateActive
            {
                return Ok(Presence::Active);
            }
        }
    }
    Ok(if incomplete {
        Presence::Unknown
    } else {
        Presence::Inactive
    })
}

struct Windows {
    pids: Vec<u32>,
    handles: Vec<HWND>,
    truncated: bool,
}

unsafe extern "system" fn collect_window(window: HWND, parameter: LPARAM) -> BOOL {
    let state = &mut *(parameter.0 as *mut Windows);
    let mut pid = 0;
    GetWindowThreadProcessId(window, Some(&mut pid));
    if state.pids.contains(&pid) && IsWindowVisible(window).as_bool() && !IsIconic(window).as_bool()
    {
        if state.handles.len() == MAX_WINDOWS {
            state.truncated = true;
            return BOOL(0);
        }
        state.handles.push(window);
    }
    BOOL(1)
}

unsafe fn ui_evidence(pids: &[u32], deadline: Instant) -> Result<Vec<UiEvidence>> {
    let mut state = Windows {
        pids: pids.to_vec(),
        handles: Vec::new(),
        truncated: false,
    };
    let enumeration = EnumWindows(
        Some(collect_window),
        LPARAM(&mut state as *mut Windows as isize),
    );
    if !state.truncated {
        enumeration?;
    }
    if state.handles.is_empty() {
        return Ok(Vec::new());
    }
    let automation: IUIAutomation2 = CoCreateInstance(&CUIAutomation8, None, CLSCTX_ALL)?;
    automation.SetConnectionTimeout(250)?;
    automation.SetTransactionTimeout(350)?;
    let conditions = LABELS
        .iter()
        .map(|label| {
            automation
                .CreatePropertyConditionEx(
                    UIA_NamePropertyId,
                    &VARIANT::from(*label),
                    PropertyConditionFlags_IgnoreCase,
                )
                .map(Some)
        })
        .collect::<Result<Vec<_>>>()?;
    let condition = automation.CreateOrConditionFromNativeArray(&conditions)?;
    let cache = automation.CreateCacheRequest()?;
    for property in [
        UIA_NamePropertyId,
        UIA_ControlTypePropertyId,
        UIA_ProcessIdPropertyId,
        UIA_IsOffscreenPropertyId,
        UIA_IsEnabledPropertyId,
    ] {
        cache.AddProperty(property)?;
    }
    let mut windows = Vec::new();
    if state.truncated {
        windows.push(UiEvidence::default());
    }
    for window in state.handles {
        if Instant::now() >= deadline {
            windows.push(UiEvidence::default());
            break;
        }
        match scan_window(&automation, window, pids, &condition, &cache) {
            Ok(evidence) => windows.push(evidence),
            Err(_) => windows.push(UiEvidence::default()),
        }
    }
    Ok(windows)
}

unsafe fn scan_window(
    automation: &IUIAutomation2,
    window: HWND,
    pids: &[u32],
    condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
    cache: &windows::Win32::UI::Accessibility::IUIAutomationCacheRequest,
) -> Result<UiEvidence> {
    let root = automation.ElementFromHandle(window)?;
    let elements = root.FindAllBuildCache(TreeScope_Descendants, condition, cache)?;
    let count = elements.Length()?;
    if !(0..=MAX_MATCHED_CONTROLS).contains(&count) {
        return Ok(UiEvidence::default());
    }
    let mut evidence = UiEvidence::default();
    for index in 0..count {
        let element = elements.GetElement(index)?;
        let pid = element.CachedProcessId()?;
        if pid <= 0
            || !pids.contains(&(pid as u32))
            || element.CachedIsOffscreen()?.as_bool()
            || !element.CachedIsEnabled()?.as_bool()
        {
            continue;
        }
        let control = element.CachedControlType()?;
        let kind = if control == UIA_ButtonControlTypeId {
            ControlKind::Button
        } else if control == UIA_CheckBoxControlTypeId {
            ControlKind::Toggle
        } else if control == UIA_TextControlTypeId {
            ControlKind::Text
        } else {
            ControlKind::Other
        };
        evidence.observe(&element.CachedName()?.to_string(), kind);
    }
    Ok(evidence)
}

unsafe fn probe_native(pids: &[u32], deadline: Instant) -> Probe {
    let capture = capture_presence(pids, deadline).unwrap_or(Presence::Unknown);
    if Instant::now() >= deadline {
        return unknown("discord_probe_timeout");
    }
    match ui_evidence(pids, deadline) {
        Ok(windows) => classify(true, &windows, capture),
        Err(_) => unknown("discord_ui_unavailable"),
    }
}
