import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';

const values = new Map<string, string>([['summaryLanguageDefault', 'ru']]);
const storage = {
  getItem: (key: string) => values.get(key) ?? null,
  setItem: (key: string, value: string) => values.set(key, value),
  removeItem: (key: string) => values.delete(key),
};
Object.assign(globalThis, { window: Object.assign(new EventTarget(), { localStorage: storage }), localStorage: storage });
let turns: any[] = [];
const calls: Array<{ command: string; args: any }> = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push({ command, args });
  if (command === 'api_ask_meeting') return 'Релиз в пятницу [12:34].';
  if (command === 'get_meeting_chat') return turns;
  if (command === 'api_get_meeting_notes') return null;
  if (command === 'api_get_summary') return { data: null };
  if (command === 'api_list_action_items') return [];
  if (command === 'api_get_meeting_summary_language') return { language: 'ru', storage: 'metadata' };
  if (command === 'send_meeting_chat') {
    const turn = { id: 'answer', meetingId: args.meetingId, question: args.question, answer: 'Релиз в пятницу [12:34].', status: 'completed', context: { startTime: 754, endTime: 754, startOffsetChars: 0, truncated: false } };
    turns = [turn]; return turn;
  }
  throw new Error(`Unexpected IPC ${command}`);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async () => () => {} }));
mock.module('@/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ meetingId: 'durable-live-id' }) }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({ children }: any) => children }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: () => null }));
mock.module('@/components/ui/tabs', () => ({
  Tabs: ({ children }: any) => <div>{children}</div>,
  TabsList: ({ children }: any) => <div>{children}</div>,
  TabsTrigger: ({ children }: any) => <div>{children}</div>,
  TabsContent: ({ children, value }: any) => value === 'ask' ? <div>{children}</div> : null,
}));
const { LivePanel } = await import('../../src/components/recording/LivePanel');
const { ChatThread } = await import('../../src/components/chat/ChatThread');
const { askMeeting } = await import('../../src/lib/workspace-api');

test('live panel sends its Russian preset and answer preference, preserving transcript and citation jump', async () => {
  const text = 'Мы решили выпустить обновление в пятницу.';
  const jumps: string[] = [];
  let view: ReturnType<typeof create>;
  await act(async () => {
    view = create(<LivePanel tab="ask" onTabChange={() => {}} speakers={[]}
      lines={[{ id: 'line-one', time: 754, speaker: 'Анна', text }]} sessionKey="ru-live-test"
      onIdentify={() => {}} onMarkMe={() => {}} onJumpTo={id => jumps.push(id)} />);
  });
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 190)); });
  const chat = view!.root.findByType(ChatThread);
  expect(chat.props.language).toBe('ru');
  expect(chat.props.emptyTitle).toBe('Спросите об этом разговоре');
  expect(chat.props.footnote).toContain('Используется модель');
  const preset = chat.props.suggestions[1];
  await act(async () => { view!.root.findAllByType('button').find(node => node.children.includes(preset))!.props.onClick(); });
  const call = calls.find(call => call.command === 'send_meeting_chat')!;
  expect(call.args.question).toBe('Какие решения были приняты?');
  expect(call.args.answerLanguage).toBe('ru');
  expect(call.args.meetingId).toBe('durable-live-id');
  expect(call.args.transcriptContext).toBeUndefined();
  act(() => view!.root.findAllByType('button').find(node => node.children.includes('12:34'))!.props.onClick());
  expect(jumps).toEqual(['line-one']);
  act(() => view!.unmount());
});

test('saved meeting API passes language alongside the unmodified question and history', async () => {
  const history = [{ question: 'Ранее?', answer: 'Ответ [00:12].' }];
  await askMeeting('meeting-id', 'Who owns this?', history, 'ru');
  expect(calls.find(call => call.command === 'api_ask_meeting')!.args).toEqual({
    meetingId: 'meeting-id', question: 'Who owns this?', history, answerLanguage: 'ru',
  });
});
