import { expect, test } from 'bun:test';
import { createMeetingDetectionSessions, startDetectedMeeting } from '../../src/lib/meeting-automation';

const zoom = { app: 'Zoom', process: 'zoom', session_id: 1 };
const discord = { app: 'Discord', process: 'discord', session_id: 2 };

test('duplicate detections and a replay after end never create a second prompt', () => {
  const sessions = createMeetingDetectionSessions();
  expect(sessions.detect(zoom)).toBe(true);
  expect(sessions.detect(zoom)).toBe(false);
  sessions.end(zoom);
  expect(sessions.detect(zoom)).toBe(false);
  expect(sessions.isActive(zoom)).toBe(false);
  expect(sessions.detect({ ...zoom, session_id: 3 })).toBe(true);
});

test('an end received during notification restoration prevents restoring that old session', () => {
  const sessions = createMeetingDetectionSessions();
  sessions.end(zoom);
  expect(sessions.detect(zoom)).toBe(false);
  expect(sessions.isActive(zoom)).toBe(false);
});

test('an ended or superseded session invalidates an asynchronous start', () => {
  const sessions = createMeetingDetectionSessions();
  sessions.detect(zoom);
  const pending = sessions.claimStart(zoom)!;
  expect(sessions.canStart(zoom, pending)).toBe(true);
  sessions.end(zoom);
  expect(sessions.canStart(zoom, pending)).toBe(false);
  sessions.detect({ ...zoom, session_id: 3 });
  expect(sessions.canStart(zoom, pending)).toBe(false);
  sessions.end(zoom);
  expect(sessions.isActive({ ...zoom, session_id: 3 })).toBe(true);
});

test('simultaneous calls share a reservation through recorder startup', () => {
  const sessions = createMeetingDetectionSessions();
  sessions.detect(zoom);
  sessions.detect(discord);
  const first = sessions.claimStart(zoom)!;
  expect(sessions.claimStart(discord)).toBeNull();
  sessions.releaseStart(first);
  const second = sessions.claimStart(discord)!;
  sessions.releaseStart(first);
  expect(sessions.canStart(discord, second)).toBe(true);
  expect(sessions.claimStart(zoom)).toBeNull();
  sessions.recordingStarted();
  expect(sessions.claimStart(zoom)).not.toBeNull();
});

test('an end during native validation cancels launch even if the old probe returns true', async () => {
  const sessions = createMeetingDetectionSessions();
  sessions.detect(zoom);
  let resolve!: (valid: boolean) => void;
  let validated!: () => void;
  const probing = new Promise<void>(done => { validated = done; });
  let starts = 0;
  const start = startDetectedMeeting(zoom, sessions, {
    getRecordingState: async () => ({ is_recording: false }),
    validateSession: () => { validated(); return new Promise(done => { resolve = done; }); },
    enabled: () => true,
    launch: () => { starts++; },
  });
  await probing;
  sessions.end(zoom);
  resolve(true);
  expect(await start).toBe('skipped');
  expect(starts).toBe(0);
});

test('simultaneous async detections launch once and a failed validation does not launch', async () => {
  const sessions = createMeetingDetectionSessions();
  sessions.detect(zoom);
  sessions.detect(discord);
  let starts = 0;
  const handlers = {
    getRecordingState: async () => ({ is_recording: false }),
    validateSession: async () => true,
    enabled: () => true,
    launch: () => { starts++; },
  };
  expect(await Promise.all([
    startDetectedMeeting(zoom, sessions, handlers),
    startDetectedMeeting(discord, sessions, handlers),
  ])).toEqual(['started', 'skipped']);
  expect(starts).toBe(1);
  sessions.recordingStarted();
  expect(await startDetectedMeeting(discord, sessions, { ...handlers, validateSession: async () => false })).toBe('unconfirmed');
  expect(starts).toBe(1);
});

test('manual recording and changed settings prevent automation from launching', async () => {
  const sessions = createMeetingDetectionSessions();
  sessions.detect(zoom);
  let validations = 0;
  let starts = 0;
  let enabled = true;
  const handlers = {
    getRecordingState: async () => ({ is_recording: true }),
    validateSession: async () => { validations++; enabled = false; return true; },
    enabled: () => enabled,
    launch: () => { starts++; },
  };
  expect(await startDetectedMeeting(zoom, sessions, handlers)).toBe('skipped');
  expect(validations).toBe(0);
  expect(await startDetectedMeeting(zoom, sessions, { ...handlers, getRecordingState: async () => ({ is_recording: false }) })).toBe('skipped');
  expect(validations).toBe(1);
  expect(starts).toBe(0);
});
