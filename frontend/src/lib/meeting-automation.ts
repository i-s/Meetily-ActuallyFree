/** Single main-WebView reservation shared by every automation source.
 * Native recording leases and settling barriers, not storage/TTL, fence capture.
 */
import { isStartCapability, isStartRequest, isStartResolution, sameStartRequest,
  isRecordingLease, type StartCapability, type StartRequest, type RecordingLease } from './recording-start-authority';
const PENDING_KEY = 'labsAutoStartPending';
const STARTING_KEY = 'labsAutoStartInFlight';
const ACTIVE_KEY = 'labsAutoRecordingProcess';
const CONSUMED_KEY = 'labsAutoStartConsumed';
export const AUTOMATION_CHANGED_EVENT = 'meetily-automation-changed';
const PENDING_TTL_MS = 60_000;
const MAX_CONSUMED = 256; // On exhaustion fail closed, never evict and rearm an old session.

export interface AutomatedCall {
  app: string;
  process: string;
  source?: string;
  sessionId?: string;
  token?: string;
  requestedAt?: number;
  recordingLease?: RecordingLease;
}
interface StoredCall extends AutomatedCall {
  startStage?: 'preparing' | 'native';
  expectedStart?: StartRequest;
  revoked?: boolean;
}
export type ReservationPhase = 'pending' | 'inflight' | 'recording';
export type StartTakeResult = { kind: 'absent' } | { kind: 'rejected'; reason: string } | { kind: 'accepted'; call: AutomatedCall };
export type AutomatedStartState = { kind: 'absent' } | { kind: 'rejected'; reason: string } |
  { kind: 'pending' | 'preparing' | 'recording'; call: AutomatedCall } |
  { kind: 'native_unresolved'; call: AutomatedCall; expected: StartRequest | null };
function read(key: string): string | null {
  try { return sessionStorage.getItem(key); } catch { return null; }
}
function storageAvailable(): boolean {
  try { sessionStorage.getItem(PENDING_KEY); return true; } catch { return false; }
}
function write(key: string, value: string | null): boolean {
  try {
    if (value === null) sessionStorage.removeItem(key); else sessionStorage.setItem(key, value);
    return sessionStorage.getItem(key) === value;
  } catch { return false; }
}
export function isAutomatedCall(value: unknown): value is AutomatedCall {
  if (!value || typeof value !== 'object') return false;
  const v = value as AutomatedCall;
  return [v.app, v.process, v.token].every((s) => typeof s === 'string' && !!s) &&
    typeof v.requestedAt === 'number' && Number.isFinite(v.requestedAt) && v.requestedAt >= 0;
}
function parse(value: string | null): StoredCall | null {
  if (value === null) return null;
  try {
    const call: unknown = JSON.parse(value);
    return isAutomatedCall(call) ? call as StoredCall : null;
  } catch { return null; }
}
function live(call: AutomatedCall): boolean {
  const age = Date.now() - (call.requestedAt ?? NaN);
  return age >= 0 && age <= PENDING_TTL_MS;
}
export function sameAutomationReservation(a: AutomatedCall | null, b: AutomatedCall | null): boolean {
  return !!a?.token && !!b?.token && a.token === b.token && a.source === b.source &&
    a.sessionId === b.sessionId && a.process === b.process && a.app === b.app;
}
function phaseKey(phase: ReservationPhase) {
  return phase === 'pending' ? PENDING_KEY : phase === 'inflight' ? STARTING_KEY : ACTIVE_KEY;
}
export function reservationCurrent(call: AutomatedCall, phase: ReservationPhase): boolean {
  const current = parse(read(phaseKey(phase)));
  return sameAutomationReservation(current, call) && (phase === 'recording' || (!!current && !current.revoked && live(current)));
}
/** Only explicitly undispatched preparation may be retired on clock expiry.
 * Old unstaged inflight records may already have invoked native code: quarantine.
 */
function retireExpiredPreparation(): void {
  for (const key of [PENDING_KEY, STARTING_KEY]) {
    const raw = read(key), call = parse(raw);
    if (call && !live(call) && (key === PENDING_KEY || call.startStage === 'preparing') && read(key) === raw) write(key, null);
  }
}
export function automatedStartState(): AutomatedStartState {
  if (!storageAvailable()) return { kind: 'rejected', reason: 'storage_unavailable' };
  retireExpiredPreparation();
  const starting = read(STARTING_KEY), call = parse(starting);
  if (starting !== null) {
    if (!call) return { kind: 'rejected', reason: 'malformed_inflight_unresolved' };
    if (call.startStage !== 'preparing') return { kind: 'native_unresolved', call, expected: isStartRequest(call.expectedStart) ? { ...call.expectedStart } : null };
    return { kind: 'preparing', call };
  }
  const pending = read(PENDING_KEY), next = parse(pending);
  if (pending !== null) return next ? { kind: 'pending', call: next } : { kind: 'rejected', reason: 'malformed_pending' };
  const active = automatedRecording();
  if (active) return { kind: 'recording', call: active };
  const unavailable = consumedUnavailable();
  return unavailable ? { kind: 'rejected', reason: unavailable } : { kind: 'absent' };
}
/** Reports live local work, not stale authority. Inspect automatedStartState for
 * native_unresolved even when this returns null; null is NOT admission proof.
 */
