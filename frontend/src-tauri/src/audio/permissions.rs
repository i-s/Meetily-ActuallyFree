// macOS audio permissions handling.
//
// The `screen_recording` names are legacy IPC/API names. Current macOS capture
// uses Audio Capture permission and a Core Audio process tap, not screen video.
// `check_screen_recording_permission` reports platform support only; the audible
// up-to-five-second probe below is the actual runtime verification.
use anyhow::Result;

#[path = "permission_probe.rs"]
mod permission_probe;
pub use permission_probe::SystemAudioProbeResult;
use log::{info, warn, error};

#[cfg(target_os = "macos")]
use std::process::Command;

/// Check whether the platform supports the Audio Capture permission flow.
///
/// Note: Core Audio taps require NSAudioCaptureUsageDescription in Info.plist.
/// When the app first attempts to create a Core Audio tap, macOS will automatically
/// show a permission dialog to the user. If permission is denied, the tap will return
/// silence (all zeros).
///
/// This function returns true because the actual permission prompt happens automatically
/// when AudioHardwareCreateProcessTap is called by the cidre library.
#[cfg(target_os = "macos")]
pub fn check_screen_recording_permission() -> bool {
    info!("ℹ️  Core Audio tap requires Audio Capture permission (macOS 14.2+)");
    info!("📍 Permission dialog will appear automatically when recording starts");
    info!("   If already granted: System Settings → Privacy & Security → Audio Capture");

    // Always return true - the actual permission dialog is triggered by Core Audio API
    true
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_recording_permission() -> bool {
    true // Not required on other platforms
}

/// Request Audio Capture permission from the user
/// This will open System Settings to the Privacy & Security page
#[cfg(target_os = "macos")]
pub fn request_screen_recording_permission() -> Result<()> {
    info!("🔐 Opening System Settings for Audio Capture permission...");

    // Open System Settings to Privacy & Security page
    // Note: There's no direct URL for Audio Capture, so we open the main Privacy page
    let result = Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security")
        .spawn();

    match result {
        Ok(_) => {
            info!("✅ Opened System Settings - navigate to Privacy & Security → Audio Capture");
            info!("👉 Please enable Audio Capture permission and restart the app");
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to open System Settings: {}", e);
            Err(anyhow::anyhow!("Failed to open System Settings: {}", e))
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_recording_permission() -> Result<()> {
    Ok(()) // Not required on other platforms
}

/// Check and request Audio Capture permission if not granted
/// Returns true if permission is granted, false otherwise
pub fn ensure_screen_recording_permission() -> bool {
    if check_screen_recording_permission() {
        return true;
    }

    warn!("Audio Capture permission not granted - requesting...");

    if let Err(e) = request_screen_recording_permission() {
        error!("Failed to request Audio Capture permission: {}", e);
        return false;
    }

    false // Permission will be granted after restart
}

/// Tauri command to check Screen Recording permission
#[tauri::command]
pub async fn check_screen_recording_permission_command() -> bool {
    check_screen_recording_permission()
}

/// Tauri command to request Screen Recording permission
#[tauri::command]
pub async fn request_screen_recording_permission_command() -> Result<(), String> {
    request_screen_recording_permission()
        .map_err(|e| e.to_string())
}

/// Trigger the permission prompt and classify a bounded native capture probe.
#[cfg(target_os = "macos")]
pub fn trigger_system_audio_permission() -> Result<SystemAudioProbeResult> {
    let result = permission_probe::run_probe(|| {
        let capture = crate::audio::capture::CoreAudioCapture::new()?;
        capture.probe(std::time::Duration::from_secs(5))
    });
    info!("Audio Capture probe: {:?}", result);
    Ok(result)
}

#[cfg(not(target_os = "macos"))]
pub fn trigger_system_audio_permission() -> Result<SystemAudioProbeResult> {
    // These platforms have no separate Audio Capture permission prompt.
    Ok(SystemAudioProbeResult {
        detected: true,
        conclusive: true,
        reason: "permission_not_required",
    })
}

/// Trigger Audio Capture permission and test for audible samples for up to five
/// seconds. An inaudible probe is inconclusive, even with zero callbacks.
#[tauri::command]
pub async fn trigger_system_audio_permission_command() -> Result<SystemAudioProbeResult, String> {
    // Run in blocking task to avoid blocking the async runtime
    tokio::task::spawn_blocking(|| {
        trigger_system_audio_permission()
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_permission() {
        let has_permission = check_screen_recording_permission();
        println!("Has Screen Recording permission: {}", has_permission);
    }
}
