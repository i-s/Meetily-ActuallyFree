import React, { useEffect } from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
const offsets: number[] = [];
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string, args: any) => {
  if (command === 'api_get_meeting_metadata') return { id: args.meetingId, title: 'Large', created_at: '2026-11-18' };
  if (command === 'api_get_meeting_transcripts') {
    offsets.push(args.offset);
    return { total_count: 4501, transcripts: Array.from({ length: Math.min(args.limit,4501-args.offset) }, (_, i) => ({ id: `t-${args.offset+i}`, text: 'speech', timestamp: '00:00', audio_start_time: args.offset+i })), has_more: args.offset+args.limit<4501 };
  }
  throw new Error(command);
} }));
const { usePaginatedTranscripts } = await import('../../src/hooks/usePaginatedTranscripts');
let loaded = 0;
function Harness() {
  const page = usePaginatedTranscripts({ meetingId: 'large' });
  loaded = page.loadedCount;
  useEffect(() => {
    if (!page.isLoading && !page.isLoadingMore && page.hasMore && !page.transcripts.some(t => t.id === 't-4500')) void page.loadMore();
  }, [page.isLoading, page.isLoadingMore, page.hasMore, page.transcripts, page.loadMore]);
  return <p>{page.loadedCount}</p>;
}
test('normal serialized pagination can reach a search hit after 45 pages without a debounce dead end', async () => {
  let root: any;
  await act(async () => { root = create(<Harness />); });
  expect(loaded).toBe(4501);
  expect(offsets.at(-1)).toBe(4500);
  await act(async () => root.unmount());
});
