/**
 * Labs meeting automation. When meeting detection confirms a call,
 * a recording starts; when that call ends, the
 * recording it started stops and saves. Recordings the user started are
 * never stopped by it.
 *
 * The recorder owns the start and stop sequences, so the hand-off between the
 * detection events (in the root layout) and the recorder goes through
 * sessionStorage, which survives the page change to the recorder.
 */

const PENDING_KEY = 'labsAutoStartPending';
const ACTIVE_KEY = 'labsAutoRecordingProcess';
export const AUTOMATION_CHANGED_EVENT = 'meetily-automation-changed';

/** A start request older than this was not picked up and is ignored. */
const PENDING_TTL_MS = 60_000;

export interface AutomatedCall {
  /** Friendly name, e.g. "Zoom". */
  app: string;
  /** Process the detector matched, e.g. "Zoom.exe". */
  process: string;
  /** Native detector session; distinguishes successive calls in the same app. */
  session_id?: number;
}

function read(key: string): string | null {
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeKey(key: string, value: string | null) {
  try {
    if (value === null) sessionStorage.removeItem(key);
    else sessionStorage.setItem(key, value);
  } catch {
    // Without storage, automation still starts recordings but cannot stop them.
  }
}

function parse(value: string | null): AutomatedCall | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<AutomatedCall>;
    if (typeof parsed.process === 'string' && parsed.process) {
      return {
        app: typeof parsed.app === 'string' && parsed.app ? parsed.app : parsed.process,
        process: parsed.process,
        ...(typeof parsed.session_id === 'number' ? { session_id: parsed.session_id } : {}),
      };
    }
  } catch {
    // Written by #38 as the bare process name.
    return { app: value, process: value };
  }
  return null;
}

/** Legacy ownership only matches legacy events; never guess a newer call's owner. */
export function sameAutomatedCall(left: AutomatedCall, right: AutomatedCall): boolean {
  return left.process === right.process && left.session_id === right.session_id;
}

/** Kept above effect lifetimes so routing cannot replay a prompt or race a start. */
export function createMeetingDetectionSessions() {
  const latest = new Map<string, number>();
  const active = new Map<string, AutomatedCall>();
  let starting: { token: symbol; expires: number } | null = null;
  const isActive = (call: AutomatedCall) => {
    const current = active.get(call.process);
    return !!current && sameAutomatedCall(current, call);
  };
  return {
    detect(call: AutomatedCall): boolean {
      if (call.session_id === undefined || (latest.get(call.process) ?? -1) >= call.session_id) return false;
      latest.set(call.process, call.session_id);
      active.set(call.process, call);
      return true;
    },
    end(call: AutomatedCall) {
      if (call.session_id !== undefined) {
        latest.set(call.process, Math.max(latest.get(call.process) ?? -1, call.session_id));
      }
      if (isActive(call)) active.delete(call.process);
    },
    isActive,
    claimStart(call: AutomatedCall): symbol | null {
      if (!isActive(call) || (starting && starting.expires > Date.now())) return null;
      const token = Symbol('meeting-start');
      // The recorder prepares models asynchronously. Keep the reservation until
      // native capture starts; a bounded lease allows retry after a failed setup.
      starting = { token, expires: Date.now() + PENDING_TTL_MS };
      return token;
    },
    canStart(call: AutomatedCall, token: symbol): boolean {
      return isActive(call) && starting?.token === token;
    },
    releaseStart(token: symbol) {
      if (starting?.token === token) starting = null;
    },
    recordingStarted() { starting = null; },
  };
}

/** Reserve before awaiting native calls; recheck session and UI lifetime after each. */
export async function startDetectedMeeting(
  call: AutomatedCall,
  sessions: ReturnType<typeof createMeetingDetectionSessions>,
  actions: {
    getRecordingState: () => Promise<{ is_recording?: boolean }>;
    validateSession: () => Promise<boolean>;
    enabled: () => boolean;
    launch: () => void;
  },
): Promise<'started' | 'skipped' | 'unconfirmed'> {
  const reservation = sessions.claimStart(call);
  if (!reservation) return 'skipped';
  let launched = false;
  const current = () => actions.enabled() && sessions.canStart(call, reservation);
  try {
    const state = await actions.getRecordingState();
    if (state.is_recording || !current()) return 'skipped';
    const valid = await actions.validateSession();
    if (!current()) return 'skipped';
    if (!valid) return 'unconfirmed';
    actions.launch();
    launched = true;
    return 'started';
  } finally {
    // Keep the reservation through model loading, until recording-started.
    if (!launched) sessions.releaseStart(reservation);
  }
}

/** Called right before the recorder is asked to start for a detected call. */
export function markAutomatedStart(call: AutomatedCall) {
  writeKey(PENDING_KEY, JSON.stringify({ ...call, at: Date.now() }));
}

/** Distinguish expired detection requests from ordinary manual starts. */
export function hasPendingAutomatedStart(): boolean {
  return read(PENDING_KEY) !== null;
}

/**
 * The recorder takes the pending request when a start begins. It is cleared
 * whatever happens, so a later manual start is never mistaken for it.
 */
export function takeAutomatedStart(): AutomatedCall | null {
  const value = read(PENDING_KEY);
  writeKey(PENDING_KEY, null);
  if (!value) return null;
  try {
    const at = Number((JSON.parse(value) as { at?: number }).at ?? 0);
    if (!at || Date.now() - at > PENDING_TTL_MS) return null;
  } catch {
    return null;
  }
  return parse(value);
}

/** The recording now running was started for this call. */
export function beginAutomatedRecording(call: AutomatedCall) {
  writeKey(ACTIVE_KEY, JSON.stringify(call));
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}

/** The call the running recording stops with, if automation started it. */
export function automatedRecording(): AutomatedCall | null {
  return parse(read(ACTIVE_KEY));
}

/** Recording stopped, or the user chose to keep it going after the call. */
export function endAutomatedRecording() {
  if (read(ACTIVE_KEY) === null && read(PENDING_KEY) === null) return;
  writeKey(ACTIVE_KEY, null);
  writeKey(PENDING_KEY, null);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}
