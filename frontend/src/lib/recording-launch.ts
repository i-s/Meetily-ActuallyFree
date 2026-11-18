/**
 * Starts and stops a recording from anywhere in the app. The recorder page
 * owns both sequences, so from another page these open the recorder with a
 * flag it picks up on arrival; on the recorder they act right away.
 */
import { writePendingGroup, type PendingGroup } from '@/lib/groups';
import { isAutomatedCall, reservationCurrent, type AutomatedCall } from './meeting-automation';

export const AUTO_START_KEY = 'autoStartRecording';
export const START_RECORDING_EVENT = 'start-recording-from-sidebar';
export const STOP_REQUEST_KEY = 'af-stop-requested';
export const STOP_RECORDING_EVENT = 'af-stop-recording';

export type RecordingLaunchIntent = { kind: 'absent' } | { kind: 'manual' } |
  { kind: 'automated'; call: AutomatedCall } | { kind: 'rejected'; reason: string };
/** Absence and parse failure are different. Never turn an invalid automatic
 * handoff into a manual Start, nor replace its token with the latest reservation.
 */
export function readRecordingLaunchIntent(payload: unknown): RecordingLaunchIntent {
  if (payload === null || payload === undefined) return { kind: 'absent' };
  try {
    const value = typeof payload === 'string' ? JSON.parse(payload) : payload;
    if (value?.kind === 'manual') return { kind: 'manual' };
    if (value?.kind === 'automated' && isAutomatedCall(value.call) && reservationCurrent(value.call, 'pending')) {
      return { kind: 'automated', call: value.call };
    }
  } catch { /* A malformed routed flag remains rejected. */ }
  return { kind: 'rejected', reason: 'invalid_or_expired_recording_launch' };
}
export function takeRecordingLaunchIntent(): RecordingLaunchIntent {
  try {
    const raw = sessionStorage.getItem(AUTO_START_KEY);
    sessionStorage.removeItem(AUTO_START_KEY);
    return readRecordingLaunchIntent(raw);
  } catch { return { kind: 'rejected', reason: 'launch_storage_unavailable' }; }
}
export function launchRecording(navigate: (href: string) => void, options: { group?: PendingGroup | null; automated?: AutomatedCall } = {}) {
  if (options.group !== undefined) writePendingGroup(options.group);
  // Property presence preserves automated intent even for a malformed caller.
  const payload = 'automated' in options ? { kind: 'automated', call: options.automated ?? null } : { kind: 'manual' };
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(START_RECORDING_EVENT, { detail: payload }));
    return;
  }
  try {
    sessionStorage.setItem(AUTO_START_KEY, JSON.stringify(payload));
  } catch {
    // Without storage the recorder opens and waits for the explicit button.
  }
  navigate('/');
}

/** Stop and save, the same way the record card's Stop button does. */
export function requestRecordingStop(navigate: (href: string) => void) {
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT));
    return;
  }
  try {
    sessionStorage.setItem(STOP_REQUEST_KEY, '1');
  } catch {
    // Without storage the user can still stop from the recorder.
  }
  navigate('/');
}
