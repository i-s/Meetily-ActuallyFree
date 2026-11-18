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
const CLAIM_KEY = 'discordAutomationClaim';
export function discordProcess(session: string) { return `${PREFIX}${session}`; }
export function isDiscordProcess(process: string) { return process.startsWith(PREFIX); }
export function discordMayStart(status: DiscordCallStatus, process: string): boolean {
  return status.enabled && status.qualified && status.state === 'active' && !!status.session_id &&
    discordProcess(status.session_id) === process;
}
function leaseMatchesCall(lease: DiscordRecordingLease | null | undefined, call: AutomatedCall): lease is DiscordRecordingLease {
  return !!lease?.generation && lease.owner?.source === call.source &&
    lease.owner?.session_id === call.sessionId && lease.owner?.process === call.process && lease.owner?.app === call.app;
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
    const owner = automatedRecording();
    const ended = status.transition === 'end' || (status.state === 'inactive' && !status.session_id);
    if (ended && owner?.source === 'discord' && reservationCurrent(owner, 'recording')) {
      if (status.session_id && status.session_id !== owner.sessionId) return;
      const state = await deps.recording();
      if (disposed || latest !== status || !deps.allowed() || !reservationCurrent(owner, 'recording')) return;
      const lease = state.recording_lease;
      if (!state.is_recording || state.is_stopping || !leaseMatchesCall(lease, owner) || !deps.stop) return;
      const expectedLease = { generation: lease.generation, owner: lease.owner ? { ...lease.owner } : null };
      const fresh = await deps.status();
      if (disposed || latest !== status || !deps.allowed() || !reservationCurrent(owner, 'recording')) return;
      const stillEnded = fresh.enabled && fresh.qualified &&
        ((fresh.transition === 'end' && fresh.session_id === status.session_id) || (fresh.state === 'inactive' && !fresh.session_id));
      if (!stillEnded) return;
      const current = await deps.recording();
      if (disposed || latest !== status || !deps.allowed() || !reservationCurrent(owner, 'recording') ||
          !current.is_recording || current.is_stopping || !sameLease(expectedLease, current.recording_lease)) return;
      const stopId = `${owner.token}:${expectedLease.generation}`;
      if (stopping === stopId) return;
      stopping = stopId;
      deps.stop(expectedLease, owner); // Native must atomically compare again after dispatch.
      return;
    }
    if (!status.session_id || status.state !== 'active') return;
    const process = discordProcess(status.session_id);
    if (sessionStorage.getItem(CLAIM_KEY) === process) return;
    // Consume BEFORE any await, including busy/manual/pending admission. Never
    // queue this observed call to unexpectedly start after somebody presses Stop.
    sessionStorage.setItem(CLAIM_KEY, process);
    if (sessionStorage.getItem(CLAIM_KEY) !== process) throw new Error('Discord automation needs session storage');
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
