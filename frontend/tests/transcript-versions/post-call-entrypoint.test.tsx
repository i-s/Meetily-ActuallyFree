import React from 'react';
import { act, create, ReactTestRenderer } from 'react-test-renderer';
import { afterEach, expect, mock, test } from 'bun:test';

const calls: Array<[string, any]> = [];
let persisted: Array<{ id: string; status: string }> = [];
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
afterEach(async () => { await act(async () => root?.unmount()); calls.length = 0; persisted = []; completed = 0; });

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
