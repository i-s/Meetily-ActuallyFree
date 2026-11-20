import React from 'react';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
let response: any = null;
let event: (() => void) | undefined;
Object.assign(globalThis, { window: new EventTarget() });
mock.module('@tauri-apps/api/core', () => ({ invoke: async (command: string) => {
  if (command === 'api_get_summary') return { data: response };
  if (command === 'api_list_action_items') return [{ id: 'task', text: 'Send proposal', done: false }];
  throw new Error(command);
} }));
mock.module('@tauri-apps/api/event', () => ({ listen: async (_name: string, callback: () => void) => { event = callback; return () => {}; } }));
const { useLiveMeetingSummary, LiveMeetingSummary } = await import('../../src/components/recording/LiveMeetingSummary');
function View({ meetingId }: {meetingId: string}) {
  const summary = useLiveMeetingSummary(meetingId);
  return <><button disabled={!summary.markdown}>Summary</button><LiveMeetingSummary {...summary} /></>;
}
test('live Summary becomes available when the same meeting finishes generation', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<View meetingId="live" />); });
  expect(view!.root.findAllByType('button')[0].props.disabled).toBe(true);
  response = { markdown: 'Ship Friday' };
  await act(async () => { event!(); });
  expect(view!.root.findAllByType('button')[0].props.disabled).toBe(false);
  expect(JSON.stringify(view!.toJSON())).toContain('Ship Friday');
  expect(JSON.stringify(view!.toJSON())).toContain('Send proposal');
  act(() => view!.unmount());
});
