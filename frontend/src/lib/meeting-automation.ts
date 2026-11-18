/**
 * Labs automation handoff shared by source lifecycle adapters and legacy
 * meeting offers. Only an automation-owned take follows its detected call;
 * manually started recordings remain under user control.
 *
 * The recorder owns the start and stop sequences, so the hand-off between the
 * detection events (in the root layout) and the recorder goes through
 * sessionStorage, which survives the page change to the recorder.
 */

const PENDING_KEY = 'labsAutoStartPending';
const STARTING_KEY = 'labsAutoStartInFlight';
const ACTIVE_KEY = 'labsAutoRecordingProcess';
export const AUTOMATION_CHANGED_EVENT = 'meetily-automation-changed';

/** A start request older than this was not picked up and is ignored. */
const PENDING_TTL_MS = 60_000;

export interface AutomatedCall {
  /** Friendly name, e.g. "Zoom". */
  app: string;
  /** Process the detector matched, e.g. "Zoom.exe". */
  process: string;
  /** Lifecycle adapter + native call identity, independent of PID/device/mute. */
  source?: string;
  sessionId?: string;
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
    // Callers can fail closed by checking the handoff write before routing.
  }
}

function parse(value: string | null): AutomatedCall | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(value) as Partial<AutomatedCall>;
    if (typeof parsed.process === 'string' && parsed.process) {
      return { app: typeof parsed.app === 'string' && parsed.app ? parsed.app : parsed.process, process: parsed.process,
        ...(typeof parsed.source === 'string' ? { source: parsed.source } : {}),
        ...(typeof parsed.sessionId === 'string' ? { sessionId: parsed.sessionId } : {}),
      };
    }
  } catch {
    // Written by #38 as the bare process name.
    return { app: value, process: value };
  }
  return null;
}

/** Called right before the recorder is asked to start for a detected call. */
export function markAutomatedStart(call: AutomatedCall) {
  const value = JSON.stringify({ ...call, at: Date.now() });
  writeKey(PENDING_KEY, value);
  return read(PENDING_KEY) === value;
}

export function automatedStartInProgress(): AutomatedCall | null {
  return parse(read(PENDING_KEY)) || parse(read(STARTING_KEY));
}

export function releaseAutomatedStart() { writeKey(STARTING_KEY, null); }

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
  const call = parse(value);
  if (call) writeKey(STARTING_KEY, JSON.stringify(call));
  return call;
}

/** The recording now running was started for this call. */
export function beginAutomatedRecording(call: AutomatedCall) {
  releaseAutomatedStart();
  writeKey(ACTIVE_KEY, JSON.stringify(call));
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}

/** The call the running recording stops with, if automation started it. */
export function automatedRecording(): AutomatedCall | null {
  return parse(read(ACTIVE_KEY));
}

/** Recording stopped, or the user chose to keep it going after the call. */
export function endAutomatedRecording() {
  if (read(ACTIVE_KEY) === null && read(PENDING_KEY) === null && read(STARTING_KEY) === null) return;
  releaseAutomatedStart();
  writeKey(ACTIVE_KEY, null);
  writeKey(PENDING_KEY, null);
  window.dispatchEvent(new Event(AUTOMATION_CHANGED_EVENT));
}
