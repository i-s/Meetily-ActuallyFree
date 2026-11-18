import type { NativeRecordingState, RecordingLease } from './native-recording';
import { reconcileAutomatedRecording, releaseAutomatedStart, automatedStartInProgress, automatedRecording, endAutomatedRecording, markAutomatedStart, type AutomatedCall } from './meeting-automation';

export interface LifecycleSource {
  source: string;
  app: string;
  process: string;
  presence: 'active' | 'inactive' | 'unknown';
  reason: string;
  qualified: boolean;
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
    s.source === call.source && s.session_id === call.sessionId && s.presence === 'active' && s.qualified));
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
  state: () => Promise<NativeRecordingState>;
  start: () => void;
  stop: (expected: RecordingLease) => void;
  error: (error: unknown) => void;
  conflict: (app: string) => void;
}) {
  let disposed = false;
  let acceptedRevision = -1;
  let processing = false;
  let dirty = false;
  const seen = consumed();
  async function reconcile() {
    if (processing || disposed) return;
    processing = true;
    try {
      while (dirty && !disposed) {
        dirty = false;
        const allowed = deps.enabled() && latest.enabled;
        // Consume before the first asynchronous admission query. Another
        // adapter cannot turn a busy session into a later surprise recording.
        const candidates = allowed ? latest.sources.filter(source => source.qualified && source.session_id
          && source.presence === 'active' && seen.get(source.source) !== source.session_id) : [];
        for (const source of candidates) {
          const call = { app: source.app, process: source.process, source: source.source, sessionId: source.session_id! };
          seen.set(source.source, source.session_id!); consume(call);
        }
        const native = await deps.state();
        if (disposed) continue;
        const lease = native.recording_lease;
        // Native authority survives reload/start completion without the caller's
        // promise. Browser storage is a display/handoff mirror, never ownership.
        if (native.is_recording && lease?.owner) {
          const owner = lease.owner;
          const pending = automatedStartInProgress();
          if (pending?.source === owner.source && pending.sessionId === owner.session_id) releaseAutomatedStart(pending);
          reconcileAutomatedRecording({ source: owner.source, sessionId: owner.session_id,
            app: owner.app, process: owner.process });
          if (lifecycleCallEnded({ source: owner.source, sessionId: owner.session_id, app: owner.app, process: owner.process })
            && !native.is_stopping && deps.enabled() && latest.enabled) deps.stop(lease);
        } else {
          const mirror = automatedRecording();
          const legacyOwnsSameTake = native.is_recording && !mirror?.sessionId
            && !!lease && mirror?.recordingLease?.generation === lease.generation;
          if (!legacyOwnsSameTake && (native.is_recording || !automatedStartInProgress())) endAutomatedRecording();
        }
        for (const source of candidates) {
          const call: AutomatedCall = { app: source.app, process: source.process, source: source.source, sessionId: source.session_id! };
          const state = await deps.state();
          if (disposed || !deps.enabled() || !lifecycleCallCanStart(call)) continue;
          const owner = state.recording_lease?.owner;
          if (owner && owner.source === call.source && owner.session_id === call.sessionId) continue;
          if (state.is_recording || state.is_stopping || automatedRecording() || automatedStartInProgress()) {
            deps.conflict(call.app); continue;
          }
          // Storage is required for a routed handoff; otherwise don't capture.
          if (!markAutomatedStart(call)) throw new Error('Meeting automation needs session storage. Start was not requested.');
          deps.start();
          // The shared reservation makes remaining simultaneous sources busy;
          // they stay consumed rather than queued for an accidental handover.
        }
      }
    } catch (error) { deps.error(error); }
    finally { processing = false; }
  }
  return {
    accept(snapshot: LifecycleSnapshot) {
      if (disposed || snapshot.revision < latest.revision || snapshot.revision === acceptedRevision) return;
      acceptedRevision = snapshot.revision;
      latest = snapshot;
      dirty = true;
      void reconcile();
    },
    dispose() { disposed = true; },
  };
}
