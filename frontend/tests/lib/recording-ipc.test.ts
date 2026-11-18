import { expect, mock, test, beforeEach } from 'bun:test';
import assert from 'node:assert/strict';
import fixture from '../fixtures/recording-start-v1.json';
import { isStartRequest, isStartResolution, isStartCapability } from '../../src/lib/recording-start-authority';
const calls: Array<[string, any]> = [];
let reply: unknown;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: unknown) => {
  calls.push([name, args]); if (reply instanceof Error) throw reply; return reply;
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
const { RecordingService } = await import('../../src/services/recordingService');
const service = new RecordingService();
beforeEach(() => { calls.length = 0; reply = undefined; });

test('device selections and meeting name keep ordinary manual IPC free of automation fields', async () => {
  await service.startRecordingWithDevices('USB (input)', 'Headphones (output)', 'Chosen title');
  expect(calls.at(-1)).toEqual(['start_recording_with_devices_and_meeting', {
    micDeviceName: 'USB (input)', systemDeviceName: 'Headphones (output)', meetingName: 'Chosen title',
  }]);
});
test('committed native v1 fixture: u64 decimal bounds, byte bounds, entire tuple and native_epoch', () => {
  expect(isStartCapability(fixture.capability)).toBe(true);
  expect(isStartRequest(fixture.request)).toBe(true);
  for (const sequence of fixture.invalid_sequences) expect(isStartRequest({ ...fixture.request, sequence })).toBe(false);
  expect(isStartRequest({ ...fixture.request, source: 'é'.repeat(65) })).toBe(false);
  expect(isStartCapability({ ...fixture.capability, version: 2 })).toBe(false);
  for (const resolution of fixture.resolutions) expect(isStartResolution(resolution, fixture.request as any)).toBe(true);
  for (const key of ['epoch', 'sequence', 'source', 'session_id', 'token']) {
    expect(isStartResolution({ ...fixture.resolutions[0], expected: { ...fixture.request, [key]: 'wrong' } }, fixture.request as any)).toBe(false);
  }
  expect(isStartResolution({ ...fixture.resolutions[0], native_epoch: 'new' }, fixture.request as any)).toBe(false);
  expect(isStartResolution({ ...fixture.resolutions[3], native_epoch: fixture.request.epoch }, fixture.request as any)).toBe(false);
});
test('automatic start transports immutable expectedStart and matching lifecycleSession, success returns lease', async () => {
  reply = fixture.capability;
  const cap = await service.getRecordingStartCapability(); expect(cap.next_sequence).toBe('18446744073709551614');
  const expected = Object.freeze({ ...fixture.request }) as any;
  reply = fixture.resolutions[1].committed_lease;
  assert.deepEqual(await service.startRecordingWithDevices('mic', 'system', 'Auto', expected.session_id, expected), reply);
  expect(calls.at(-1)).toEqual(['start_recording_with_devices_and_meeting', {
    micDeviceName: 'mic', systemDeviceName: 'system', meetingName: 'Auto', lifecycleSession: expected.session_id, expectedStart: fixture.request,
  }]);
});
test('missing capability, rejected command and malformed automatic guards never dispatch a manual fallback', async () => {
  reply = new Error('unknown command');
  await expect(service.getRecordingStartCapability()).rejects.toThrow('unknown command');
  reply = {};
  await expect(service.getRecordingStartCapability()).rejects.toThrow('unavailable');
  calls.length = 0;
  for (const [session, expected] of [[fixture.request.session_id, undefined], [undefined, fixture.request], ['different', fixture.request], [fixture.request.session_id, {}]]) {
    await expect(service.startRecordingWithDevices(null, null, 'Auto', session as any, expected as any)).rejects.toThrow('exact lifecycle session');
  }
  expect(calls).toHaveLength(0);
});
test('resolve/cancel validate terminal native receipt, future errors/timeouts and idle snapshots do not settle', async () => {
  const expected = fixture.request as any;
  for (const resolution of fixture.resolutions) {
    reply = resolution;
    assert.deepEqual(await service.resolveAutomatedRecordingStart(expected), resolution);
    expect(calls.at(-1)).toEqual(['resolve_automated_recording_start', { expected }]);
    assert.deepEqual(await service.cancelAutomatedRecordingStart(expected), resolution);
    expect(calls.at(-1)).toEqual(['cancel_automated_recording_start', { expected }]);
  }
  for (const invalid of [undefined, {}, { is_recording: false }, { ...fixture.resolutions[0], terminal: false },
    { ...fixture.resolutions[0], no_future_commit: false }, { ...fixture.resolutions[0], recording_lease: {} },
    { ...fixture.resolutions[0], expected: { ...expected, token: 'B' } }]) {
    reply = invalid;
    await expect(service.resolveAutomatedRecordingStart(expected)).rejects.toThrow('remains unresolved');
  }
  for (const error of [new Error(fixture.future_sequence_error.error), new Error('transport timeout')]) {
    reply = error; await expect(service.cancelAutomatedRecordingStart(expected)).rejects.toThrow(error.message);
  }
});
