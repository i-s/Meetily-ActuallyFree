import { useCallback, useEffect, useRef } from 'react';
import { toast } from 'sonner';
import { exportSummaryAs, type ExportFormat } from '@/lib/exportSummary';
import { captureMeetingExport, renderMeetingMarkdown, type ExportPart } from '@/lib/meeting-export';
import type { Summary } from '@/types';

export type MeetingExportContent = 'transcript' | 'summary' | 'both' | 'notes' | 'all';
export type MeetingExportFormat = ExportFormat | 'clipboard';
interface UseCopyOperationsProps {
  meeting: { id: string; title?: string; created_at: string };
  meetingTitle?: string;
  aiSummary: Summary | null;
}
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const partsFor = (content: MeetingExportContent): Set<ExportPart> => new Set<ExportPart>(
  content === 'all' ? ['notes', 'summary', 'transcript'] : content === 'both' ? ['summary', 'transcript'] : [content],
);

/** Both export entrypoints read the same committed native snapshot, not a stale
 * React summary or only the transcript pages currently loaded on screen. */
export function useCopyOperations({ meeting }: UseCopyOperationsProps) {
  const active = useRef<string | null>(meeting.id);
  active.current = meeting.id;
  const generation = useRef(0);
  useEffect(() => {
    active.current = meeting.id;
    return () => { active.current = null; generation.current++; };
  }, [meeting.id]);

  const handleExportMeeting = useCallback(async (content: MeetingExportContent, format: MeetingExportFormat): Promise<boolean> => {
    const id = meeting.id;
    const run = ++generation.current;
    const isCurrent = () => active.current === id && generation.current === run;
    try {
      const snapshot = await captureMeetingExport(id);
      if (!isCurrent()) return false;
      const markdown = renderMeetingMarkdown(snapshot, partsFor(content));
      if (format === 'clipboard') {
        await navigator.clipboard.writeText(markdown);
        if (isCurrent()) toast.success('Saved meeting snapshot copied');
      } else {
        const saved = await exportSummaryAs(format, markdown, snapshot.title, isCurrent);
        if (!saved || !isCurrent()) return false;
        toast.success(`Exported as ${format === 'markdown' ? 'Markdown' : format.toUpperCase()}`);
      }
      return isCurrent();
    } catch (error) {
      if (isCurrent()) toast.error('Export failed', { description: message(error) });
      return false;
    }
  }, [meeting.id]);
  const handleCopyTranscript = useCallback(() => handleExportMeeting('transcript', 'clipboard'), [handleExportMeeting]);
  const handleCopySummary = useCallback(() => handleExportMeeting('summary', 'clipboard'), [handleExportMeeting]);
  return { handleCopyTranscript, handleCopySummary, handleExportMeeting };
}
