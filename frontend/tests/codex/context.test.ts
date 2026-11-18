import { expect, test } from 'bun:test';
import { snapshotLiveQuestion } from '../../src/lib/live-codex';

const notes = { source: 'user_notes' as const, markdown: 'My idea: ask about budget. Not agreed.', json: null, revision: 7 };
const lines = [
  { id: 'early', sequenceId: 1, time: 12, endTime: 15, text: 'The launch budget is 42.', speaker: 'Speaker 1' },
  { id: 'outside', sequenceId: 2, time: 299.9, endTime: 300, text: 'Outside the five minute interval.' },
  { id: 'boundary', sequenceId: 3, time: 300, endTime: 301, text: 'At the boundary.' },
  { id: 'now', sequenceId: 4, time: 599, endTime: 600, text: 'Latest complete speech.' },
  { id: 'future', sequenceId: 5, time: 600, endTime: 601, text: 'Not yet complete at request time.' },
];
test('lookback uses request recording clock, not the last ASR line, and includes the exact five-minute boundary', () => {
  const result = snapshotLiveQuestion('recording:/owned/current', lines, 600, 'missed', '', notes);
  expect(result.lines.map(line => line.id)).toEqual(['boundary', 'now']);
  expect(result.asOfSeconds).toBe(600);
  expect(result.fromSeconds).toBe(300);
  expect(result.userNotes).toEqual({ source: 'user_notes', markdown: notes.markdown, revision: 7 });
});
test('arbitrary question retains early evidence in a long meeting without recency trimming', () => {
  const long = [...lines.slice(0, 1), ...Array.from({ length: 700 }, (_, i) => ({ id: `late-${i}`, sequenceId: i + 10, time: 100 + i, endTime: 101 + i, text: 'Later unrelated Russian English speech '.repeat(5) }))];
  const result = snapshotLiveQuestion('recording:/owned/current', long, 5400, 'question', 'What was the launch budget?', notes);
  expect(result.lines[0].text).toBe('The launch budget is 42.');
  expect(result.lines.length).toBe(701);
});
test('snapshot is detached; delayed ASR and later Notes edits never enter an already requested answer', () => {
  const input = structuredClone(lines);
  const personal = structuredClone(notes);
  const result = snapshotLiveQuestion('recording:/owned/current', input, 90, 'missed', '', personal);
  input[0].text = 'mutated';
  input.push({ id: 'delayed', sequenceId: 99, time: 20, endTime: 21, text: 'Arrived after click' });
  personal.markdown = 'changed after click';
  expect(result.lines.map(line => line.id)).toEqual(['early']);
  expect(result.lines[0].text).toContain('42');
  expect(result.userNotes.markdown).toContain('Not agreed');
  expect(result.fromSeconds).toBe(0);
});
test('silence after old ASR does not silently shift a five-minute window backwards', () => {
  const result = snapshotLiveQuestion('recording:/owned/current', lines.slice(0, 1), 900, 'missed', '', notes);
  expect(result.lines).toEqual([]);
  expect(result.fromSeconds).toBe(600);
});
test('invalid timing and oversize input fail explicitly instead of fabricating timestamps or silently omitting context', () => {
  expect(() => snapshotLiveQuestion('recording:/owned/current', [{ id: 'unknown', text: 'timing missing', time: NaN }], 30, 'question', 'what?', notes)).toThrow();
  expect(() => snapshotLiveQuestion('meeting:other', lines, 600, 'question', 'what?', notes)).toThrow();
  expect(() => snapshotLiveQuestion('recording:/owned/current', [{ ...lines[0], text: 'a'.repeat(600_000) }], 600, 'question', 'what?', notes)).toThrow();
});
