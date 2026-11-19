'use client';

import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Overline } from '@/components/ui/surface';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { EXPORT_PARTS, exportMeetings, type ExportPart, type ExportableMeeting } from '@/lib/meeting-export';
import type { ExportFormat } from '@/lib/exportSummary';

const FORMATS: Array<{ id: ExportFormat; label: string; hint: string }> = [
  { id: 'pdf', label: 'PDF', hint: 'To share or print' },
  { id: 'docx', label: 'Word', hint: 'Editable document' },
  { id: 'markdown', label: 'Markdown', hint: 'For notes apps' },
  { id: 'txt', label: 'Text', hint: 'Plain text' },
  { id: 'json', label: 'JSON', hint: 'For other tools' },
];

/** Exports several meetings into one document, one section per meeting. */
export function ExportMeetingsDialog({
  open,
  onOpenChange,
  meetings,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetings: ExportableMeeting[];
}) {
  const identity = JSON.stringify(meetings.map(meeting => meeting.id));
  const active = useRef<string | null>(identity);
  active.current = open ? identity : null;
  const runId = useRef(0);
  useEffect(() => () => { active.current = null; ++runId.current; }, []);
  const [parts, setParts] = useState<Set<ExportPart>>(() => new Set<ExportPart>(['notes', 'actions', 'summary', 'transcript']));
  const [format, setFormat] = useState<ExportFormat>('markdown');
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);

  useEffect(() => {
    ++runId.current;
    setProgress(null);
  }, [open, identity]);

  const toggle = (part: ExportPart, on: boolean) =>
    setParts((current) => {
      const next = new Set(current);
      if (on) next.add(part);
      else next.delete(part);
      return next;
    });

  const count = meetings.length;
  const busy = progress !== null;

  const run = async () => {
    const run = ++runId.current;
    const key = identity;
    const isCurrent = () => active.current === key && runId.current === run;
    setProgress({ done: 0, total: count });
    try {
      const saved = await exportMeetings(meetings, parts, format, (done, total) => { if (isCurrent()) setProgress({ done, total }); }, isCurrent);
      if (saved && isCurrent()) {
        toast.success(count === 1 ? 'Meeting exported' : `${count} meetings exported`);
        onOpenChange(false);
      }
    } catch (error) {
      if (isCurrent()) toast.error('Export failed', { description: error instanceof Error ? error.message : String(error) });
    } finally {
      if (isCurrent()) setProgress(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !busy && onOpenChange(next)}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{count === 1 ? 'Export this meeting' : `Export ${count} meetings`}</DialogTitle>
          <DialogDescription>
            {count === 1 ? 'One saved snapshot, with transcript, Notes and summary kept distinct.' : 'One document with a separately timestamped saved snapshot for each meeting.'}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-5">
          <div>
            <Overline className="mb-2">Include</Overline>
            <div className="grid grid-cols-2 gap-1.5">
              {EXPORT_PARTS.map((part) => (
                <label
                  key={part.id}
                  className="flex cursor-pointer items-center gap-2.5 rounded-lg border border-af-border bg-af-panel-2/60 px-3 py-2 text-[13px] text-af-text-2 transition-colors hover:border-af-border-strong hover:text-af-text"
                >
                  <Checkbox checked={parts.has(part.id)} onCheckedChange={(checked) => toggle(part.id, checked === true)} />
                  {part.label}
                </label>
              ))}
            </div>
          </div>

          <div>
            <Overline className="mb-2">Format</Overline>
            <div role="radiogroup" aria-label="Format" className="grid grid-cols-3 gap-1.5">
              {FORMATS.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  role="radio"
                  aria-checked={format === option.id}
                  onClick={() => setFormat(option.id)}
                  className={cn(
                    'rounded-lg border px-3 py-2 text-left transition-[background-color,border-color,color] active:scale-[0.98]',
                    format === option.id
                      ? 'border-af-accent bg-af-accent/[0.1] text-af-text'
                      : 'border-af-border bg-af-panel-2/60 text-af-text-2 hover:border-af-border-strong hover:text-af-text',
                  )}
                >
                  <span className="block text-[13px] font-medium">{option.label}</span>
                  <span className="block text-[11px] text-af-text-4">{option.hint}</span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={() => void run()} loading={busy} disabled={parts.size === 0 || count === 0}>
            {busy && progress && progress.total > 1 ? `Preparing ${progress.done} of ${progress.total}…` : 'Export'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
