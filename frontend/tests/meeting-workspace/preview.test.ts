import { expect, mock, test } from 'bun:test';
let ipc!: (command: string, args: any) => any;
const emitted: Array<{name: string; payload: any}> = [];
const browser = Object.assign(new EventTarget(), { location: { search: '' } });
Object.assign(globalThis, { window: browser, localStorage: { getItem: () => 'preview', setItem: () => {} } });
mock.module('framer-motion', () => ({ MotionGlobalConfig: {} }));
mock.module('@tauri-apps/api/event', () => ({ emit: async (name: string, payload: any) => { emitted.push({name,payload}); } }));
mock.module('@tauri-apps/api/mocks', () => ({ mockWindows: () => {}, mockIPC: (callback: any) => { ipc = callback; (browser as any).__TAURI_INTERNALS__ = {}; } }));
const { installPreviewMocks } = await import('../../src/dev/preview');
installPreviewMocks();

test('preview allocates one meeting at Start, retains it at short Stop, and creates another on next Start', async () => {
  await ipc('start_recording_with_devices_and_meeting', { meetingName: 'Silent preview' });
  const active = await ipc('get_recording_state', {});
  expect(typeof active.meeting_id).toBe('string');
  expect((await ipc('api_get_meeting', {meetingId: active.meeting_id})).title).toBe('Silent preview');
  await ipc('pause_recording', {}); await ipc('resume_recording', {});
  expect((await ipc('get_recording_state', {})).meeting_id).toBe(active.meeting_id);
  await ipc('stop_recording', {});
  const saved = await ipc('api_save_transcript', { meetingId: active.meeting_id, meetingTitle: 'Silent preview', transcripts: [] });
  expect(saved.meeting_id).toBe(active.meeting_id);
  await ipc('start_recording_with_devices_and_meeting', { meetingName: 'Next preview' });
  const next = await ipc('get_recording_state', {});
  await ipc('stop_recording', {});
  expect(next.meeting_id).not.toBe(active.meeting_id);
});

test('preview context counts Unicode text only and saves actual request range with fixture answer', async () => {
  const result = await ipc('api_save_transcript', { meetingTitle: 'Unicode preview', transcripts: [
    {text: 'abc', audio_start_time: 0, audio_end_time: 2},
    {text: 'А🙂Б界В', audio_start_time: 74, audio_end_time: 77},
  ] });
  const meetingId = result.meeting_id;
  await ipc('set_ask_ai_context_settings', { settings: { mode: 'recent', maxChars: 3 } });
  const range = await ipc('get_meeting_chat_context', {meetingId});
  expect(range).toMatchObject({totalChars: 8, includedChars: 3, startChar: 5, endChar: 8, startOffsetChars: 2, startTime: 74});
  const turn = await ipc('send_meeting_chat', {meetingId, question: 'What happened?'});
  expect(turn.status).toBe('pending');
  await new Promise(resolve => setTimeout(resolve, 800));
  const history = await ipc('get_meeting_chat', {meetingId});
  expect(history[0].answer).toContain('Preview fixture');
  expect(history[0].context).toEqual(range);
  await ipc('api_delete_meeting', {meetingId});
  await expect(Promise.resolve().then(() => ipc('get_meeting_chat', {meetingId}))).rejects.toThrow('Meeting not found');
});

test('preview summary records immutable notes snapshot independently of later edits', async () => {
  const {meeting_id: meetingId} = await ipc('api_save_transcript', {meetingTitle: 'Notes-only preview', transcripts: []});
  await ipc('api_save_meeting_notes', {meetingId, markdown:'Before', json: [{text:'Before'}]});
  const started = await ipc('api_process_transcript', {meetingId});
  expect(typeof started.process_id).toBe('string');
  await ipc('api_save_meeting_notes', {meetingId, markdown:'After', json: [{text:'After'}]});
  await new Promise(resolve => setTimeout(resolve, 850));
  expect((await ipc('api_get_summary_input_snapshot', {meetingId})).notesMarkdown).toBe('Before');
  expect((await ipc('api_get_summary', {meetingId})).data.markdown).toContain('Before');
});
