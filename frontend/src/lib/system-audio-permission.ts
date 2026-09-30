/** Native probe evidence, distinct from device availability and silent audio. */
export interface SystemAudioProbeResult {
  detected: boolean;
  conclusive: boolean;
  reason: string;
}

export type SystemAudioPermission = 'verified' | 'denied' | 'unknown';

// Session-only evidence. Legacy booleans conflated silence with denied, so the
// old key is deliberately ignored (including legacy true after a code upgrade).
export const MACOS_SYSTEM_AUDIO_VERIFIED_KEY = 'macos_system_audio_evidence_v2';

export function readSystemAudioPermission(): SystemAudioPermission {
  try {
    const cached = JSON.parse(window.sessionStorage.getItem(MACOS_SYSTEM_AUDIO_VERIFIED_KEY) ?? 'null');
    return cached?.version === 2 && (cached.status === 'verified' || cached.status === 'denied')
      ? cached.status : 'unknown';
  } catch {
    return 'unknown';
  }
}

export function applySystemAudioProbe(
  result: SystemAudioProbeResult,
  previous: SystemAudioPermission = readSystemAudioPermission(),
): SystemAudioPermission {
  // Unknown results (including malformed/legacy IPC responses) add no evidence.
  const next = result?.conclusive === true
    ? result.detected === true ? 'verified' : result.detected === false ? 'denied' : previous
    : previous;
  if (result?.conclusive === true && next !== 'unknown') {
    try {
      window.sessionStorage.setItem(MACOS_SYSTEM_AUDIO_VERIFIED_KEY, JSON.stringify({ version: 2, status: next }));
    } catch { /* Storage may be unavailable; callers retain in-memory evidence. */ }
  }
  return next;
}
