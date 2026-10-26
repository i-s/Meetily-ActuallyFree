import type { TranscriptSegmentData } from '@/types';
import { stamp } from '@/lib/live-context';

export const ASK_AI_SETTINGS_CHANGED = 'af-ask-ai-context-settings-changed';

export interface AskBoundary {
  truncated: boolean;
  startTranscriptId: string | null;
  startOffsetChars: number;
  startTime: number | null;
}

export interface ContextTranscriptSegment extends TranscriptSegmentData {
  askBoundary?: string;
  preventMergeAfter?: boolean;
  originalSegmentId?: string;
}

export function askBoundaryLabel(range: AskBoundary): string {
  const beginning = range.startTime == null ? 'с отмеченного фрагмента' : `с ${stamp(range.startTime)}`;
  return `Ask AI видит разговор начиная ${beginning}; предыдущая часть не передаётся`;
}

/** Keep every character visible. A partial turn retains the original turn's
 * timestamp: ASR does not supply word timings, so inventing a later time would
 * misrepresent the actual context. Split rows must not be remerged by the view. */
export function transcriptWithAskBoundary(
  segments: TranscriptSegmentData[], range: AskBoundary | null,
): ContextTranscriptSegment[] {
  if (!range?.truncated || !range.startTranscriptId) return segments;
  return segments.flatMap<ContextTranscriptSegment>(segment => {
    if (segment.id !== range.startTranscriptId) return [segment];
    const chars = Array.from(segment.text);
    const offset = Math.max(0, Math.min(chars.length, range.startOffsetChars));
    const marker = askBoundaryLabel(range);
    if (offset === 0) return [{ ...segment, askBoundary: marker }];
    return [
      { ...segment, text: chars.slice(0, offset).join(''), preventMergeAfter: true },
      { ...segment, id: `${segment.id}:ask-context`, originalSegmentId: segment.id, text: chars.slice(offset).join(''), askBoundary: marker },
    ];
  });
}
