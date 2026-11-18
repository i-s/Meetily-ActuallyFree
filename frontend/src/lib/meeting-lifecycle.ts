import { automatedStartInProgress, automatedRecording, endAutomatedRecording, markAutomatedStart, type AutomatedCall } from './meeting-automation';

export interface LifecycleSource {
  source: string;
  app: string;
  process: string;
  presence: 'active' | 'inactive' | 'unknown';
  reason: string;
  /** Retained through Unknown and stop grace; null only after confirmed exit. */
  session_id: string | null;
}
export interface LifecycleSnapshot {
  revision: number;
  enabled: boolean;
  sources: LifecycleSource[];
}
let latest: LifecycleSnapshot = { revision: -1, enabled: false, sources: [] };
const SEEN_KEY = 'meetingLifecycleConsumed';
function identity(call: AutomatedCall) { return `${call.source}:${call.sessionId}`; }
function consumed(): Map<string, string> {
  try { return new Map(JSON.parse(sessionStorage.getItem(SEEN_KEY) || '[]')); }
  catch { return new Map(); }
}
function consume(call: AutomatedCall) {
  const calls = consumed(); calls.set(call.source!, call.sessionId!);
  // One tombstone per source: a long Zoom call cannot be rearmed by many
  // intervening Discord sessions evicting its manual-stop suppression.
  try { sessionStorage.setItem(SEEN_KEY, JSON.stringify([...calls])); } catch { /* fail closed below */ }
}
export function lifecycleCallCanStart(call: AutomatedCall): boolean {
  return !call.sessionId || (latest.enabled && latest.sources.some(s =>
    s.source === call.source && s.session_id === call.sessionId && s.presence === 'active'));
}
export function lifecycleCallCurrent(call: AutomatedCall): boolean {
  return !call.sessionId || (latest.enabled && latest.sources.some(s =>
    s.source === call.source && s.session_id === call.sessionId));
}

export function lifecycleCallEnded(call: AutomatedCall): boolean {
  return latest.enabled && latest.sources.some(s => s.source === call.source && (
    (s.presence === 'inactive' && s.session_id === null)
    // A new native session can only exist after the old one confirmed exit.
    || (s.session_id !== null && s.session_id !== call.sessionId)
  ));
}

/** One recorder, independent sources, first accepted session wins. Busy calls
 * are consumed, not queued: ending Zoom must never stop a manual/Discord take.
 * Shared by the root event listener and recorder's asynchronous start boundary.
 */
export function createLifecycleAutomation(deps: {
  enabled: () => boolean;
  state: () => Promise<{ is_recording?: boolean; is_stopping?: boolean }>;
  start: () => void;
  stop: () => void;
  error: (error: unknown) => void;
  conflict: (app: string) => void;
}) {
  let disposed = false;
  let processing = false;
  let dirty = false;
  const seen = consumed();
  async function reconcile() {
    if (processing || disposed) return;
    processing = true;
    try {
      while (dirty && !disposed) {
        dirty = false;
        if (!deps.enabled() || !latest.enabled) continue;
        const owner = automatedRecording();
        if (owner?.sessionId && lifecycleCallEnded(owner)) {
          const state = await deps.state();
          if (disposed) return;
          // A manual stop/start may have replaced the recording during await.
          const current = automatedRecording();
          if (current && identity(current) === identity(owner) && lifecycleCallEnded(owner)) {
            endAutomatedRecording();
            if (state.is_recording && !state.is_stopping) deps.stop();
          }
        }
        for (const source of latest.sources) {
          if (!source.session_id || source.presence !== 'active') continue;
          const call: AutomatedCall = { app: source.app, process: source.process, source: source.source, sessionId: source.session_id };
          if (seen.get(source.source) === source.session_id) continue;
          // Never retry the same call after manual Stop, failed startup or reload.
          seen.set(source.source, source.session_id); consume(call);
          const state = await deps.state();
          if (disposed || !deps.enabled() || !lifecycleCallCanStart(call)) continue;
          if (state.is_recording || state.is_stopping || automatedRecording() || automatedStartInProgress()) {
            deps.conflict(call.app); continue;
          }
          // Storage is required for a routed handoff; otherwise don't capture.
          if (!markAutomatedStart(call)) throw new Error('Meeting automation needs session storage. Start was not requested.');
          deps.start();
          // One request per reconciliation; other overlapping calls are consumed
          // on the next snapshot once the pending request reserves the recorder.
          break;
        }
      }
    } catch (error) { deps.error(error); }
    finally { processing = false; }
  }
  return {
    accept(snapshot: LifecycleSnapshot) {
      if (disposed || snapshot.revision <= latest.revision) return;
      latest = snapshot;
      dirty = true;
      void reconcile();
    },
    dispose() { disposed = true; },
  };
}
