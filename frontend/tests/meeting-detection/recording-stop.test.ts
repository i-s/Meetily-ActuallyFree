import { beforeEach, expect, test } from 'bun:test';
import { consumeRecordingStopRequest, requestRecordingStop, STOP_REQUEST_KEY } from '../../src/lib/recording-launch';

const storage = new Map<string, string>();
const events = new EventTarget();
const location = { pathname: '/' };
Object.assign(events, { location });
Object.assign(globalThis, { window: events, sessionStorage: {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
} });
beforeEach(() => { storage.clear(); location.pathname = '/'; });

test('a stop arriving before recorder readiness stays pending and runs exactly once when ready', () => {
  let ready = false;
  let stops = 0;
  const consume = () => consumeRecordingStopRequest(ready, () => { stops++; });
  events.addEventListener('af-stop-recording', consume);
  try {
    requestRecordingStop(() => { throw new Error('Already on recorder'); });
    expect(stops).toBe(0);
    expect(storage.get(STOP_REQUEST_KEY)).toBe('1');
    consume();
    expect(storage.get(STOP_REQUEST_KEY)).toBe('1');
    ready = true;
    consume();
    consume();
    expect(stops).toBe(1);
    expect(storage.has(STOP_REQUEST_KEY)).toBe(false);
  } finally { events.removeEventListener('af-stop-recording', consume); }
});

test('a stop after navigation persists until the recorder mounts ready', () => {
  location.pathname = '/home';
  const routes: string[] = [];
  requestRecordingStop(route => routes.push(route));
  expect(routes).toEqual(['/']);
  expect(storage.get(STOP_REQUEST_KEY)).toBe('1');
  let stops = 0;
  consumeRecordingStopRequest(false, () => { stops++; });
  expect(stops).toBe(0);
  consumeRecordingStopRequest(true, () => { stops++; });
  expect(stops).toBe(1);
});

test('a duplicate request during stopping is satisfied and cannot stop the next manual recording', () => {
  let stops = 0;
  requestRecordingStop(() => {});
  consumeRecordingStopRequest(true, () => { stops++; });
  expect(stops).toBe(1);
  requestRecordingStop(() => {});
  consumeRecordingStopRequest(false, () => { stops++; }, true);
  expect(storage.has(STOP_REQUEST_KEY)).toBe(false);
  // The previous recording ends, then a new manual recording becomes ready.
  consumeRecordingStopRequest(false, () => { stops++; });
  consumeRecordingStopRequest(true, () => { stops++; });
  expect(stops).toBe(1);
});
