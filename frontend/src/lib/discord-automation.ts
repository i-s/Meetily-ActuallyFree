/** Discord-only bridge into the ordinary recorder. No capture or RPC in this adapter. */
import {
  automatedRecording, markAutomatedStart,
} from './meeting-automation';

export interface DiscordCallStatus {
  enabled: boolean;
  state: 'active' | 'inactive' | 'unknown';
  reason: string;
  session_id: string | null;
  transition: 'start' | 'end' | 'none';
  qualified: boolean;
}
const PREFIX = 'discord-macos:';
const CLAIM_KEY = 'discordAutomationClaim';
const REQUEST_KEY = 'discordAutomationRequest';
export function takeDiscordStartRequest(): { process: string; at: number } | null {
  try {
    const value = sessionStorage.getItem(REQUEST_KEY);
    sessionStorage.removeItem(REQUEST_KEY);
    if (!value) return null;
    return JSON.parse(value) as { process: string; at: number };
  } catch { return null; }
}
export function discordProcess(session: string) { return `${PREFIX}${session}`; }
export function isDiscordProcess(process: string) { return process.startsWith(PREFIX); }
export function discordMayStart(status: DiscordCallStatus, process: string): boolean {
  return status.enabled && status.qualified && status.state === 'active' && !!status.session_id &&
    discordProcess(status.session_id) === process;
}

interface Dependencies {
  allowed: () => boolean;
  status: () => Promise<DiscordCallStatus>;
  recording: () => Promise<{ is_recording?: boolean }>;
  start: () => void;
  stop: () => void;
  error: (error: unknown) => void;
}

export function createDiscordAutomation(deps: Dependencies) {
  let latest: DiscordCallStatus | null = null;
  let busy = false;
  let disposed = false;
  let stopping: string | null = null;

  async function reconcile() {
    if (disposed || busy || !latest) return;
    busy = true;
    try {
      const status = latest;
      const owner = automatedRecording();
      if (!status.enabled || !status.qualified || !deps.allowed()) {
        return; // Turning automation off never stops an existing recording.
      }
      const ended = status.transition === 'end' || (status.state === 'inactive' && !status.session_id);
      if (ended && owner && isDiscordProcess(owner.process)) {
        if (status.session_id && discordProcess(status.session_id) !== owner.process) return;
        const state = await deps.recording();
        const fresh = await deps.status();
        const stillEnded = fresh.enabled && fresh.qualified &&
          ((fresh.transition === 'end' && fresh.session_id === status.session_id) ||
            (fresh.state === 'inactive' && !fresh.session_id));
        // Manual Stop/Don't stop it may revoke ownership during the await.
        if (!disposed && latest === status && stillEnded && deps.allowed() && automatedRecording()?.process === owner.process && state.is_recording && stopping !== owner.process) {
          stopping = owner.process;
          deps.stop();
        }
        return;
      }
      if (!status.session_id || status.state !== 'active') return;
      const process = discordProcess(status.session_id);
      // A per-call claim survives route changes, disable/re-enable and manual Stop.
      // Without storage there is no safe automation ownership; fail closed.
      if (sessionStorage.getItem(CLAIM_KEY) === process) return;
      const state = await deps.recording();
      if (state.is_recording || disposed || latest !== status || !deps.allowed()) return;
      const fresh = await deps.status();
      if (disposed || latest !== status || !deps.allowed() || !discordMayStart(fresh, process)) return;
      sessionStorage.setItem(CLAIM_KEY, process);
      if (sessionStorage.getItem(CLAIM_KEY) !== process) return;
      sessionStorage.setItem(REQUEST_KEY, JSON.stringify({ process, at: Date.now() }));
      markAutomatedStart({ app: 'Discord', process });
      stopping = null;
      deps.start();
    } catch (error) { deps.error(error); }
    finally { busy = false; }
  }

  return {
    update(status: DiscordCallStatus) {
      latest = status;
      void reconcile();
    },
    recordingChanged() { void reconcile(); },
    dispose() { disposed = true; },
  };
}