export function automatedStartInProgress(): AutomatedCall | null {
  const state = automatedStartState();
  return 'call' in state && state.kind !== 'recording' && live(state.call) && !automatedRecording() ? state.call : null;
}
function sessionKey(call: AutomatedCall): string {
  return JSON.stringify([call.source || call.app, call.sessionId || call.process]);
}
function consumedUnavailable(): string | null {
  try {
    const raw = read(CONSUMED_KEY), value: unknown = raw === null ? [] : JSON.parse(raw);
    if (!Array.isArray(value) || value.some((v) => typeof v !== 'string')) return 'consumed_history_invalid';
    return value.length >= MAX_CONSUMED ? 'consumed_capacity_exhausted' : null;
  } catch { return 'consumed_history_invalid'; }
}
/** Visible unavailability, not a reset/qualification override. Recovery requires
 * ending calls and a clean app/WebView session, never deleting a live fence.
 */
export function automationUnavailableMessage(state = automatedStartState()): string | null {
  if (state.kind === 'native_unresolved') return 'Automatic recording unavailable: a native start is unresolved or the recorder is busy. Wait for native settlement; manual controls remain available.';
  if (state.kind !== 'rejected') return null;
  if (state.reason === 'consumed_capacity_exhausted') return 'Automatic recording unavailable: the 256-session safety history is full. Stop and save recordings, end all calls, fully quit and reopen Meetily, then join a new call. Reloading or toggling Labs cannot reset it. Manual controls remain available.';
  return 'Automatic recording unavailable: saved automation authority cannot be verified. Use manual controls; do not clear live reservations to retry.';
}
function consume(call: AutomatedCall): boolean {
  try {
    const raw = read(CONSUMED_KEY);
    const consumed: unknown = raw === null ? [] : JSON.parse(raw);
    if (!Array.isArray(consumed) || consumed.some((v) => typeof v !== 'string') || consumed.length >= MAX_CONSUMED) return false;
    const key = sessionKey(call);
    if (consumed.includes(key)) return false;
    const saved = write(CONSUMED_KEY, JSON.stringify([...consumed, key]));
    if (saved && consumed.length + 1 === MAX_CONSUMED) window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
    return saved;
  } catch { return false; }
}
/** Busy sessions are consumed, not queued. No TTL/manual Stop clears tombstones. */
export function markAutomatedStart(call: AutomatedCall): boolean {
  if (!storageAvailable() || !call.app || !call.process) return false;
  const state = automatedStartState();
  if (!consume(call)) return false;
  if (state.kind !== 'absent') return false;
  const token = globalThis.crypto?.randomUUID?.();
  if (!token) return false;
  const reserved: StoredCall = { app: call.app, process: call.process, source: call.source, sessionId: call.sessionId,
    token, requestedAt: Date.now(), startStage: 'preparing' };
  return write(PENDING_KEY, JSON.stringify(reserved)) && reservationCurrent(reserved, 'pending');
}
/** Absence is the ONLY outcome that a caller may treat as a manual launch.
 * Invalid/expired automatic payloads remain rejected, including after reload.
 */
export function takeAutomatedStartResult(expectedCall?: AutomatedCall): StartTakeResult {
  if (!storageAvailable()) return { kind: 'rejected', reason: 'storage_unavailable' };
  const value = read(PENDING_KEY), call = parse(value);
  if (value === null) {
    if (expectedCall) return { kind: 'rejected', reason: 'automated_reservation_missing' };
    return read(STARTING_KEY) !== null || read(ACTIVE_KEY) !== null ? { kind: 'rejected', reason: 'owner_busy_or_unresolved' } : { kind: 'absent' };
  }
  if (!call) return { kind: 'rejected', reason: 'malformed_pending' };
  if (expectedCall && !sameAutomationReservation(call, expectedCall)) return { kind: 'rejected', reason: 'reservation_replaced' };
  if (!reservationCurrent(call, 'pending')) {
    retireExpiredPreparation(); return { kind: 'rejected', reason: 'expired_pending' };
  }
  if (read(STARTING_KEY) !== null || read(ACTIVE_KEY) !== null) return { kind: 'rejected', reason: 'owner_busy_or_unresolved' };
  const preparing: StoredCall = { ...call, startStage: 'preparing' };
  if (!write(STARTING_KEY, JSON.stringify(preparing))) return { kind: 'rejected', reason: 'storage_unavailable' };
  if (!sameAutomationReservation(parse(read(PENDING_KEY)), call) || !write(PENDING_KEY, null)) {
    releaseAutomatedStart(call); return { kind: 'rejected', reason: 'reservation_replaced' };
  }
  return { kind: 'accepted', call: preparing };
}
/** Compatibility only: recorder/route consumers must use the discriminated result. */
export function takeAutomatedStart(): AutomatedCall | null {
  const result = takeAutomatedStartResult(); return result.kind === 'accepted' ? result.call : null;
}
/** Persist the entire exact native request BEFORE invoke. Reading capability is
 * not admission. Never mint a new sequence/token while reconciling an old request.
 */
