import { recordingService } from '@/services/recordingService';
import { automatedStartState, applyAutomatedStartResolution, reconcileAutomatedRecording,
  sameAutomationReservation, type AutomatedCall } from './meeting-automation';
import { sameStartRequest, type StartRequest, type StartResolution } from './recording-start-authority';

let settling: { expected: StartRequest; promise: Promise<StartResolution | null> } | null = null;

/** One barrier per persisted request. Errors/future-sequence rejection preserve
 * the fence; only a validated terminal idle receipt can release it. This does
 * not stop a committed recording, even when Labs or a source opt-in is off.
 */
export function settleAutomatedStart(call: AutomatedCall, expected: StartRequest, cancel = false): Promise<StartResolution | null> {
  const state = automatedStartState();
  if (state.kind !== 'native_unresolved' || !sameAutomationReservation(state.call, call)
    || !sameStartRequest(state.expected, expected)) return Promise.resolve(null);
  if (settling && sameStartRequest(settling.expected, expected)) return settling.promise;
  const operation = (async () => {
    const receipt = await (cancel ? recordingService.cancelAutomatedRecordingStart(expected)
      : recordingService.resolveAutomatedRecordingStart(expected));
    const result = applyAutomatedStartResolution(call, expected, receipt);
    if (result === 'settled_busy') reconcileAutomatedRecording(call, expected, receipt);
    return receipt;
  })();
  const entry = { expected, promise: operation };
  settling = entry;
  void operation.finally(() => { if (settling === entry) settling = null; }).catch(() => {});
  return operation;
}

export function recoverAutomatedStart(): Promise<StartResolution | null> {
  const state = automatedStartState();
  if (state.kind !== 'native_unresolved' || !state.expected) return Promise.resolve(null);
  return settleAutomatedStart(state.call, state.expected);
}
