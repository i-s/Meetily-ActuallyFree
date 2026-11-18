/** One common recorder reservation across independent client adapters. Native
 * recording_lease owns an actual take; this token owns only its JS handoff.
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
  recordingLease?: import('./native-recording').RecordingLease;
}
function read(key: string): string | null { try { return sessionStorage.getItem(key); } catch { return null; } }
function write(key: string, value: string | null): boolean {
  try {
    if (value === null) sessionStorage.removeItem(key); else sessionStorage.setItem(key, value);
    return read(key) === value;
  } catch { return false; }
}
function parse(value: string | null): AutomatedCall | null {
  if (!value) return null;
  try {
    const { at: _at, ...call } = JSON.parse(value);
    return typeof call.process === 'string' && call.process ? call : null;
  } catch { return { app: value, process: value }; } // historical bare process
}
function matches(a: AutomatedCall | null, b: AutomatedCall): boolean {
  return !!a?.token && a.token === b.token && a.source === b.source && a.sessionId === b.sessionId;
}
export function reservationCurrent(call: AutomatedCall, phase: 'pending' | 'inflight'): boolean {
  return matches(parse(read(phase === 'pending' ? PENDING_KEY : STARTING_KEY)), call);
}
export function automatedStartInProgress(): AutomatedCall | null {
  return parse(read(PENDING_KEY)) || parse(read(STARTING_KEY));
}
/** Synchronous admission: no adapter priority and no await between claim/write.
 * The caller retains the generated opaque token on this same expectedCall.
 */
export function markAutomatedStart(call: AutomatedCall): boolean {
  if (automatedStartInProgress()) return false;
  call.token = crypto.randomUUID();
  return write(PENDING_KEY, JSON.stringify({ ...call, at: Date.now() }));
}
export function takeAutomatedStart(): AutomatedCall | null {
  const value = read(PENDING_KEY);
  const call = parse(value);
  if (!call?.token || !value) {
    if (value && read(PENDING_KEY) === value) write(PENDING_KEY, null);
    return null;
  }
  try {
    const at = JSON.parse(value).at;
    if (typeof at !== 'number' || Date.now() - at > PENDING_TTL_MS) {
      if (reservationCurrent(call, 'pending')) write(PENDING_KEY, null);
      return null;
    }
  } catch { return null; }
  if (read(STARTING_KEY) || !write(STARTING_KEY, value)) return null;
  if (reservationCurrent(call, 'pending')) write(PENDING_KEY, null);
  return call;
}
export function releaseAutomatedStart(expectedCall: AutomatedCall): void {
  for (const phase of ['pending', 'inflight'] as const) {
    if (reservationCurrent(expectedCall, phase)) write(phase === 'pending' ? PENDING_KEY : STARTING_KEY, null);
  }
}
export function beginAutomatedRecording(expectedCall: AutomatedCall): boolean {
  if (!reservationCurrent(expectedCall, 'inflight')) return false;
  if (!write(ACTIVE_KEY, JSON.stringify(expectedCall))) return false;
  releaseAutomatedStart(expectedCall);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
  return true;
}
/** Only callers that just queried native recording authority may restore this
 * display mirror. It never admits a start and cannot authorize native Stop.
 */
export function reconcileAutomatedRecording(call: AutomatedCall | null): void {
  const value = call ? JSON.stringify(call) : null;
  if (read(ACTIVE_KEY) === value) return;
  write(ACTIVE_KEY, value);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}
export function automatedRecording(): AutomatedCall | null { return parse(read(ACTIVE_KEY)); }
export function endAutomatedRecording(): void { reconcileAutomatedRecording(null); }
