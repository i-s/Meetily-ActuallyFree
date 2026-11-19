/**
 * Keeps a meeting's action item rows in step with its AI summary.
 *
 * When a summary contains an action section, its items become rows (linked to
 * contacts by Rust) and the section is removed from the summary so the
 * meeting shows each item once. The untrimmed text is kept on the summary as
 * `original_markdown`. Summaries without a recognisable action section are
 * never modified.
 */
import { invoke } from '@tauri-apps/api/core';
import { extractActionItems, fingerprint, locateInTranscript } from '@/lib/action-items';
import { completeSummaryMarkdown, parseSummaryData } from '@/lib/summary-markdown';
import { announceChange, listUnsyncedActionMeetings, type ActionItem } from '@/lib/workspace-api';

export interface TranscriptLine {
  id: string;
  timestamp: number;
  text: string;
}

export interface SyncOutcome {
  changed: boolean;
  items?: ActionItem[];
  /** The summary as saved after its action section was moved out. */
  summary?: { markdown: string; original_markdown: string };
}

interface ActionSyncContext {
  meetingId: string;
  versionId: string;
  revision: number;
  summaryToken: string;
  summary: unknown;
  transcripts: Array<{ id: string; text: string; audio_start_time?: number | null }>;
}

export async function syncMeetingActionItems(
  meetingId: string,
  summaryData: unknown,
  // Kept for existing callers; anchors must come from the native checked context,
  // not a paginated/stale UI projection or a separate backfill read.
  _transcript?: TranscriptLine[],
  markRead = false,
): Promise<SyncOutcome> {
  const expected = parseSummaryData(summaryData);
  if (!expected || (!markRead && !extractActionItems(completeSummaryMarkdown(expected)).hadActionSection)) return { changed: false };
  const context = await invoke<ActionSyncContext | null>('api_action_sync_context', { meetingId });
  if (!context) return { changed: false };
  const data = parseSummaryData(context.summary);
  const markdown = completeSummaryMarkdown(data);
  // UI loaders may drop native metadata or normalize legacy section blocks.
  // Compare the visible projection, then extract only from the captured native
  // document. The commit still checks its full canonical content and ownership.
  if (markdown !== completeSummaryMarkdown(expected)) return { changed: false };
  const result = extractActionItems(markdown);
  if (!result.hadActionSection && !markRead) return { changed: false };
  const transcript = context.transcripts.map(line => ({ id: line.id, text: line.text, timestamp: line.audio_start_time ?? 0 }));
  const drafts = result.drafts.map((draft) => {
    const located = transcript.length ? locateInTranscript(draft.text, transcript) : null;
    return { ...draft, audioTime: located?.audioTime ?? null, transcriptId: located?.transcriptId ?? null };
  });
  const summary = result.hadActionSection ? {
    markdown: result.remainingMarkdown,
    original_markdown: typeof data?.original_markdown === 'string' ? data.original_markdown : markdown,
  } : undefined;
  const items = await invoke<ActionItem[] | null>('api_commit_action_sync', {
    context, items: drafts, sourceFingerprint: fingerprint(result.remainingMarkdown), summary: summary ?? null,
  });
  // Stale ownership is an expected no-op, including the no-section backfill.
  if (items === null) return { changed: false };
  announceChange('actions', { meetingId });
  return { changed: true, items, summary };
}

const BACKFILL_KEY = 'af-action-backfill-done';
const BACKFILL_LIMIT = 400;
/** Reads action items out of meetings summarised before they were records. */
export async function backfillActionItems(): Promise<void> {
  try {
    if (sessionStorage.getItem(BACKFILL_KEY)) return;
    sessionStorage.setItem(BACKFILL_KEY, '1');
  } catch {
    // Without session storage the backfill may simply run again next launch.
  }
  let ids: string[] = [];
  try {
    ids = await listUnsyncedActionMeetings();
  } catch (error) {
    console.warn('[actions] Could not list meetings to backfill', error);
    return;
  }
  let changed = 0;
  for (const meetingId of ids.slice(0, BACKFILL_LIMIT)) {
    try {
      const response = await invoke<{ data?: unknown }>('api_get_summary', { meetingId });
      const outcome = await syncMeetingActionItems(meetingId, response?.data, undefined, true);
      if (outcome.changed) changed += 1;
    } catch (error) {
      console.warn('[actions] Backfill skipped a meeting', meetingId, error);
    }
    await new Promise((resolve) => setTimeout(resolve, 40));
  }
  if (changed > 0) announceChange('actions');
}
