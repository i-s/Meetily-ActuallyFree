import React from 'react';
import { act, create, ReactTestRenderer } from 'react-test-renderer';
import { afterEach, expect, mock, test } from 'bun:test';

const calls: Array<[string, any]> = [];
let revision = 7;
let missingAudio = false;
let stale = false;
let changed = 0;
const version = { id: 'refined', kind: 'refined', status: 'ready', created_at: '2026-11-18T10:00:00Z', settings_json: '{"provider":"whisper"}' };
mock.module('@tauri-apps/api/core', () => ({ invoke: async (name: string, args: any) => {
  calls.push([name, args]);
  if (name === 'list_transcript_versions') return { selected_id: 'live', revision, audio_available: !missingAudio, versions: [version] };
  if (name === 'get_selected_transcript') return { revision, segments: [{ id: 'live-line', text: 'Original text', speaker: 'You', audio_start_time: 1 }] };
  if (name === 'correct_transcript_text') return;
  if (name === 'get_transcript_version') return [{ id: 'line', text: 'RU/EN API candidate', speaker: 'Guest', audio_start_time: 1 }];
  if (name === 'select_transcript_version') {
    if (stale) throw new Error('Transcript changed in another operation. Refresh versions and review again; nothing was replaced.');
    revision++;
    return;
  }
  throw new Error(`Unexpected command ${name}`);
} }));
mock.module('@/components/ui/button', () => ({ Button: ({ children, ...props }: any) => <button {...props}>{children}</button> }));
const { TranscriptVersions } = await import('../../src/components/MeetingDetails/TranscriptVersions');
let root: ReactTestRenderer | undefined;
const button = (text: string) => root!.root.findAllByType('button').find((node) => JSON.stringify(node.props.children).includes(text))!;
afterEach(async () => { await act(async () => root?.unmount()); calls.length = 0; missingAudio = false; stale = false; changed = 0; revision = 7; });

test('normal version review never selects until explicitly confirmed, with revision CAS', async () => {
  await act(async () => { root = create(<TranscriptVersions meetingId="m" onChanged={() => { changed++; }} />); });
  expect(calls.map(([name]) => name)).toEqual(['list_transcript_versions']);
  await act(async () => button('Review snapshot').props.onClick());
  expect(JSON.stringify(root!.toJSON())).toContain('RU/EN API candidate');
  expect(calls.some(([name]) => name === 'select_transcript_version')).toBe(false);
  await act(async () => button('Save current corrections').props.onClick());
  expect(calls).toContainEqual(['select_transcript_version', { meetingId: 'm', versionId: 'refined', expectedRevision: 7 }]);
  expect(changed).toBe(1);
});

test('audio deletion still permits review and stale selection displays a visible error', async () => {
  missingAudio = true; stale = true;
  await act(async () => { root = create(<TranscriptVersions meetingId="m" onChanged={() => { changed++; }} />); });
  expect(JSON.stringify(root!.toJSON())).toContain('Reprocessing is disabled');
  await act(async () => button('Review snapshot').props.onClick());
  await act(async () => button('Save current corrections').props.onClick());
  expect(JSON.stringify(root!.toJSON())).toContain('nothing was replaced');
  expect(changed).toBe(0);
});

test('keeping current version is a read-only action', async () => {
  await act(async () => { root = create(<TranscriptVersions meetingId="m" onChanged={() => { changed++; }} />); });
  await act(async () => button('Review snapshot').props.onClick());
  await act(async () => button('Keep current version').props.onClick());
  expect(calls.some(([name]) => name === 'select_transcript_version')).toBe(false);
});


test('manual text correction uses selected-row identity and its loaded revision', async () => {
  await act(async () => { root = create(<TranscriptVersions meetingId="m" onChanged={() => { changed++; }} />); });
  await act(async () => button('Correct selected text').props.onClick());
  const textarea = root!.root.findByType('textarea');
  await act(async () => textarea.props.onChange({ target: { value: 'Corrected API text' } }));
  await act(async () => button('Save text correction').props.onClick());
  expect(calls).toContainEqual(['correct_transcript_text', { meetingId: 'm', transcriptId: 'live-line', text: 'Corrected API text', expectedRevision: 7 }]);
  expect(changed).toBe(1);
});


test('refreshing the list cannot bless a stale review with a newer revision', async () => {
  await act(async () => { root = create(<TranscriptVersions meetingId="m" onChanged={() => { changed++; }} />); });
  await act(async () => button('Review snapshot').props.onClick());
  revision = 8; stale = true;
  await act(async () => button('Refresh versions').props.onClick());
  await act(async () => button('Save current corrections').props.onClick());
  expect(calls).toContainEqual(['select_transcript_version', { meetingId: 'm', versionId: 'refined', expectedRevision: 7 }]);
  expect(changed).toBe(0);
});
