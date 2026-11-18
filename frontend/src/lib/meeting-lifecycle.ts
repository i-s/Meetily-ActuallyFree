import type { NativeRecordingState, RecordingLease } from './native-recording';
import { releaseAutomatedStart, automatedStartInProgress, markAutomatedStart, type AutomatedCall } from './meeting-automation';

export interface LifecycleSource {
  source: string;
  app: string;
  process: string;
  presence: 'active' | 'inactive' | 'unknown';
  reason: string;
  qualified: boolean;
  session_id: string | null;
}
export interface LifecycleSnapshot {
  revision: number;
  enabled: boolean;
  automation_enabled?: boolean;
  sources: LifecycleSource[];
}
let latest: LifecycleSnapshot = { revision: -1, enabled: false, sources: [] };
export function lifecycleCallCanStart(call: AutomatedCall): boolean {
  return call.source === 'zoom' && latest.enabled && latest.automation_enabled === true && latest.sources.some(s =>
    s.source === 'zoom' && s.session_id === call.sessionId && s.presence === 'active' && s.qualified);
}
export function lifecycleCallCurrent(call: AutomatedCall): boolean {
  return call.source === 'zoom' && latest.enabled && latest.sources.some(s => s.source === 'zoom' && s.session_id === call.sessionId);
}
export function lifecycleCallEnded(call: AutomatedCall): boolean {
  return call.source === 'zoom' && latest.enabled && latest.sources.some(s => s.source === 'zoom' && s.qualified && (
    (s.presence === 'inactive' && s.session_id === null)
    || (s.session_id !== null && s.session_id !== call.sessionId)
  ));
}

/** Zoom adapter only. Discord owns its own status/qualification contract, while
 * BOTH adapters use the one common reservation/tombstone store before any await.
 * Actual Stop authority comes from native, even if a JS start reply was lost.
 */
export function createLifecycleAutomation(deps: {
  enabled: () => boolean;
  state: () => Promise<NativeRecordingState>;
  start: (call: AutomatedCall) => void;
  stop: (expected: RecordingLease) => void;
  error: (error: unknown) => void;
  conflict: (app: string) => void;
}) {
  let disposed = false, processing = false, dirty = false;
  let acceptedRevision = -1;
  let stopping: string | null = null;
  async function reconcile() {
    if (processing || disposed) return;
    processing = true;
    try {
      while (dirty && !disposed) {
        dirty = false;
        const allowed = deps.enabled() && latest.enabled && latest.automation_enabled === true;
        const reserved: AutomatedCall[] = [];
        if (allowed) for (const source of latest.sources) {
          if (source.source !== 'zoom' || !source.qualified || !source.session_id || source.presence !== 'active') continue;
          const call = { app: source.app, process: source.process, source: source.source, sessionId: source.session_id };
          if (markAutomatedStart(call)) {
            const pending = automatedStartInProgress();
            if (pending) reserved.push(pending);
          }
        }
        const native = await deps.state();
        if (disposed) { reserved.forEach(releaseAutomatedStart); continue; }
        const lease = native.recording_lease;
        if (native.is_recording && lease?.owner?.source === 'zoom') {
          const owner = lease.owner;
          if (lifecycleCallEnded({ source: owner.source, sessionId: owner.session_id, app: owner.app, process: owner.process })
            && !native.is_stopping && deps.enabled() && latest.automation_enabled === true) {
            const key = JSON.stringify(lease);
            if (stopping !== key) { stopping = key; deps.stop(lease); }
          }
        } else stopping = null;
        for (const call of reserved) {
          if (!deps.enabled() || !lifecycleCallCanStart(call)) { releaseAutomatedStart(call); continue; }
          if (native.is_recording || native.is_stopping) {
            releaseAutomatedStart(call); deps.conflict(call.app); continue;
          }
          deps.start(call);
        }
      }
    } catch (error) { deps.error(error); }
    finally { processing = false; }
  }
  return {
    accept(snapshot: LifecycleSnapshot) {
      if (disposed || snapshot.revision < latest.revision || snapshot.revision === acceptedRevision) return;
      acceptedRevision = snapshot.revision; latest = snapshot; dirty = true; void reconcile();
    },
    recordingChanged() { dirty = true; void reconcile(); },
    dispose() { disposed = true; },
  };
}
