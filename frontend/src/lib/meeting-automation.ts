/** Single main-WebView reservation shared by every automation source.
 * Native recording leases, not this storage, are final start/stop authority.
 */
const PENDING_KEY = 'labsAutoStartPending';
const STARTING_KEY = 'labsAutoStartInFlight';
const ACTIVE_KEY = 'labsAutoRecordingProcess';
export const AUTOMATION_CHANGED_EVENT = 'meetily-automation-changed';
const PENDING_TTL_MS = 60_000;

export interface AutomatedCall {
  app: string;
  process: string;
  source?: string;
  sessionId?: string;
  token?: string;
  requestedAt?: number;
}
export type ReservationPhase = 'pending' | 'inflight' | 'recording';
function read(key: string): string | null {
  try { return sessionStorage.getItem(key); } catch { return null; }
}
function write(key: string, value: string | null): boolean {
  try {
    if (value === null) sessionStorage.removeItem(key); else sessionStorage.setItem(key, value);
    return sessionStorage.getItem(key) === value;
  } catch { return false; }
}
function parse(value: string | null): AutomatedCall | null {
  if (!value) return null;
  try {
    const call = JSON.parse(value) as AutomatedCall;
    if (typeof call.process !== 'string' || !call.process) return { app: 'Invalid automation request', process: 'invalid-automation-request' };
    return { app: typeof call.app === 'string' && call.app ? call.app : call.process, process: call.process,
      ...(typeof call.source === 'string' ? { source: call.source } : {}),
      ...(typeof call.sessionId === 'string' ? { sessionId: call.sessionId } : {}),
      ...(typeof call.token === 'string' ? { token: call.token } : {}),
      ...(typeof call.requestedAt === 'number' ? { requestedAt: call.requestedAt } : {}),
    };
  } catch { return { app: value, process: value }; } // legacy active owner only
}
export function sameAutomationReservation(a: AutomatedCall | null, b: AutomatedCall | null): boolean {
  return !!a?.token && !!b?.token && a.token === b.token && a.source === b.source &&
    a.sessionId === b.sessionId && a.process === b.process;
}
function phaseKey(phase: ReservationPhase) {
  return phase === 'pending' ? PENDING_KEY : phase === 'inflight' ? STARTING_KEY : ACTIVE_KEY;
}
export function reservationCurrent(call: AutomatedCall, phase: ReservationPhase): boolean {
  const current = parse(read(phaseKey(phase)));
  if (!sameAutomationReservation(current, call)) return false;
  if (phase === 'recording') return true;
  const age = Date.now() - (current?.requestedAt ?? 0);
  return !!current?.requestedAt && age >= 0 && age <= PENDING_TTL_MS;
}
export function automatedStartInProgress(): AutomatedCall | null {
  return parse(read(PENDING_KEY)) || parse(read(STARTING_KEY));
}
/** No caller can replace another source's pending or in-flight request. */
export function markAutomatedStart(call: AutomatedCall): boolean {
  if (read(PENDING_KEY) !== null || read(STARTING_KEY) !== null || automatedRecording()) return false;
  const token = globalThis.crypto?.randomUUID?.();
  if (!token) return false;
  const reserved = { ...call, token, requestedAt: Date.now() };
  return write(PENDING_KEY, JSON.stringify(reserved)) && reservationCurrent(reserved, 'pending');
}
/** Transfer, never recreate, the accepted token. A stale request cannot become
 * an unguarded manual start: recorder checks whether a request existed first.
 */
export function takeAutomatedStart(): AutomatedCall | null {
  const value = read(PENDING_KEY);
  const call = parse(value);
  if (!call) return null;
  if (read(STARTING_KEY) || !reservationCurrent(call, 'pending')) {
    if (read(PENDING_KEY) === value) write(PENDING_KEY, null);
    return null;
  }
  if (!write(STARTING_KEY, JSON.stringify(call))) return null;
  if (!sameAutomationReservation(parse(read(PENDING_KEY)), call) || !write(PENDING_KEY, null)) {
    releaseAutomatedStart(call); return null;
  }
  if (!reservationCurrent(call, 'inflight')) { releaseAutomatedStart(call); return null; }
  return call;
}
/** A late finally from A must not clear B. No unconditional automation release. */
export function releaseAutomatedStart(expectedCall: AutomatedCall): void {
  for (const key of [PENDING_KEY, STARTING_KEY]) {
    if (sameAutomationReservation(parse(read(key)), expectedCall)) write(key, null);
  }
}
export function beginAutomatedRecording(expectedCall: AutomatedCall): boolean {
  if (!reservationCurrent(expectedCall, 'inflight') || automatedRecording()) return false;
  if (!write(ACTIVE_KEY, JSON.stringify(expectedCall))) return false;
  releaseAutomatedStart(expectedCall);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
  return reservationCurrent(expectedCall, 'recording');
}
export function automatedRecording(): AutomatedCall | null { return parse(read(ACTIVE_KEY)); }
/** Explicit manual Stop/Don't stop it revokes all local reservations. Native
 * release/stop must separately validate the exact generation when integrated.
 */
export function endAutomatedRecording(expectedCall?: AutomatedCall) {
  if (expectedCall) {
    if (reservationCurrent(expectedCall, 'recording') && write(ACTIVE_KEY, null)) {
      window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
    }
    return;
  }
  if ([ACTIVE_KEY, PENDING_KEY, STARTING_KEY].every((key) => read(key) === null)) return;
  for (const key of [ACTIVE_KEY, PENDING_KEY, STARTING_KEY]) write(key, null);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}
