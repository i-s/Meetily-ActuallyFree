/**
 * Starts and stops a recording from anywhere in the app. The recorder page
 * owns both sequences, so from another page these open the recorder with a
 * flag it picks up on arrival; on the recorder they act right away.
 */
import { writePendingGroup, type PendingGroup } from '@/lib/groups';

export const AUTO_START_KEY = 'autoStartRecording';
export const START_RECORDING_EVENT = 'start-recording-from-sidebar';
export const STOP_REQUEST_KEY = 'af-stop-requested';
export const STOP_RECORDING_EVENT = 'af-stop-recording';

export function launchRecording(navigate: (href: string) => void, options: { group?: PendingGroup | null } = {}) {
  if (options.group !== undefined) writePendingGroup(options.group);
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(START_RECORDING_EVENT));
    return;
  }
  try {
    sessionStorage.setItem(AUTO_START_KEY, 'true');
  } catch {
    // Without storage the recorder opens and waits for the button.
  }
  navigate('/');
}

/** Stop and save, the same way the record card's Stop button does. */
export function requestRecordingStop(navigate: (href: string) => void) {
  try {
    sessionStorage.setItem(STOP_REQUEST_KEY, '1');
  } catch {
    // Without storage the user can still stop from the recorder.
  }
  if (window.location.pathname === '/') {
    window.dispatchEvent(new CustomEvent(STOP_RECORDING_EVENT));
    return;
  }
  navigate('/');
}

/** Keep early stop requests until the recorder can run its normal save flow. */
export function consumeRecordingStopRequest(ready: boolean, stop: () => void): boolean {
  if (!ready) return false;
  try {
    if (sessionStorage.getItem(STOP_REQUEST_KEY) !== '1') return false;
    // Consume before dispatching so an event and readiness effect cannot both stop.
    sessionStorage.removeItem(STOP_REQUEST_KEY);
  } catch {
    return false;
  }
  stop();
  return true;
}
