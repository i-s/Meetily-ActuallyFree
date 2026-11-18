import React from 'react';
import { act, create, ReactTestRenderer } from 'react-test-renderer';
import { afterEach, expect, mock, test } from 'bun:test';

const storage = new Map<string,string>();
(globalThis as any).window = { localStorage: { getItem: (key:string) => storage.get(key) ?? null, setItem: (key:string,value:string) => storage.set(key,value) } };
const calls: Array<[string, any]> = [];
let persisted: Array<{ id: string; status: string; kind?: string }> = [];
let completed = 0;
let timer: (() => void) | undefined;
const oldSetInterval = globalThis.setInterval;
const oldClearInterval = globalThis.clearInterval;
globalThis.setInterval = ((fn: () => void) => { timer = fn; return 1; }) as any;
globalThis.clearInterval = (() => { timer = undefined; }) as any;
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push([name, args]);
  if (name === 'list_transcript_versions') return { versions: persisted };
  if (name === 'api_get_post_call_transcript_config') return { provider: 'whisper', model: 'large-v3-turbo-q5_0' };
  if (name === 'whisper_get_available_models') return [{ name: 'large-v3-turbo-q5_0', status: 'Available' }];
  if (name === 'parakeet_get_available_models') return [];
  if (name === 'start_retranscription_command') { persisted = [{ id: 'job', status: 'running' }]; return { version_id: 'job' }; }
  if (name === 'cancel_retranscription_command') { persisted = [{ id: 'job', status: 'cancelled' }]; return; }
  throw new Error(`Unexpected product command ${name}`);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('@/contexts/ConfigContext', () => ({ useConfig: () => ({ selectedLanguage: 'ru', transcriptModelConfig: { provider: 'parakeet', model: 'live' } }) }));
mock.module('@/components/ui/button', () => ({ Button: ({ children, ...props }: any) => <button {...props}>{children}</button> }));
mock.module('@/components/PostCallHandoffCard', () => ({ PostCallHandoffCard: ({ children }: any) => <section>{children}</section> }));
mock.module('sonner', () => ({ toast: { success: () => {} } }));
const { PostCallProcessingDialog } = await import('../../src/components/MeetingDetails/PostCallProcessingDialog');
let root: ReactTestRenderer | undefined;
const view = () => <PostCallProcessingDialog enabled meetingId="m" meetingFolderPath="/synthetic/meeting" onComplete={() => { completed++; }} />;
const button = (text: string) => root!.root.findAllByType('button').find((node) => JSON.stringify(node.props.children).includes(text))!;
afterEach(async () => { await act(async () => root?.unmount()); calls.length = 0; persisted = []; completed = 0; storage.clear(); });

test('normal post-call entrypoint starts a local durable candidate, not selected-row diarization or summary', async () => {
  await act(async () => { root = create(view()); });
  await act(async () => button('Create refined version').props.onClick());
  expect(calls).toContainEqual(['start_retranscription_command', {
    meetingId: 'm', meetingFolderPath: '/synthetic/meeting', model: 'large-v3-turbo-q5_0', provider: 'whisper', language: 'ru',
  }]);
  expect(calls.some(([name]) => ['diarize_meeting', 'select_transcript_version', 'api_generate_summary'].includes(name))).toBe(false);
  persisted = [{ id: 'job', status: 'ready' }];
  await act(async () => timer!());
  expect(completed).toBe(1);
  expect(calls.some(([name]) => name === 'select_transcript_version')).toBe(false);
});

test('page remount observes the existing job and cancellation never starts a second pass', async () => {
  persisted = [{ id: 'already-running', status: 'running' }];
  await act(async () => { root = create(view()); });
  expect(JSON.stringify(root!.toJSON())).toContain('Cancel refinement');
  expect(calls.some(([name]) => name === 'start_retranscription_command')).toBe(false);
  await act(async () => button('Cancel refinement').props.onClick());
  expect(calls.some(([name]) => name === 'cancel_retranscription_command')).toBe(true);
});

// This file is run in its own Bun process, like all mock-heavy frontend groups.
process.on('exit', () => { globalThis.setInterval = oldSetInterval; globalThis.clearInterval = oldClearInterval; });

test('R1 ready result on remount consumes handoff without a second ASR pass', async () => {
  persisted = [{ id: 'ready-on-disk', status: 'ready', kind: 'refined' }];
  await act(async () => { root = create(view()); });
  expect(completed).toBe(1);
  expect(root!.toJSON()).toBeNull();
  expect(calls.some(([name]) => name === 'start_retranscription_command')).toBe(false);
});

test('R1 completion refresh rejection is visible and retry does not run ASR again', async () => {
  persisted = [{ id: 'job', status: 'running' }];
  let rejects = true;
  let refreshed = 0;
  await act(async () => { root = create(<PostCallProcessingDialog enabled meetingId="m" onComplete={() => { completed++; }} onRefetchTranscripts={async () => { refreshed++; if (rejects) throw new Error('refresh unavailable'); }} />); });
  persisted = [{ id: 'job', status: 'ready' }];
  await act(async () => timer!());
  expect(completed).toBe(0);
  expect(JSON.stringify(root!.toJSON())).toContain('refresh unavailable');
  rejects = false;
  await act(async () => button('Retry completion refresh').props.onClick());
  expect(completed).toBe(1);
  expect(refreshed).toBe(2);
  expect(calls.some(([name]) => name === 'start_retranscription_command')).toBe(false);
});

 test('R1 defer survives remount and is scoped to the meeting', async () => {
  await act(async () => { root = create(view()); });
  await act(async () => button('Keep live transcript').props.onClick());
  expect(storage.get('post-call-handoff:m')).toBe('deferred');
  await act(async () => root!.unmount());
  await act(async () => { root = create(view()); });
  expect(root!.toJSON()).toBeNull();
  await act(async () => root!.update(<PostCallProcessingDialog enabled meetingId="other" onComplete={() => {}} />));
  expect(JSON.stringify(root!.toJSON())).toContain('Create refined version');
 });
 test('R1 interrupted run offers an explicit later retry', async () => {
  persisted = [{id:'old-ready',kind:'refined',status:'ready'},{id:'interrupted',kind:'refined',status:'interrupted'}];
  await act(async () => { root = create(view()); });
  expect(completed).toBe(0);
  await act(async () => button('Retry local refinement').props.onClick());
  expect(calls.filter(([name]) => name === 'start_retranscription_command')).toHaveLength(1);
 });

 test('R1 consumed handoff callback rejection keeps a refresh retry, not another ASR start', async () => {
  storage.set('post-call-handoff:m','ready');
  let rejects = true;
  await act(async () => { root = create(<PostCallProcessingDialog enabled meetingId="m" onComplete={() => { if (rejects) throw new Error('handoff unavailable'); completed++; }} />); });
  expect(JSON.stringify(root!.toJSON())).toContain('handoff unavailable');
  rejects = false;
  await act(async () => button('Retry completion refresh').props.onClick());
  expect(completed).toBe(1);
  expect(root!.toJSON()).toBeNull();
  expect(calls.some(([name]) => name === 'start_retranscription_command')).toBe(false);
 });
