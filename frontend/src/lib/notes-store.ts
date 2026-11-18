import { invoke } from '@tauri-apps/api/core';

export interface NotesContent { markdown: string; json: unknown[] | null }
export interface NotesDocument extends NotesContent { revision: number; updatedAt?: string }
export interface NotesSnapshot {
  loaded: boolean;
  content: NotesContent;
  state: 'loading' | 'saved' | 'pending' | 'saving' | 'error';
  error: string | null;
}
interface Entry {
  snapshot: NotesSnapshot;
  revision: number;
  generation: number;
  dirty: boolean;
  draftAlias?: string;
  draftAliasRaw?: string;
  converting: boolean;
  timer?: ReturnType<typeof setTimeout>;
  loading?: Promise<void>;
  saving?: Promise<void>;
  listeners: Set<() => void>;
}
const entries = new Map<string, Entry>();
const draftKey = (target: string) => `af-notes-v1:${target}`;
const message = (e: unknown) => e instanceof Error ? e.message : String(e);
function entryFor(target: string): Entry {
  let entry = entries.get(target);
  if (!entry) {
    entry = { snapshot: { loaded: false, content: { markdown: '', json: null }, state: 'loading', error: null }, revision: 0, generation: 0, dirty: false, converting: false, listeners: new Set() };
    entries.set(target, entry);
  }
  return entry;
}
function publish(entry: Entry, patch: Partial<NotesSnapshot>) {
  entry.snapshot = { ...entry.snapshot, ...patch };
  entry.listeners.forEach(fn => fn());
}
function journal(target: string, entry: Entry) {
  // Synchronous JSON backup precedes async markdown conversion and IPC. Each
  // session has a distinct key; a new recording never clears another draft.
  localStorage.setItem(draftKey(target), JSON.stringify({ content: entry.snapshot.content, revision: entry.revision, converting: entry.converting, inheritedAlias: entry.draftAlias ? { target: entry.draftAlias, raw: entry.draftAliasRaw } : undefined }));
  if (entry.draftAlias && localStorage.getItem(draftKey(entry.draftAlias)) === entry.draftAliasRaw) {
    localStorage.removeItem(draftKey(entry.draftAlias));
    entry.draftAlias = undefined;
    entry.draftAliasRaw = undefined;
  }
}
export const readNotesDocument = (target: string) => invoke<NotesDocument | null>('api_read_notes_document', { target });
export const currentRecordingNotesTarget = async () => {
  const folder = await invoke<string | null>('get_meeting_folder_path');
  if (!folder) throw new Error('The recording has no durable Notes identity yet. Retry after recording starts.');
  return `recording:${folder}`;
};
export const getNotesSnapshot = (target: string) => entryFor(target).snapshot;
export function subscribeNotes(target: string, listener: () => void) {
  const entry = entryFor(target);
  entry.listeners.add(listener);
  return () => { entry.listeners.delete(listener); };
}
export function loadNotes(target: string, refresh = false): Promise<void> {
  const entry = entryFor(target);
  // Transcript/version/summary refetches may ask for a refresh. The Notes
  // document owns its dirty draft (including conversion and in-flight writes),
  // not the projection being refreshed; never replace it with committed text.
  if (entry.snapshot.loaded && (!refresh || entry.dirty || entry.saving)) return Promise.resolve();
  if (entry.loading) return entry.loading;
  publish(entry, { loaded: false, state: 'loading', error: null });
  entry.loading = (async () => {
    try {
      const alias = target.startsWith('meeting:')
        ? await invoke<string | null>('api_notes_recording_target', { meetingId: target.slice(8) }) : null;
      if (alias && entries.has(alias)) await flushNotes(alias);
      const stored = await readNotesDocument(target);
      const own = localStorage.getItem(draftKey(target));
      const aliasDraft = alias ? localStorage.getItem(draftKey(alias)) : null;
      // Two independently edited journals require human reconciliation, never
      // guessing which session's changes should replace the other.
      if (own && aliasDraft) {
        const inherited = JSON.parse(own).inheritedAlias;
        if (inherited?.target !== alias || inherited?.raw !== aliasDraft) throw new Error('Two Notes recovery drafts exist. Reopen the recorder and copy its Notes before continuing.');
      }
      entry.draftAlias = aliasDraft ? alias! : undefined;
      entry.draftAliasRaw = aliasDraft ?? undefined;
      const raw = own || aliasDraft;
      const draft = raw ? JSON.parse(raw) as { content: NotesContent; revision: number; converting: boolean } : null;
      if (draft && (typeof draft.content?.markdown !== 'string' || !Number.isInteger(draft.revision))) throw new Error('Invalid Notes recovery draft. It has been retained.');
      entry.revision = draft?.revision ?? stored?.revision ?? 0;
      entry.dirty = !!draft;
      entry.converting = !!draft?.converting;
      publish(entry, { loaded: true, content: draft?.content ?? { markdown: stored?.markdown ?? '', json: stored?.json ?? null }, state: draft ? 'pending' : 'saved', error: null });
      // The editor reconverts recovered exact JSON before any write or text read.
      if (draft && !entry.converting) void flushNotes(target);
    } catch (e) {
      publish(entry, { state: 'error', error: message(e) });
    } finally { entry.loading = undefined; }
  })();
  return entry.loading;
}

