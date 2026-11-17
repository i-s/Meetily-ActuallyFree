import { expect, test } from 'bun:test';
import { transcriptWithAskBoundary, askBoundaryLabel } from '../../src/lib/ask-ai-context';

const range = { truncated: true, startTranscriptId: 'b', startOffsetChars: 2, startTime: 754 };
const segments = [
  { id: 'a', text: 'Earlier', timestamp: 700, speaker: 'You' },
  { id: 'b', text: 'A😀БВ', timestamp: 754, speaker: 'You' },
  { id: 'c', text: 'Later', timestamp: 760, speaker: 'You' },
];

test('boundary preserves all text and splits at Unicode scalar offset inside a turn', () => {
  const shown = transcriptWithAskBoundary(segments, range);
  expect(shown.map(s => s.text).join('')).toBe(segments.map(s => s.text).join(''));
  expect(shown.map(s => s.id)).toEqual(['a', 'b', 'b:ask-context', 'c']);
  expect(shown[1].text).toBe('A😀');
  expect(shown[2].text).toBe('БВ');
  expect(shown[2].askBoundary).toContain('12:34');
  expect(shown[2].timestamp).toBe(754);
  expect(shown[1].preventMergeAfter).toBe(true);
  expect(segments[1].text).toBe('A😀БВ');
});

test('whole transcript has no marker; boundary at row start does not add empty rows', () => {
  expect(transcriptWithAskBoundary(segments, { ...range, truncated: false })).toEqual(segments);
  const shown = transcriptWithAskBoundary(segments, { ...range, startOffsetChars: 0 });
  expect(shown).toHaveLength(3);
  expect(shown[1].askBoundary).toBeDefined();
});

test('unloaded boundary never marks a different paginated row and unknown time is honest', () => {
  expect(transcriptWithAskBoundary(segments.slice(0, 1), range)).toEqual(segments.slice(0, 1));
  expect(askBoundaryLabel({ ...range, startTime: null })).toContain('отмеченного фрагмента');
});
