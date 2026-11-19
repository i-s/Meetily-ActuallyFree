import { invoke } from '@tauri-apps/api/core';
import type { GlobalSearchResult } from '@/types';

export interface LibraryMaterial {
  id: string;
  transcriptCount: number;
  hasNotes: boolean;
  hasSummary: boolean;
  summaryStatus: string | null;
  audioAvailable: boolean;
}
export interface MeetingExportSnapshot {
  id: string;
  title: string;
  createdAt: string;
  capturedAt: string;
  audioAvailable: boolean;
  notes: { source: 'user_notes'; markdown: string; json?: unknown[] | null; revision: number };
  summary: string | null;
  summaryStatus: string | null;
  transcripts: Array<{ id: string; text: string; speaker: string | null; audioStartTime: number | null }>;
  actions: Array<{ text: string; done: boolean; owner: string | null; dueText: string | null; source: string }>;
}
export const libraryMaterials = () => invoke<LibraryMaterial[]>('api_library_materials');
export const searchLibrary = (query: string) => invoke<GlobalSearchResult[]>('api_search_library', { query });

/** Navigation only. Search hits are never appended to a meeting's AI context. */
export function searchResultLink(result: Pick<GlobalSearchResult, 'kind' | 'id' | 'meetingId' | 'transcriptId' | 'audioStartTime'>): string {
  const params = new URLSearchParams({ id: result.meetingId ?? result.id });
  if (result.kind === 'transcript' && result.transcriptId) params.set('t', result.transcriptId);
  if (result.kind === 'notes') params.set('section', 'notes');
  if (result.audioStartTime != null && Number.isFinite(result.audioStartTime)) params.set('ts', String(Math.max(0, result.audioStartTime)));
  return `/meeting-details?${params}`;
}