/** Exact editor JSON is journaled immediately; only the latest conversion wins. */
export function editNotes(target: string, content: NotesContent, markdown: Promise<string>) {
  const entry = entryFor(target);
  const generation = ++entry.generation;
  entry.dirty = true;
  entry.converting = true;
  publish(entry, { content, state: 'pending', error: null });
  try { journal(target, entry); }
  catch (e) { publish(entry, { state: 'error', error: `Recovery backup failed: ${message(e)}` }); }
  void markdown.then(value => {
    if (generation !== entry.generation) return;
    entry.converting = false;
    publish(entry, { content: { ...content, markdown: value }, state: 'pending' });
    try { journal(target, entry); }
    catch (e) { publish(entry, { state: 'error', error: `Recovery backup failed: ${message(e)}` }); }
    clearTimeout(entry.timer);
    entry.timer = setTimeout(() => void flushNotes(target), 400);
  }).catch(e => {
    if (generation === entry.generation) publish(entry, { state: 'error', error: message(e) });
  });
}

/** One writer per identity, owned outside React; navigation does not cancel it. */
export async function flushNotes(target: string): Promise<void> {
  const entry = entryFor(target);
  clearTimeout(entry.timer);
  if (entry.saving) return entry.saving;
  if (!entry.dirty || entry.converting || !entry.snapshot.loaded) return;
  entry.saving = (async () => {
    while (entry.dirty && !entry.converting) {
      const generation = entry.generation;
      const content = entry.snapshot.content;
      publish(entry, { state: 'saving', error: null });
      try {
        const revision = await invoke<number>('api_write_notes_document', { target, ...content, expectedRevision: entry.revision });
        entry.revision = revision;
        if (generation === entry.generation) {
          localStorage.removeItem(draftKey(target));
          if (entry.draftAlias && localStorage.getItem(draftKey(entry.draftAlias)) === entry.draftAliasRaw) localStorage.removeItem(draftKey(entry.draftAlias));
          entry.draftAlias = undefined;
          entry.draftAliasRaw = undefined;
          entry.dirty = false;
          publish(entry, { state: 'saved', error: null });
        } else {
          journal(target, entry);
        }
      } catch (e) {
        publish(entry, { state: 'error', error: message(e) });
        break;
      }
    }
  })().finally(() => { entry.saving = undefined; });
  return entry.saving;
}

/** Explicit conflict resolution only: the UI shows the saved text first. The
 * reviewed revision still has to match, so another save cannot be overwritten. */
export async function saveReviewedNotes(target: string, reviewedRevision: number) {
  const entry = entryFor(target);
  if (!entry.snapshot.loaded || entry.converting) throw new Error('Notes conversion is still pending. Retry shortly.');
  entry.revision = reviewedRevision;
  entry.dirty = true;
  journal(target, entry);
  await flushNotes(target);
  if (entry.snapshot.error) throw new Error(entry.snapshot.error);
}

/** Read boundary for #13/#15/#18. Never silently return stale markdown while
 * conversion or saving failed. Native consumers can use the revisioned reader. */
export async function notesForContext(target: string): Promise<NotesDocument & { source: 'user_notes' }> {
  await loadNotes(target);
  await flushNotes(target);
  const entry = entryFor(target);
  if (!entry.snapshot.loaded || entry.dirty || entry.converting || entry.snapshot.error) throw new Error(entry.snapshot.error || 'Notes are still being saved. Retry shortly.');
  const committed = await readNotesDocument(target);
  if ((committed?.revision ?? 0) !== entry.revision) throw new Error('Notes changed in another editor. Reopen Notes before preparing context.');
  return { ...entry.snapshot.content, revision: entry.revision, source: 'user_notes' };
}
