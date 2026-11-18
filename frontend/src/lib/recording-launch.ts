import { invoke } from '@tauri-apps/api/core';
import type { NativeRecordingState } from './native-recording';
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

export type RecordingStopRequest =
  | { kind: 'automated'; id: string; expected: RecordingLease }
  | { kind: 'manual'; id: string; state: 'resolving'; generation: null }
  | { kind: 'manual'; id: string; state: 'ready'; generation: string };
interface StopQueue { version: 2; requests: RecordingStopRequest[]; done: string[] }
const MAX_STOP_REQUESTS = 16;
const MAX_STOP_RECEIPTS = 64;
const resolvingStops = new Set<string>();
const claimedStops = new Set<string>();
// Failed durable completion is a queue-wide barrier, not permission to retry
// Stop. At most the bounded pending queue can complete concurrently.
const pendingStopAcknowledgements = new Set<string>();
function acknowledgeCompletedStops(): boolean {
  if (!pendingStopAcknowledgements.size) return true;
  try {
    const queue = readStopQueue();
    queue.requests = queue.requests.filter(item => !pendingStopAcknowledgements.has(item.id));
    queue.done = [...queue.done.filter(id => !pendingStopAcknowledgements.has(id)),
      ...pendingStopAcknowledgements].slice(-MAX_STOP_RECEIPTS);
    if (!writeStopQueue(queue)) return false;
    for (const id of pendingStopAcknowledgements) claimedStops.delete(id);
    pendingStopAcknowledgements.clear();
    return true;
  } catch { return false; }
}
function guardedStopId(lease: RecordingLease): string {
  return JSON.stringify([lease.generation, lease.owner?.source, lease.owner?.session_id, lease.owner?.app, lease.owner?.process]);
}
/** Unbound stored manual markers cannot safely acquire a later take. Only a
 * fresh manual action may resolve its target, once, before waiting for Controls.
 */
export function readRecordingStopRequest(payload: unknown): RecordingStopRequest | null {
  try {
    const value = typeof payload === 'string' ? JSON.parse(payload) : payload;
    if (value?.kind === 'manual') {
      if (typeof value.id !== 'string' || !value.id || value.id.length > 256 || value.expected !== undefined) return null;
      if (value.state === 'ready' && typeof value.generation === 'string' && value.generation && value.generation.length <= 256) return value;
      if (value.state === 'resolving' && value.generation === null) return value;
      return null;
    }
    const lease = validRecordingLease(value?.expected) ? value.expected : validRecordingLease(value) ? value : null;
    if (lease) return { kind: 'automated', id: guardedStopId(lease), expected: lease };
  } catch { /* Malformed automation never becomes manual intent. */ }
  return null;
}
function readStopQueue(): StopQueue {
  const raw = sessionStorage.getItem(STOP_REQUEST_KEY);
  const empty: StopQueue = { version: 2, requests: [], done: [] };
  if (!raw) return empty;
  try {
    const value = JSON.parse(raw);
    if (value?.version === 2) {
      if (!Array.isArray(value.requests) || value.requests.length > MAX_STOP_REQUESTS
        || !Array.isArray(value.done) || value.done.length > MAX_STOP_RECEIPTS
        || value.done.some((id: unknown) => typeof id !== 'string')) return empty;
      const requests = value.requests.map(readRecordingStopRequest).filter((request: RecordingStopRequest | null): request is RecordingStopRequest =>
        !!request && (request.kind !== 'manual' || request.state !== 'resolving' || resolvingStops.has(request.id)));
      return { version: 2, requests, done: value.done };
    }
  } catch { /* Legacy valid guarded routes retain their exact lease. */ }
  const legacy = readRecordingStopRequest(raw);
  return legacy ? { ...empty, requests: [legacy] } : empty;
}
function writeStopQueue(queue: StopQueue): boolean {
  try { sessionStorage.setItem(STOP_REQUEST_KEY, JSON.stringify(queue)); return true; }
  catch { return false; }
}
function notifyStopQueue() {
  window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT, { detail: { kind: 'queued' } }));
}
/** Distinct user actions coexist with automation. Only the SAME intent/guarded
 * lease deduplicates. Capacity/storage failure cannot create an unbound Stop.
 */
