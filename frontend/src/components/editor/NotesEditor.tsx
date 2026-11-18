'use client';

/**
 * The rich-text editor behind meeting notes and the editable AI summary.
 * Themed through the --bn-* variables in globals.css. Content loads once per
 * mount (remount with a new `key` to load different content), and every edit
 * reports both BlockNote JSON (exact) and markdown (for search, export, AI).
 */
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Block, PartialBlock } from '@blocknote/core';
import { GridSuggestionMenuController, SuggestionMenuController, useCreateBlockNote } from '@blocknote/react';
import { BlockNoteView } from '@blocknote/shadcn';
import '@blocknote/shadcn/style.css';
import { cn } from '@/lib/utils';
import { themeInfo, useAppTheme } from '@/lib/app-theme';
import { blocksToMarkdownSafely } from '@/lib/blocknote-markdown';
import { BlockNoteLayerProvider, blockNoteMenus, useBlockNoteLayer } from '@/components/editor/blocknote-menus';

/** Space between a menu and the line, and between a menu and the window's edge. */
const MENU_GAP = 10;
const MENU_EDGE = 8;
/** A suggestion menu's height when there is room for all of it. */
const MENU_HEIGHT = 360;

/**
 * Opens a suggestion menu below the line when it fits there (or there is more
 * room below than above), otherwise above the line. BlockNote decides this
 * from the menu's measured height, which is only a sliver when it opens (its
 * items load a moment later), so near the bottom of the window the menu stayed
 * below the line, squeezed to nothing. The room on screen decides it here.
 */
const placeAgainstWindow = {
  name: 'placeAgainstWindow',
  fn({
    x,
    rects,
    elements,
  }: {
    x: number;
    rects: { floating: { width: number } };
    elements: { reference: { getBoundingClientRect(): { top: number; bottom: number } }; floating: HTMLElement };
  }) {
    const line = elements.reference.getBoundingClientRect();
    const below = window.innerHeight - line.bottom - MENU_GAP - MENU_EDGE;
    const above = line.top - MENU_GAP - MENU_EDGE;
    const opensBelow = below >= MENU_HEIGHT || below >= above;
    // Above the line the menu hangs from its bottom edge (globals.css), so it
    // stays against the line while its items load or filter.
    elements.floating.dataset.afMenuSide = opensBelow ? 'below' : 'above';
    elements.floating.style.maxHeight = `${Math.max(0, Math.min(MENU_HEIGHT, opensBelow ? below : above))}px`;
    return {
      x: Math.max(MENU_EDGE, Math.min(x, window.innerWidth - rects.floating.width - MENU_EDGE)),
      y: opensBelow ? line.bottom + MENU_GAP : line.top - MENU_GAP,
    };
  },
};

const suggestionMenuPlacement = { strategy: 'fixed' as const, placement: 'bottom-start' as const, middleware: [placeAgainstWindow] };

/**
 * The slash menu ("/" and the + button) and the emoji picker, drawn in the
 * page-level layer so the notes panel cannot clip them. BlockNote's defaults
 * otherwise; its own copies are turned off below.
 */
function EditorSuggestionMenus({ layer }: { layer: HTMLElement | null }) {
  if (!layer) return null;
  return createPortal(
    <>
      <SuggestionMenuController triggerCharacter="/" floatingOptions={suggestionMenuPlacement} />
      <GridSuggestionMenuController triggerCharacter=":" columns={10} minQueryLength={2} floatingOptions={suggestionMenuPlacement} />
    </>,
    layer,
  );
}

export interface NotesContent {
  json: Block[];
  markdown: string;
}

export interface NotesEditorProps {
  initialBlocks?: unknown[] | null;
  initialMarkdown?: string | null;
  placeholder?: string;
  editable?: boolean;
  onChange?: (content: NotesContent) => void;
  /** Synchronous JSON recovery draft; await markdown before generating or stopping. */
  onDraft?: (content: NotesContent, markdown: Promise<string>) => void;
  /** Synchronous user edit notification, before asynchronous conversion. */
  onEdit?: () => void;
  className?: string;
  /** Called once the initial content is in the editor. */
  onReady?: () => void;
  registerFlush?: (flush: () => Promise<void>) => (() => void);
}

