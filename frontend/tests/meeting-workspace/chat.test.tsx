import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const context = { settings: { mode: 'recent', maxChars: 10000 }, totalChars: 12000, includedChars: 10000, startChar: 2000, endChar: 12000, startTime: 754, endTime: 900, truncated: true, startOffsetChars: 2, historyTurnsIncluded: 20, historyTurnsTotal: 24 };
const rows: Record<string, any[]> = { first: [{ id: 'one', meetingId: 'first', question: 'Decisions?', answer: 'Friday [12:34]', status: 'completed', context }], second: [] };
const calls: any[] = [];
let failSend = false;
let chatEvent: ((event: any) => void) | undefined;
Object.assign(globalThis, { window: new EventTarget(), localStorage: { getItem: () => null, setItem: () => {}, removeItem: () => {} } });
mock.module('@tauri-apps/api/event', () => ({ listen: async (_name: string, callback: any) => { chatEvent = callback; return () => {}; } }));
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  calls.push({ command, args });
  if (command === 'get_meeting_chat') return rows[args.meetingId] ?? [];
  if (command === 'api_get_meeting_notes') return null;
  if (command === 'send_meeting_chat') { if (failSend) throw new Error('Provider unavailable'); const turn = { id: 'new', meetingId: args.meetingId, question: args.question, answer: null, status: 'pending', context }; rows[args.meetingId].push(turn); return turn; }
  throw new Error(command);
} }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({children}: any) => children }));
const { MeetingChat } = await import('../../src/components/chat/MeetingChat');

test('reopens persisted conversation with its actual range and clears visible history on meeting switch', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<MeetingChat meetingId="first" language="en" />); });
  const rendered = JSON.stringify(view!.toJSON());
  expect(rendered).toContain('Friday');
  expect(rendered).toContain('12:34');
  expect(rendered).toContain('15:00');
  expect(rendered).toContain('20 of 24 previous answers');
  await act(async () => { view!.update(<MeetingChat meetingId="second" language="en" />); });
  expect(JSON.stringify(view!.toJSON())).not.toContain('Friday');
  const suggestion = view!.root.findAllByType('button').find(node => node.children.includes('What has been decided?'))!;
  await act(async () => { suggestion.props.onClick(); });
  expect(calls.find(call => call.command === 'send_meeting_chat').args).toEqual({ meetingId: 'second', question: 'What has been decided?', answerLanguage: 'en' });
  expect(JSON.stringify(view!.toJSON())).toContain('Reading the records');
  act(() => view!.unmount());
});


test('request errors survive background history refresh so retry remains actionable', async () => {
  rows.failure = []; failSend = true;
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<MeetingChat meetingId="failure" language="en" />); });
  await act(async () => { view!.root.findAllByType('button').find(node => node.children.includes('What has been decided?'))!.props.onClick(); });
  expect(JSON.stringify(view!.toJSON())).toContain('Provider unavailable');
  await act(async () => { chatEvent!({ payload: { meetingId: 'failure' } }); });
  expect(JSON.stringify(view!.toJSON())).toContain('Provider unavailable');
  act(() => view!.unmount()); failSend = false;
});

test('untimed transcript remains visible as characters instead of claiming no transcript', async () => {
  rows.untimed = [{ ...rows.first[0], context: { ...context, startTime: null, endTime: null } }];
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<MeetingChat meetingId="untimed" language="en" />); });
  const text = JSON.stringify(view!.toJSON());
  expect(text).toContain('timestamps unavailable');
  expect(text).toContain('2000–12000');
  expect(text).not.toContain('No transcript');
  act(() => view!.unmount());
});