export function queueRecordingStopRequest(request: RecordingStopRequest): RecordingStopRequest | null {
  if (!acknowledgeCompletedStops()) return null;
  try {
    const queue = readStopQueue();
    if (queue.done.includes(request.id)) return null;
    const pending = queue.requests.find(item => item.id === request.id);
    if (pending) return pending;
    if (queue.requests.length >= MAX_STOP_REQUESTS) return null;
    queue.requests.push(request);
    return writeStopQueue(queue) ? request : null;
  } catch { return null; }
}
async function bindManualStop(request: Extract<RecordingStopRequest, { state: 'resolving' }>) {
  try {
    // Freeze the native generation at the action, not after route/state sync or
    // an awaited directory lookup. A lost unresolved binding is never retried
    // against the next take on WebView reload.
    const native = await invoke<NativeRecordingState>('get_recording_state');
    if (!native.is_recording || native.is_stopping || !validRecordingLease(native.recording_lease)) {
      completeRecordingStopRequest(request); return;
    }
    const queue = readStopQueue();
    const index = queue.requests.findIndex(item => item.id === request.id);
    if (index < 0 || queue.done.includes(request.id)) return;
    queue.requests[index] = { kind: 'manual', id: request.id, state: 'ready', generation: native.recording_lease.generation };
    if (!writeStopQueue(queue)) completeRecordingStopRequest(request);
  } catch (error) {
    console.error('Could not bind manual Stop to its native take:', error);
    completeRecordingStopRequest(request);
  } finally { resolvingStops.delete(request.id); notifyStopQueue(); }
}
/** Guarded routing stays exact. A fresh explicit manual action has its own ID
 * and resolves once even when Controls' initial isRecording is still false.
 */
export function requestRecordingStop(navigate: (href: string) => void, expected?: RecordingLease): string | null {
  let request: RecordingStopRequest;
  if (arguments.length > 1) {
    if (!validRecordingLease(expected)) return null;
    request = { kind: 'automated', id: guardedStopId(expected), expected };
  } else {
    request = { kind: 'manual', id: crypto.randomUUID(), state: 'resolving', generation: null };
    resolvingStops.add(request.id);
  }
  const queued = queueRecordingStopRequest(request);
  if (!queued) { resolvingStops.delete(request.id); return null; }
  if (request.kind === 'manual' && request.state === 'resolving') void bindManualStop(request);
  if (window.location.pathname === '/') {
    // Retain the legacy guarded event shape for source adapters/diagnostics.
    if (request.kind === 'automated') window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT, { detail: request.expected }));
    else notifyStopQueue();
  } else navigate('/');
  return request.id;
}
/** Claims remain durable until completion and module-owned across remount.
 * Replaying a bound request after WebView loss is safe at native generation
 * claim; an unresolved manual binding is dropped rather than rebound.
 */
export function takeRecordingStopRequest(): RecordingStopRequest | null {
  if (!acknowledgeCompletedStops()) return null;
  try {
    const queue = readStopQueue();
    const request = queue.requests.find(item => !queue.done.includes(item.id) && !claimedStops.has(item.id)
      && (item.kind !== 'manual' || item.state === 'ready'));
    if (!request) return null;
    claimedStops.add(request.id);
    return request;
  } catch { return null; }
}
export function completeRecordingStopRequest(request: RecordingStopRequest) {
  // Retain the claim until durable acknowledgement succeeds. Drain/wakeup can
  // retry only this write, never directory lookup/native Stop or target binding.
  // Fail closed at the queue's capacity even for unexpected external callers.
  if (pendingStopAcknowledgements.size >= MAX_STOP_REQUESTS && !pendingStopAcknowledgements.has(request.id)) return;
  pendingStopAcknowledgements.add(request.id);
  acknowledgeCompletedStops();
}
export function validRecordingLease(value: unknown): value is RecordingLease {
  if (!value || typeof value !== 'object') return false;
  const lease = value as RecordingLease;
  return typeof lease.generation === 'string' && !!lease.generation && (lease.owner === null
    || (!!lease.owner && typeof lease.owner.source === 'string' && typeof lease.owner.session_id === 'string'
      && typeof lease.owner.app === 'string' && typeof lease.owner.process === 'string'));
}
