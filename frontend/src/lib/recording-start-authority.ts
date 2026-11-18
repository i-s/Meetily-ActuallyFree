/** Wire contract shared with #10's native lifecycle authority, not a JS lease issuer. */
export interface StartCapability { version: 1; epoch: string; next_sequence: string }
export interface StartRequest {
  version: 1; epoch: string; sequence: string; source: string; session_id: string; token: string;
}
export interface LifecycleOwner { source: string; session_id: string; app: string; process: string }
export interface RecordingLease { generation: string; owner: LifecycleOwner | null }
export interface StartResolution {
  version: 1;
  expected: StartRequest;
  native_epoch: string;
  terminal: true;
  no_future_commit: true;
  outcome: 'committed' | 'cancelled' | 'failed' | 'rejected' | 'retired' | 'epoch_mismatch';
  committed_lease: RecordingLease | null;
  recording_lease: RecordingLease | null;
  is_recording: boolean;
  is_stopping: boolean;
}
const text = (v: unknown): v is string => typeof v === 'string' && v.length > 0;
const bounded = (v: unknown, bytes: number): v is string => text(v) && new TextEncoder().encode(v).length <= bytes;
const object = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
// Keep u64 sequences as canonical decimal strings; never round through a JS number.
const sequence = (v: unknown): v is string => text(v) && /^[1-9][0-9]*$/.test(v) &&
  (v.length < 20 || (v.length === 20 && v < '18446744073709551615'));
export function isStartCapability(v: unknown): v is StartCapability {
  return object(v) && v.version === 1 && bounded(v.epoch, 64) && sequence(v.next_sequence);
}
export function isStartRequest(v: unknown): v is StartRequest {
  return object(v) && v.version === 1 && bounded(v.epoch, 64) && sequence(v.sequence) && bounded(v.source, 128) && bounded(v.session_id, 256) && bounded(v.token, 256);
}
export function sameStartRequest(a: unknown, b: unknown): boolean {
  return isStartRequest(a) && isStartRequest(b) && a.version === b.version && a.epoch === b.epoch &&
    a.sequence === b.sequence && a.source === b.source && a.session_id === b.session_id && a.token === b.token;
}
export function isRecordingLease(v: unknown): v is RecordingLease {
  return object(v) && text(v.generation) && (v.owner === null || (object(v.owner) &&
    ['source', 'session_id', 'app', 'process'].every((k) => text((v.owner as Record<string, unknown>)[k]))));
}
export function isStartResolution(v: unknown, expected: StartRequest): v is StartResolution {
  if (!object(v) || v.version !== 1 || !sameStartRequest(v.expected, expected) || !text(v.native_epoch) ||
      v.terminal !== true || v.no_future_commit !== true || typeof v.is_recording !== 'boolean' ||
      typeof v.is_stopping !== 'boolean' || !['committed', 'cancelled', 'failed', 'rejected', 'retired', 'epoch_mismatch'].includes(String(v.outcome))) return false;
  if ((v.outcome === 'epoch_mismatch') !== (v.native_epoch !== expected.epoch)) return false;
  if (v.committed_lease !== null && !isRecordingLease(v.committed_lease)) return false;
  if (v.recording_lease !== null && !isRecordingLease(v.recording_lease)) return false;
  // An idle proof cannot simultaneously claim an active native lease. Busy may
  // include a manual owner or stopping with no lease, neither permits admission.
  if (!v.is_recording && !v.is_stopping && v.recording_lease !== null) return false;
  return v.outcome !== 'committed' || isRecordingLease(v.committed_lease);
}
