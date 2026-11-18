/** Synthetic committed native reply for bridge-only owner fixtures. This does
 * not run capture, qualify a signal or prove the native lifecycle lock.
 */
import { freezeAutomatedStart, beginAutomatedRecording, type AutomatedCall } from '../../src/lib/meeting-automation';
export function commitAutomatedFixture(call: AutomatedCall): boolean {
  const expected = freezeAutomatedStart(call, { version: 1, epoch: 'synthetic-native-epoch', next_sequence: '1' });
  if (!expected) return false;
  return beginAutomatedRecording(call, { generation: `fixture:${call.token}`, owner: {
    source: call.source!, session_id: call.sessionId!, app: call.app, process: call.process,
  } });
}
