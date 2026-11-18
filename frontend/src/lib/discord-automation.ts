/** Discord-only evidence bridge. Shared reservation is the only local owner;
 * the integrated native lease is the final authority for Stop.
 */
import {
  automatedRecording, automatedStartInProgress, markAutomatedStart,
  releaseAutomatedStart, reservationCurrent, type AutomatedCall,
} from './meeting-automation';

export interface DiscordCallStatus {
  enabled: boolean;
  state: 'active' | 'inactive' | 'unknown';
  reason: string;
  session_id: string | null;
  transition: 'start' | 'end' | 'none';
  qualified: boolean;
  platform_supported?: boolean;
  client_present?: boolean | null;
  recording_active?: boolean;
  evidence?: { complete: boolean; connected: boolean; disconnect: boolean; reconnect: boolean; join: boolean; connected_pair: boolean };
  cue_transition?: string;
}
/** Structural match for #10's native RecordingLease. No local generation minting. */
export interface DiscordRecordingLease {
  generation: string;
  owner: { source: string; session_id: string; app: string; process: string } | null;
}
interface RecordingSnapshot {
  is_recording?: boolean;
  is_stopping?: boolean;
  recording_lease?: DiscordRecordingLease | null;
}
const PREFIX = 'discord-macos:';
export function discordProcess(session: string) { return `${PREFIX}${session}`; }
export function isDiscordProcess(process: string) { return process.startsWith(PREFIX); }
export function discordMayStart(status: DiscordCallStatus, process: string): boolean {
  return status.enabled && status.qualified && status.state === 'active' && !!status.session_id &&
    discordProcess(status.session_id) === process;
}
function sameLease(a: DiscordRecordingLease, b: DiscordRecordingLease | null | undefined): boolean {
  return !!b && a.generation === b.generation && a.owner?.source === b.owner?.source &&
    a.owner?.session_id === b.owner?.session_id && a.owner?.process === b.owner?.process && a.owner?.app === b.owner?.app;
}
interface Dependencies {
  allowed: () => boolean;
  status: () => Promise<DiscordCallStatus>;
  recording: () => Promise<RecordingSnapshot>;
  start: (expectedCall: AutomatedCall) => void;
  /** Must route this exact lease through #10's guarded normal Stop boundary.
   * A caller without that integrated boundary must provide no stop dispatcher.
   */
  stop?: (expectedLease: DiscordRecordingLease, expectedCall: AutomatedCall) => void;
  error: (error: unknown) => void;
}
export function createDiscordAutomation(deps: Dependencies) {
  let latest: DiscordCallStatus | null = null;
  let busy = false, dirty = false, disposed = false;
  let stopping: string | null = null;
  async function reconcileOne(status: DiscordCallStatus) {
    if (!status.enabled || !status.qualified || !deps.allowed()) return;
    const ended = status.transition === 'end' || (status.state === 'inactive' && !status.session_id);
    if (ended) {
      const state = await deps.recording();
      const lease = state.recording_lease;
      if (disposed || latest !== status || !deps.allowed() || !state.is_recording || state.is_stopping
        || lease?.owner?.source !== 'discord' || !deps.stop) return;
      const nativeOwner = lease.owner;
      if (status.session_id && status.session_id !== nativeOwner.session_id) return;
      const fresh = await deps.status();
      if (disposed || latest !== status || !deps.allowed() || !fresh.enabled || !fresh.qualified
        || !((fresh.transition === 'end' && fresh.session_id === status.session_id) || (fresh.state === 'inactive' && !fresh.session_id))) return;
      const current = await deps.recording();
      if (disposed || latest !== status || !deps.allowed() || !current.is_recording || current.is_stopping
        || !sameLease(lease, current.recording_lease)) return;
      const stopId = JSON.stringify(lease);
      if (stopping === stopId) return;
      stopping = stopId;
      deps.stop(lease, { source: 'discord', sessionId: nativeOwner.session_id, app: nativeOwner.app, process: nativeOwner.process });
      return;
    }
    if (!status.session_id || status.state !== 'active') return;
    const process = discordProcess(status.session_id);
    // Consume BEFORE any await, including busy/manual/pending admission. Never
    // queue this observed call to unexpectedly start after somebody presses Stop.
    if (!markAutomatedStart({ app: 'Discord', process, source: 'discord', sessionId: status.session_id })) return;
    const reserved = automatedStartInProgress();
    if (!reserved || reserved.process !== process || reserved.source !== 'discord' || !reservationCurrent(reserved, 'pending')) return;
    let dispatched = false;
    try {
      const state = await deps.recording();
      if (state.is_recording || state.is_stopping || automatedRecording() || disposed || latest !== status ||
          !deps.allowed() || !reservationCurrent(reserved, 'pending')) return;
      const fresh = await deps.status();
      if (disposed || latest !== status || !deps.allowed() || !discordMayStart(fresh, process) || !reservationCurrent(reserved, 'pending')) return;
      const current = await deps.recording();
      if (current.is_recording || current.is_stopping || automatedRecording() || disposed || latest !== status ||
          !deps.allowed() || !reservationCurrent(reserved, 'pending')) return;
      stopping = null;
      deps.start(reserved);
      dispatched = true;
    } finally { if (!dispatched) releaseAutomatedStart(reserved); }
  }
  async function reconcile() {
    if (disposed || busy) return;
    busy = true;
    try { while (!disposed && dirty && latest) { dirty = false; await reconcileOne(latest); } }
    catch (error) { deps.error(error); }
    finally { busy = false; }
  }
  return {
    update(status: DiscordCallStatus) { latest = status; dirty = true; void reconcile(); },
    recordingChanged() { dirty = true; void reconcile(); },
    dispose() { disposed = true; },
  };
}