export function freezeAutomatedStart(call: AutomatedCall, capability: StartCapability): StartRequest | null {
  const stored = parse(read(STARTING_KEY));
  if (!isStartCapability(capability) || !stored || stored.startStage !== 'preparing' ||
      !reservationCurrent(call, 'inflight') || !call.source || !call.sessionId) return null;
  const expected: StartRequest = { version: 1, epoch: capability.epoch, sequence: capability.next_sequence,
    source: call.source, session_id: call.sessionId, token: call.token! };
  if (!write(STARTING_KEY, JSON.stringify({ ...stored, startStage: 'native', expectedStart: expected }))) return null;
  return Object.freeze({ ...expected });
}
/** Late finally may cancel local preparation, never a possibly dispatched start. */
export function releaseAutomatedStart(expectedCall: AutomatedCall): void {
  for (const key of [PENDING_KEY, STARTING_KEY]) {
    const stored = parse(read(key));
    if (!sameAutomationReservation(stored, expectedCall)) continue;
    if (key === STARTING_KEY && stored?.startStage !== 'preparing') {
      if (!reservationCurrent(expectedCall, 'recording')) write(key, JSON.stringify({ ...stored, revoked: true }));
    } else write(key, null);
  }
}
function leaseMatchesCall(lease: unknown, call: AutomatedCall): lease is RecordingLease {
  return isRecordingLease(lease) && lease.owner?.source === call.source && lease.owner?.session_id === call.sessionId &&
    lease.owner?.app === call.app && lease.owner?.process === call.process;
}
export function beginAutomatedRecording(expectedCall: AutomatedCall, lease?: RecordingLease): boolean {
  const stored = parse(read(STARTING_KEY));
  if (!stored || stored.startStage !== 'native' || !isStartRequest(stored.expectedStart) ||
      stored.expectedStart.token !== stored.token || stored.expectedStart.source !== stored.source ||
      stored.expectedStart.session_id !== stored.sessionId ||
      !reservationCurrent(expectedCall, 'inflight') || automatedRecording() || !leaseMatchesCall(lease, expectedCall)) return false;
  if (!write(ACTIVE_KEY, JSON.stringify({ ...expectedCall, recordingLease: lease }))) return false;
  // Keep native fence even after promotion: manual ownership release does not
  // establish idle and cannot allow B to race an unobserved native take.
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
  return reservationCurrent(expectedCall, 'recording');
}
/** A terminal receipt fences future commit, not necessarily an idle recorder.
 * Retired may have committed historically. Never derive idle from its outcome.
 */
export function applyAutomatedStartResolution(call: AutomatedCall, expected: StartRequest, resolution: unknown): 'unresolved' | 'settled_idle' | 'settled_busy' {
  const stored = parse(read(STARTING_KEY));
  if (!stored || stored.startStage !== 'native' || !sameAutomationReservation(stored, call) ||
      !sameStartRequest(stored.expectedStart, expected) || !isStartResolution(resolution, expected)) return 'unresolved';
  if (resolution.is_recording || resolution.is_stopping) return 'settled_busy';
  if (!write(STARTING_KEY, null)) return 'unresolved';
  if (reservationCurrent(call, 'recording')) {
    if (!write(ACTIVE_KEY, null)) return 'unresolved';
    window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
  }
  return 'settled_idle';
}
export function automatedRecording(): AutomatedCall | null {
  const value = read(ACTIVE_KEY);
  if (value === null) return null;
  const call = parse(value);
  return call || { app: value, process: value }; // Legacy display only; blocks admission, never has token authority.
}
/** Manual control revokes local intent, never erases dispatched/legacy native
 * uncertainty. Cancel/resolve must still settle it; cancel never stops a take.
 */
export function endAutomatedRecording(expectedCall?: AutomatedCall) {
  if (expectedCall) {
    if (reservationCurrent(expectedCall, 'recording') && write(ACTIVE_KEY, null)) {
      releaseAutomatedStart(expectedCall);
      window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
    }
    return;
  }
  if ([ACTIVE_KEY, PENDING_KEY, STARTING_KEY].every((key) => read(key) === null)) return;
  write(ACTIVE_KEY, null);
  write(PENDING_KEY, null);
  const starting = parse(read(STARTING_KEY));
  if (starting?.startStage === 'preparing') write(STARTING_KEY, null);
  else if (starting) write(STARTING_KEY, JSON.stringify({ ...starting, revoked: true }));
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}
