import React from 'react';
import { act, create } from 'react-test-renderer';
import { beforeEach, expect, mock, test } from 'bun:test';

const storage = new Map<string, string>();
const target = new EventTarget();
Object.assign(globalThis, { window: Object.assign(target, { localStorage: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
} }) });
let explicit: string | null = null;
let cached: string | null = null;
let detected: string | null = 'ru';
let failDetection = false;
let receivedTexts: string[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_get_meeting_summary_language') return { language: explicit, storage: 'metadata' };
  if (command === 'api_save_meeting_summary_language') { explicit = args.summaryLanguage; return { language: explicit, storage: 'metadata' }; }
  if (command === 'api_get_meeting_detected_summary_language') return { language: cached, storage: 'metadata' };
  if (command === 'api_detect_transcript_summary_language') {
    if (failDetection) throw new Error('detector unavailable');
    receivedTexts = args.transcriptTexts; return { language: detected, reason: detected ? 'detected' : 'empty' };
  }
  throw new Error(`Unexpected IPC ${command}`);
} }));
const { resolveAskAiLanguage, askAiCopy } = await import('../../src/lib/ask-ai-language');
const { useAskAiLanguage } = await import('../../src/hooks/useAskAiLanguage');
const { saveMeetingSummaryLanguage, writePinnedSummaryLanguageDefault } = await import('../../src/lib/summary-language-preferences');
const transcript = ['Мы обсудили сроки проекта и решили выпустить обновление в пятницу.'];

beforeEach(() => { storage.clear(); explicit = null; cached = null; detected = 'ru'; failDetection = false; receivedTexts = []; });

test('saved meeting explicit choice beats cached detection and global pin', async () => {
  explicit = 'en'; cached = 'ru'; storage.set('summaryLanguageDefault', 'de');
  expect(await resolveAskAiLanguage('meeting', transcript)).toBe('en');
  expect(receivedTexts).toEqual([]);
});

test('saved Auto uses transcript detection even when new meetings are pinned English', async () => {
  storage.set('summaryLanguageDefault', 'en');
  expect(await resolveAskAiLanguage('meeting', transcript)).toBe('ru');
  expect(receivedTexts).toEqual(transcript);
  expect(askAiCopy('ru').meeting.suggestions[0]).toBe('Что было решено?');
});

test('live pin beats detection; Auto detects the current transcript', async () => {
  storage.set('summaryLanguageDefault', 'en');
  expect(await resolveAskAiLanguage(undefined, transcript)).toBe('en');
  storage.clear();
  expect(await resolveAskAiLanguage(undefined, transcript)).toBe('ru');
});

test('saved cached detection is reused; unknown/failed detection safely falls back to English', async () => {
  cached = 'fr';
  expect(await resolveAskAiLanguage('meeting', transcript)).toBe('fr');
  cached = null; detected = null;
  expect(await resolveAskAiLanguage('meeting', [])).toBe('en');
  failDetection = true;
  expect(await resolveAskAiLanguage(undefined, transcript)).toBe('en');
});

test('regional Russian gets Russian copy; other output languages retain English copy', () => {
  expect(askAiCopy('ru-RU').tab).toBe('Спросить ИИ');
  expect(askAiCopy('de').tab).toBe('Ask AI');
});

function View({ meetingId }: { meetingId?: string }) {
  const { language } = useAskAiLanguage(meetingId, transcript.map(text => ({ text })));
  return <span>{language}</span>;
}
const settle = () => new Promise(resolve => setTimeout(resolve, 190));

test('the open meeting refreshes after summary language is saved, including switching to Auto', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<View meetingId="meeting" />); await settle(); });
  expect(view!.root.findByType('span').children).toEqual(['ru']);
  await act(async () => { await saveMeetingSummaryLanguage('meeting', 'en'); });
  await act(async () => { await settle(); });
  expect(view!.root.findByType('span').children).toEqual(['en']);
  await act(async () => { await saveMeetingSummaryLanguage('meeting', null); });
  await act(async () => { await settle(); });
  expect(view!.root.findByType('span').children).toEqual(['ru']);
  act(() => view!.unmount());
});

test('live view refreshes when the pinned default changes', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<View />); await settle(); });
  expect(view!.root.findByType('span').children).toEqual(['ru']);
  act(() => writePinnedSummaryLanguageDefault('en'));
  await act(async () => { await settle(); });
  expect(view!.root.findByType('span').children).toEqual(['en']);
  act(() => view!.unmount());
});
