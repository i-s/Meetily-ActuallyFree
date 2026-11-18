/**
 * Starts and stops a recording from anywhere in the app. The recorder page
 * owns both sequences, so from another page these open the recorder with a
 * flag it picks up on arrival; on the recorder they act right away.
 */
import type { RecordingLease } from './native-recording';
import type { AutomatedCall } from './meeting-automation';
import { writePendingGroup, type PendingGroup } from '@/lib/groups';

export const AUTO_START_KEY = 'autoStartRecording';
export const START_RECORDING_EVENT = 'start-recording-from-sidebar';
export const STOP_REQUEST_KEY = 'af-stop-requested';
export const STOP_RECORDING_EVENT = 'af-stop-recording';

export function launchRecording(navigate: (href: string) => void, options: { group?: PendingGroup | null; automated?: AutomatedCall } = {}) {
  const intent = options.automated ? { kind: 'automated', call: options.automated } : { kind: 'manual' };
  if (options.group !== undefined) writePendingGroup(options.group);
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(START_RECORDING_EVENT, { detail: intent }));
    return;
  }
  try {
    sessionStorage.setItem(AUTO_START_KEY, JSON.stringify(intent));
  } catch {
    // Without storage the recorder opens and waits for the button.
  }
  navigate('/');
}

/** Stop and save, the same way the record card's Stop button does. */
export function requestRecordingStop(navigate: (href: string) => void, expected?: RecordingLease) {
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT, { detail: expected }));
    return;
  }
  try {
    sessionStorage.setItem(STOP_REQUEST_KEY, expected ? JSON.stringify({ expected }) : '1');
  } catch {
    // Without storage the user can still stop from the recorder.
  }
  navigate('/');
}

/** Preserve a routed automated request verbatim, including its old generation.
 * Invalid payloads are discarded, NEVER downgraded to an unguarded manual Stop.
 */
export function takeRecordingStopRequest(): { expected?: RecordingLease } | null {
  try {
    const value = sessionStorage.getItem(STOP_REQUEST_KEY);
    sessionStorage.removeItem(STOP_REQUEST_KEY);
    if (value === '1') return {};
    if (!value) return null;
    const parsed = JSON.parse(value);
    return validRecordingLease(parsed.expected) ? parsed : null;
  } catch { return null; }
}
export function validRecordingLease(value: unknown): value is RecordingLease {
  if (!value || typeof value !== 'object') return false;
  const lease = value as RecordingLease;
  return typeof lease.generation === 'string' && !!lease.generation && (lease.owner === null
    || (!!lease.owner && typeof lease.owner.source === 'string' && typeof lease.owner.session_id === 'string'
      && typeof lease.owner.app === 'string' && typeof lease.owner.process === 'string'));
}
