//! Device-independent classification shared by the native permission command
//! and its tests. Silence is not evidence of authorization denial.
use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SystemAudioProbeResult {
    pub detected: bool,
    pub conclusive: bool,
    pub reason: &'static str,
}

pub(super) fn run_probe(probe: impl FnOnce() -> Result<bool>) -> SystemAudioProbeResult {
    match probe() {
        Ok(detected) => SystemAudioProbeResult {
            detected,
            conclusive: detected,
            reason: if detected { "audio_detected" } else { "no_audio_detected" },
        },
        Err(error) => {
            // Core Audio's explicit `perm` status is mapped to PermissionDenied
            // at the native boundary. Human-readable/unknown errors (including
            // messages mentioning permissions) cannot prove authorization state.
            let denied = error.chain().any(|cause| {
                cause.downcast_ref::<std::io::Error>().is_some_and(|e| {
                    e.kind() == std::io::ErrorKind::PermissionDenied
                })
            });
            SystemAudioProbeResult {
                detected: false,
                conclusive: denied,
                reason: if denied { "permission_denied" } else { "probe_failed" },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_inconclusive() {
        let result = run_probe(|| Ok(false));
        assert!(!result.detected);
        assert!(!result.conclusive);
        assert_eq!(result.reason, "no_audio_detected");
    }

    #[test]
    fn audible_samples_verify_capture() {
        let result = run_probe(|| Ok(true));
        assert!(result.detected && result.conclusive);
        assert_eq!(result.reason, "audio_detected");
    }

    #[test]
    fn explicit_denial_is_conclusive() {
        for error in [
            anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                .context("Failed to create tap"),
        ] {
            let result = run_probe(|| Err(error));
            assert!(!result.detected && result.conclusive);
            assert_eq!(result.reason, "permission_denied");
        }
    }

    #[test]
    fn unrelated_errors_are_inconclusive() {
        for message in ["permission API unavailable", "permission denied status unavailable", "device disconnected", "failed to create process tap"] {
            let result = run_probe(|| Err(anyhow::anyhow!(message)));
            assert!(!result.detected && !result.conclusive);
            assert_eq!(result.reason, "probe_failed");
        }
    }
}
