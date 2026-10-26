import { getMeetingNotes, saveMeetingNotes, announceChange } from '@/lib/workspace-api';

export interface MeetingNotesContent { json: unknown[] | null; markdown: string; }
export interface MeetingNotesSnapshot {
  loaded: boolean;
  content: MeetingNotesContent;
  state: 'loading' | 'saved' | 'pending' | 'saving' | 'error';
  error: string | null;
}
interface Entry {
  deleted?: boolean;
  snapshot: MeetingNotesSnapshot;
  revision: number;
  savedRevision: number;
  timer?: ReturnType<typeof setTimeout>;
  loading?: Promise<void>;
  saving?: Promise<void>;
  conversion?: Promise<void>;
  needsConversion?: boolean;
  listeners: Set<() => void>;
}
const entries = new Map<string, Entry>();
const keyFor = (id: string) => `af-meeting-notes-draft:${id}`;
function entryFor(id: string): Entry {
  let entry = entries.get(id);
  if (!entry) {
    entry = { snapshot: { loaded: false, content: { json: null, markdown: '' }, state: 'loading', error: null }, revision: 0, savedRevision: 0, listeners: new Set() };
    entries.set(id, entry);
  }
  return entry;
}
function publish(entry: Entry, patch: Partial<MeetingNotesSnapshot>) {
  entry.snapshot = { ...entry.snapshot, ...patch };
  entry.listeners.forEach(listener => listener());
}
function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }
function persistDraft(id: string, entry: Entry) {
  if (entry.deleted) return;
  localStorage.setItem(keyFor(id), JSON.stringify({ ...entry.snapshot.content, needsConversion: !!entry.needsConversion }));
}
export function getMeetingNotesSnapshot(id: string) { return entryFor(id).snapshot; }
export function subscribeMeetingNotes(id: string, listener: () => void) {
  const entry = entryFor(id); entry.listeners.add(listener);
  return () => { entry.listeners.delete(listener); };
}
export async function loadMeetingNotes(id: string): Promise<void> {
  const entry = entryFor(id);
  if (entry.deleted) throw new Error('This meeting was deleted.');
  if (entry.snapshot.loaded) return;
  if (entry.loading) return entry.loading;
  entry.loading = (async () => {
    try {
      // A failed database read is never treated as an empty document.
      const saved = await getMeetingNotes(id);
      if (entry.deleted) throw new Error('This meeting was deleted.');
      const raw = localStorage.getItem(keyFor(id));
      const draft = raw ? JSON.parse(raw) as MeetingNotesContent & { needsConversion?: boolean } : null;
      entry.needsConversion = draft?.needsConversion;
      if (draft) entry.revision++;
      publish(entry, { loaded: true, content: draft ? { json: draft.json, markdown: draft.markdown } : { json: saved?.json ?? null, markdown: saved?.markdown ?? '' }, state: draft ? 'pending' : 'saved', error: null });
      if (draft && !entry.needsConversion) schedule(id, entry);
    } catch (error) {
      publish(entry, { state: 'error', error: errorText(error) });
      throw error;
    }
  })().finally(() => { entry.loading = undefined; });
  return entry.loading;
}
function schedule(id: string, entry: Entry) {
  clearTimeout(entry.timer);
  entry.timer = setTimeout(() => { void flushMeetingNotes(id).catch(() => {}); }, 400);
}
/** The JSON draft is synchronous; conversion and database writes belong to this
 * module, so unmounting or changing meetings cannot cancel or retarget a save. */
export function updateMeetingNotes(id: string, content: MeetingNotesContent, markdown?: Promise<string>) {
  const entry = entryFor(id);
  if (entry.deleted) return;
  const revision = ++entry.revision;
  entry.needsConversion = !!markdown;
  publish(entry, { loaded: true, content, state: 'pending', error: null });
  try { persistDraft(id, entry); }
  catch (error) { publish(entry, { state: 'error', error: `Local recovery draft could not be saved: ${errorText(error)}` }); }
  if (markdown) {
    entry.conversion = markdown.then(value => {
      if (entry.revision !== revision) return;
      entry.needsConversion = false;
      publish(entry, { content: { ...content, markdown: value } });
      try { persistDraft(id, entry); } catch { /* The SQLite save below can still succeed. */ }
    }).catch(error => {
      if (entry.revision === revision) publish(entry, { state: 'error', error: errorText(error) });
      throw error;
    });
    // Keep a rejection observed even before the debounce/explicit flush runs.
    void entry.conversion.catch(() => {});
  } else entry.conversion = undefined;
  schedule(id, entry);
}
export async function flushMeetingNotes(id: string): Promise<void> {
  const entry = entryFor(id);
  clearTimeout(entry.timer);
  await loadMeetingNotes(id);
  if (entry.saving) { await entry.saving; if (entry.savedRevision !== entry.revision) return flushMeetingNotes(id); return; }
  entry.saving = (async () => {
    try {
      while (entry.savedRevision !== entry.revision) {
        if (entry.deleted) throw new Error('This meeting was deleted.');
        const revision = entry.revision;
        if (entry.conversion) await entry.conversion;
        if (entry.deleted) throw new Error('This meeting was deleted.');
        if (revision !== entry.revision) continue;
        if (entry.needsConversion) throw new Error('Open Meeting notes to restore the latest editor draft before continuing.');
        const content = entry.snapshot.content;
        publish(entry, { state: 'saving', error: null });
        await saveMeetingNotes(id, content.markdown, content.json);
        entry.savedRevision = revision;
        if (entry.revision === revision) {
          try { localStorage.removeItem(keyFor(id)); } catch { /* Database already contains this revision. */ }
          publish(entry, { state: 'saved', error: null });
        }
        announceChange('notes', { meetingId: id });
      }
    } catch (error) {
      publish(entry, { state: 'error', error: errorText(error) });
      throw error;
    }
  })().finally(() => { entry.saving = undefined; });
  return entry.saving;
}

/** Called after native deletion succeeds. Tombstones reject callbacks retained by
 * mounted editors; native foreign keys reject any already-dispatched database write. */
export function forgetMeetingNotes(id: string) {
  const entry = entryFor(id);
  entry.deleted = true;
  entry.revision++;
  clearTimeout(entry.timer);
  try { localStorage.removeItem(keyFor(id)); } catch { /* The native meeting is gone. */ }
  publish(entry, { loaded: false, content: { json: null, markdown: '' }, state: 'error', error: 'This meeting was deleted.' });
}