export function NotesEditor({
  initialBlocks,
  initialMarkdown,
  placeholder = 'Write your notes…',
  editable = true,
  onChange,
  onDraft,
  onEdit,
  className,
  onReady,
  registerFlush,
}: NotesEditorProps) {
  const [theme] = useAppTheme();
  const dark = themeInfo(theme).dark;
  const menuLayer = useBlockNoteLayer(dark);
  const loaded = useRef(false);
  const [ready, setReady] = useState(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onEditRef = useRef(onEdit);
  onEditRef.current = onEdit;
  const onDraftRef = useRef(onDraft);
  onDraftRef.current = onDraft;
  const publishRef = useRef<() => void>(() => {});

  const editor = useCreateBlockNote({
    initialContent: initialBlocks && initialBlocks.length > 0 ? (initialBlocks as PartialBlock[]) : undefined,
    placeholders: { emptyDocument: placeholder, default: "Type '/' for headings, lists, and checklists" },
  });

  // Markdown-only content is parsed after the editor exists.
  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      if (!(initialBlocks && initialBlocks.length > 0) && initialMarkdown?.trim()) {
        try {
          const blocks = await editor.tryParseMarkdownToBlocks(initialMarkdown);
          if (!cancelled) editor.replaceBlocks(editor.document, blocks);
        } catch (error) {
          console.error('Could not read notes as markdown', error);
        }
      }
      // Let the replace settle before edits count as changes.
      window.setTimeout(() => {
        if (cancelled) return;
        loaded.current = true;
        setReady(true);
        // Reconcile a recovered JSON draft even if its last conversion was interrupted.
        if (onDraftRef.current) publishRef.current();
        onReady?.();
      }, 60);
    };
    void load();
    return () => {
      cancelled = true;
    };
    // Content is loaded once per mount; callers remount to load new content.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor]);

  // Preserve exact JSON before conversion starts. A failed conversion leaves
  // the recovery draft intact and blocks an AI snapshot of stale markdown.
  const lastMarkdown = useRef(initialMarkdown ?? '');
  const conversions = useRef<Promise<void>>(Promise.resolve());
  useEffect(() => registerFlush?.(async () => {
    // Only the newest edit is authoritative; an older conversion may still be
    // finishing when a newer JSON draft has already replaced it.
    for (;;) {
      const pending = conversions.current;
      try { await pending; }
      catch (error) { if (pending === conversions.current) throw error; }
      if (pending === conversions.current) return;
    }
  }), [registerFlush]);
  const conversionRevision = useRef(0);
  useEffect(() => {
    const publish = () => {
      if (!loaded.current || (!onChangeRef.current && !onDraftRef.current)) return;
      const currentRevision = ++conversionRevision.current;
      const json = structuredClone(editor.document);
      const converted = blocksToMarkdownSafely(editor, json, { source: 'notes-editor', fallbackMarkdown: lastMarkdown.current });
      const markdown = converted.then(result => {
        if (!result.ok || result.markdown === undefined) throw new Error('Could not prepare Meeting notes for saving. Edit or retry the notes.');
        return result.markdown;
      });
      onDraftRef.current?.({ json, markdown: lastMarkdown.current }, markdown);
      const published = markdown.then(value => {
        if (currentRevision !== conversionRevision.current) return;
        lastMarkdown.current = value;
        onChangeRef.current?.({ json, markdown: value });
      });
      conversions.current = published;
      // The preparation barrier observes this rejection; a normal edit must
      // still journal exact JSON without an unhandled promise rejection.
      void published.catch(() => {});
    };
    publishRef.current = publish;
    // BlockNote remounts its Tiptap view when editable changes, removing
    // listeners. Reattach after the readiness transition as well as external
    // editable changes; editor object identity itself stays the same.
    return editor.onChange(() => {
      if (loaded.current) onEditRef.current?.();
      publish();
    });
  }, [editor, editable, ready]);

  return (
    <div className={cn('af-notes-editor', className)}>
      <BlockNoteLayerProvider value={menuLayer}>
        <BlockNoteView
          editor={editor}
          editable={editable && ready}
          theme={dark ? 'dark' : 'light'}
          shadCNComponents={blockNoteMenus}
          slashMenu={false}
          emojiPicker={false}
        >
          <EditorSuggestionMenus layer={menuLayer} />
        </BlockNoteView>
      </BlockNoteLayerProvider>
    </div>
  );
}
