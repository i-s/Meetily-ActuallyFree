import React from 'react';
import type { MeetingDocumentProps } from '../../src/components/meeting/MeetingDocument';
import { act, create } from 'react-test-renderer';
import { expect, mock, test } from 'bun:test';
mock.module('@/components/recording/LiveMeetingSummary', () => ({ useLiveMeetingSummary: () => ({markdown: '', items: []}), LiveMeetingSummary: () => null }));
mock.module('@/components/ui/tooltip', () => ({ Hint: ({children}: any) => children }));
mock.module('@/components/ui/tabs', () => ({ Tabs: ({children}: any) => <div>{children}</div>, TabsList: ({children}: any) => <div>{children}</div>, TabsTrigger: ({children, ...props}: any) => <button {...props}>{children}</button>, TabsContent: ({children}: any) => <div>{children}</div> }));
mock.module('@/hooks/useUserName', () => ({ useUserName: () => '' }));
mock.module('@/hooks/useAskAiLanguage', () => ({ useAskAiLanguage: () => ({ language: 'en', ready: true }) }));
mock.module('@/contexts/RecordingStateContext', () => ({ useRecordingState: () => ({ meetingId: 'native-durable-id' }) }));
mock.module('@/components/meeting/MeetingNotesEditor', () => ({ MeetingNotesEditor: ({meetingId}: any) => <div data-notes-id={meetingId} />, useMeetingNotes: () => ({ loaded: true, content: { markdown: 'Notes without speech', json: null }, state: 'saved' }) }));
mock.module('@/components/chat/MeetingChat', () => ({ MeetingChat: ({meetingId}: any) => <div data-chat-id={meetingId} /> }));
mock.module('@/components/editor/NotesEditor', () => ({ NotesEditor: () => null }));
const { LivePanel } = await import('../../src/components/recording/LivePanel');
test('live notes and chat use durable native identity and retain all four tabs', async () => {
  let view: ReturnType<typeof create>;
  await act(async () => { view = create(<LivePanel tab="notes" onTabChange={() => {}} speakers={[]} lines={[]} sessionKey="temporary-ui-id" onIdentify={() => {}} onMarkMe={() => {}} onJumpTo={() => {}} />); });
  expect(view!.root.findByProps({'data-notes-id': 'native-durable-id'})).toBeDefined();
  expect(view!.root.findByProps({'data-chat-id': 'native-durable-id'})).toBeDefined();
  const text = JSON.stringify(view!.toJSON());
  for (const label of ['Meeting notes', 'Summary', 'Ask AI', 'Speakers']) expect(text).toContain(label);
  act(() => view!.unmount());
});


mock.module('@/components/actions/ActionItemsList', () => ({ ActionItemsList: () => <div>Tasks</div> }));
mock.module('@/lib/meeting-notes-store', () => ({ flushMeetingNotes: async () => {} }));
mock.module('@/lib/action-item-sync', () => ({ syncMeetingActionItems: async () => ({}) }));
mock.module('@/lib/workspace-api', () => ({ announceChange: () => {}, askMeeting: async () => '', listActionItems: async () => [], onWorkspaceChange: () => () => {}, getSummaryInputSnapshot: async () => null, getMeetingNotes: async () => ({markdown: 'Notes without speech'}), saveMeetingNotes: async () => {} }));
mock.module('@/lib/summary-language-preferences', () => ({ readMeetingSummaryLanguage: async () => ({ language: null }), saveMeetingSummaryLanguage: async () => ({ language: null }) }));
mock.module('@/components/ModelSettingsModal', () => ({ ModelSettingsModal: () => null }));
const { MeetingDocument } = await import('../../src/components/meeting/MeetingDocument');
test('saved view separates notes and summary, permits notes-only generation', async () => {
  let generated = false;
  let view: ReturnType<typeof create>;
  const props: MeetingDocumentProps = { meetingId: 'saved', aiSummary: null, summaryUserEdited: false, onSummaryChange: () => {}, summaryStatus: 'idle' as const, summaryError: null, statusMessage: () => '', onGenerate: () => { generated = true; }, onRegenerate: async () => {}, onStop: () => {}, hasTranscript: false, transcript: [], onSeek: () => {}, modelConfig: {provider:'ollama', model: 'model', whisperModel: 'base'}, setModelConfig: () => {}, onSaveModelConfig: async () => {}, templates: [], selectedTemplate: '', onTemplateSelect: () => {}, onManageTemplates: () => {}, regenerateRequest: null, onRegenerateRequestHandled: () => {} };
  await act(async () => { view = create(<MeetingDocument {...props} />); });
  expect(view!.root.findByProps({'data-notes-id': 'saved'})).toBeDefined();
  const summary = view!.root.findAllByType('button').find(node => node.props.value === 'summary');
  expect(summary).toBeDefined();
  const generate = view!.root.findAllByType('button').find(node => node.children.includes('Generate summary'));
  expect(generate).toBeDefined();
  await act(async () => { generate!.props.onClick(); });
  expect(generated).toBe(true);
  act(() => view!.unmount());
});
