import React from 'react';
import {act, create, ReactTestRenderer} from 'react-test-renderer';
import {afterEach, beforeEach, expect, mock, test} from 'bun:test';
let calls: string[] = [];
let flush: (id: string) => Promise<void>;
mock.module('@/lib/meeting-notes-store', () => ({flushMeetingNotes: (id: string) => flush(id)}));
const startSummaryPolling = () => {};
const stopSummaryPolling = () => {};
mock.module('@/components/Sidebar/SidebarProvider', () => ({useSidebar: () => ({startSummaryPolling, stopSummaryPolling})}));
mock.module('@tauri-apps/api/core', () => ({invoke: async (command: string) => {
  calls.push(command);
  if (command === 'api_get_summary') return {status: 'idle', data: null};
  if (command === 'api_get_meeting_transcripts') return {transcripts: [], total_count: 0, has_more: false};
  if (command === 'api_process_transcript') return {process_id: 'run'};
  throw new Error(`Unexpected command: ${command}`);
}}));
mock.module('@tauri-apps/api/event', () => ({listen: async () => () => {}}));
mock.module('@/lib/summary-language-preferences', () => ({readMeetingSummaryLanguage: async () => ({language: null}), readCachedDetectedSummaryLanguage: async () => null, detectAndCacheSummaryLanguage: async () => ({language: null})}));
mock.module('sonner', () => ({toast: {info: () => {}, error: () => {}, warning: () => {}, success: () => {}}}));
mock.module('@/lib/analytics', () => ({default: {trackSummaryGenerationStarted: async () => {}, trackSummaryGenerationCompleted: async () => {}}}));
const {useSummaryGeneration} = await import('../../src/hooks/meeting-details/useSummaryGeneration');
let current: ReturnType<typeof useSummaryGeneration>;
const roots: ReactTestRenderer[] = [];
function Probe({id = 'notes-only'}: {id?: string}) {
  current = useSummaryGeneration({meeting: {id, created_at: '2026-10-26'}, transcripts: [], modelConfig: {provider: 'claude', model: 'claude-sonnet-4-5', whisperModel: 'base'}, isModelConfigLoading: false, selectedTemplate: 'standard', setAiSummary: () => {}});
  return null;
}
async function mount() { let root!: ReactTestRenderer; await act(async () => {root = create(<Probe/>);}); roots.push(root); return root; }
beforeEach(() => {calls = []; flush = async id => {calls.push(`flush:${id}`);};});
afterEach(async () => {await act(async () => {roots.splice(0).forEach(root => root.unmount());});});
for (const regenerate of [false, true]) {
  test(`notes-only ${regenerate ? 'regeneration' : 'generation'} waits for durable notes before native snapshot`, async () => {
    let release!: () => void;
    flush = async id => {calls.push(`flush:${id}`); await new Promise<void>(resolve => {release = resolve;});};
    await mount();
    let request!: Promise<unknown>;
    await act(async () => {request = regenerate ? current.handleRegenerateSummary() : current.handleGenerateSummary();});
    expect(calls).toContain('flush:notes-only');
    expect(calls).not.toContain('api_process_transcript');
    await act(async () => {release(); await request;});
    expect(calls).toContain('api_process_transcript');
    expect(calls).not.toContain('api_get_meeting_transcripts');
  });
}
test('failed notes save blocks snapshot and exposes error', async () => {
  flush = async () => {throw new Error('Notes could not be saved');};
  await mount();
  await act(async () => {expect(await current.handleGenerateSummary()).toBe(false);});
  expect(calls).not.toContain('api_process_transcript');
  expect(current.summaryError).toContain('Notes could not be saved');
});
test('navigation during notes flush does not start a summary for the next meeting', async () => {
  let release!: () => void;
  flush = async () => {await new Promise<void>(resolve => {release = resolve;});};
  const root = await mount();
  let request!: Promise<unknown>;
  await act(async () => {request = current.handleGenerateSummary();});
  await act(async () => {root.update(<Probe id="other"/>);});
  await act(async () => {release(); await request;});
  expect(calls).not.toContain('api_process_transcript');
});
