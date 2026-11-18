import { beforeEach, expect, mock, test } from 'bun:test';
const store = new Map<string, string>();
const events: CustomEvent[] = [];
const location = { pathname: '/' };
Object.assign(globalThis, {
  sessionStorage: { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => store.set(k, v), removeItem: (k: string) => store.delete(k) },
  window: { location, dispatchEvent: (e: CustomEvent) => events.push(e) },
  CustomEvent: class extends Event { detail: unknown; constructor(name: string, init?: CustomEventInit) { super(name); this.detail = init?.detail; } },
});
mock.module('@/lib/groups', () => ({ writePendingGroup: () => {} }));
const { markAutomatedStart, automatedStartInProgress, endAutomatedRecording, takeAutomatedStartResult } = await import('../../src/lib/meeting-automation');
const { launchRecording, readRecordingLaunchIntent, takeRecordingLaunchIntent, AUTO_START_KEY } = await import('../../src/lib/recording-launch');
const zoom = { app: 'Zoom', process: 'us.zoom.xos', source: 'zoom', sessionId: 'session-A' };
beforeEach(() => { store.clear(); events.length = 0; location.pathname = '/'; });
test('manual event and routed manual button stay explicitly manual, absence stays absent', () => {
  expect(readRecordingLaunchIntent(undefined)).toEqual({ kind: 'absent' });
  launchRecording(() => {});
  expect(readRecordingLaunchIntent(events[0].detail)).toEqual({ kind: 'manual' });
  location.pathname = '/library'; const routes: string[] = [];
  launchRecording((href) => routes.push(href));
  expect(routes).toEqual(['/']); expect(takeRecordingLaunchIntent()).toEqual({ kind: 'manual' });
  expect(takeRecordingLaunchIntent()).toEqual({ kind: 'absent' });
});
test('automatic event and navigation/reload keep exact original token, never substitute a new owner', async () => {
  markAutomatedStart(zoom); const a = automatedStartInProgress()!;
  launchRecording(() => {}, { automated: a });
  expect(readRecordingLaunchIntent(events[0].detail)).toEqual({ kind: 'automated', call: a });
  location.pathname = '/library'; launchRecording(() => {}, { automated: a });
  const reloadedPath = '../../src/lib/recording-launch.ts?reload-intent';
  const reloaded = await import(reloadedPath);
  expect(reloaded.takeRecordingLaunchIntent()).toEqual({ kind: 'automated', call: a });
  endAutomatedRecording(); markAutomatedStart({ ...zoom, sessionId: 'session-B' }); const b = automatedStartInProgress()!;
  expect(readRecordingLaunchIntent(events[0].detail).kind).toBe('rejected');
  expect(takeAutomatedStartResult(a).kind).toBe('rejected');
  expect(takeAutomatedStartResult(b).kind).toBe('accepted');
});
test('malformed/absent/expired automation payload cannot become manual through event or route', () => {
  for (const payload of [{}, '', 'true', '{}', 'null', [], { kind: 'automated' }, { kind: 'automated', call: {} }]) {
    expect(readRecordingLaunchIntent(payload).kind).toBe('rejected');
    store.set(AUTO_START_KEY, typeof payload === 'string' ? payload : JSON.stringify(payload));
    expect(takeRecordingLaunchIntent().kind).toBe('rejected');
  }
  launchRecording(() => {}, { automated: undefined });
  expect(readRecordingLaunchIntent(events.at(-1)!.detail).kind).toBe('rejected');
  markAutomatedStart(zoom); const a = automatedStartInProgress()!;
  const realNow = Date.now;
  try {
    Date.now = () => a.requestedAt! + 60_001;
    expect(readRecordingLaunchIntent({ kind: 'automated', call: a }).kind).toBe('rejected');
    endAutomatedRecording();
    expect(readRecordingLaunchIntent({ kind: 'automated', call: a }).kind).toBe('rejected');
  } finally { Date.now = realNow; }
});
