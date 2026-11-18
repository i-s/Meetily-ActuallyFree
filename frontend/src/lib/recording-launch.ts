/**
 * Starts and stops a recording from anywhere in the app. The recorder page
 * owns both sequences, so from another page these open the recorder with a
 * flag it picks up on arrival; on the recorder they act right away.
 */
import type { RecordingLease } from './recording-start-authority';
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

export type RecordingStopRequest = { expected?: RecordingLease };

/** A missing/malformed automated lease is not manual intent. The legacy routed
 * "1" and typed manual event are explicit user requests; bare events are not.
 */
export function readRecordingStopRequest(payload: unknown): RecordingStopRequest | null {
  try {
    if (payload === '1') return {};
    const value = typeof payload === 'string' ? JSON.parse(payload) : payload;
    if (value?.kind === 'manual') return {};
    if (validRecordingLease(value?.expected)) return { expected: value.expected };
    if (validRecordingLease(value)) return { expected: value };
  } catch { /* Corrupt routing never becomes manual. */ }
  return null;
}

/** One durable pending slot: the first valid request wins a storage/event
 * collision. Never replace an old guarded lease with another lease or manual
 * fallback while the recorder page is still synchronizing native state.
 */
export function queueRecordingStopRequest(request: RecordingStopRequest): RecordingStopRequest {
  try {
    const pending = readRecordingStopRequest(sessionStorage.getItem(STOP_REQUEST_KEY));
    if (pending) return pending;
    sessionStorage.setItem(STOP_REQUEST_KEY, request.expected ? JSON.stringify({ expected: request.expected }) : '1');
  } catch { /* A mounted Controls listener can still retain the event locally. */ }
  return request;
}

/** Stop and save through the recorder card; persist even on the recorder route
 * because Controls may not have mounted or received its initial native sync.
 */
export function requestRecordingStop(navigate: (href: string) => void, expected?: RecordingLease) {
  const request = arguments.length > 1 ? (validRecordingLease(expected) ? { expected } : null) : {};
  if (!request) return;
  const pending = queueRecordingStopRequest(request);
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT, { detail: pending.expected ?? { kind: 'manual' } }));
  } else navigate('/');
}

/** Consume only when Controls is ready to dispatch. Until then storage owns
 * the exact request across state sync, unmount/remount and event delivery.
 */
export function takeRecordingStopRequest(): RecordingStopRequest | null {
  try {
    const request = readRecordingStopRequest(sessionStorage.getItem(STOP_REQUEST_KEY));
    sessionStorage.removeItem(STOP_REQUEST_KEY);
    return request;
  } catch { return null; }
}
export function validRecordingLease(value: unknown): value is RecordingLease {
  if (!value || typeof value !== 'object') return false;
  const lease = value as RecordingLease;
  return typeof lease.generation === 'string' && !!lease.generation && (lease.owner === null
    || (!!lease.owner && typeof lease.owner.source === 'string' && typeof lease.owner.session_id === 'string'
      && typeof lease.owner.app === 'string' && typeof lease.owner.process === 'string'));
}
