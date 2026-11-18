export type SuggestionSegment = { id: string; text: string; audio_start_time?: number | null; text_truncated?: boolean };
export type VocabularySuggestion = {
  term: string;
  transcriptId: string;
  excerpt: string;
  audioStartTime?: number | null;
};

const MAX_ROWS = 2000;
const MAX_CHARS = 100_000;
const MAX_CANDIDATES = 100;
const MAX_VISIBLE = 5;
const tokenCharacter = /[\p{L}\p{N}_-]/u;

/** A spelling finder, not a correctness/identity model. Never changes source
 * text or learns a word. Bounds apply even to a single pathological long line. */
export function suggestVocabulary(segments: SuggestionSegment[], excluded: ReadonlySet<string>): {
  candidates: VocabularySuggestion[]; truncated: boolean;
} {
  const candidates: VocabularySuggestion[] = [];
  const seen = new Set(excluded);
  let remaining = MAX_CHARS;
  let truncated = segments.length > MAX_ROWS;
  for (const segment of segments.slice(0, MAX_ROWS)) {
    if (remaining <= 0) { truncated = true; break; }
    let cut = Math.min(segment.text.length, remaining);
    // UTF-16 budgets must never cut a supplementary letter in half.
    if (cut < segment.text.length && /[\uD800-\uDBFF]/.test(segment.text[cut - 1] ?? '')) cut--;
    const text = segment.text.slice(0, cut);
    remaining -= text.length;
    if (text.length < segment.text.length) truncated = true;
    for (const match of text.matchAll(/[\p{L}\p{N}][\p{L}\p{N}_-]*/gu)) {
      // Do not turn the cut-off prefix of a long token into a suggested term.
      if (match.index + match[0].length === text.length && text.length < segment.text.length
        && tokenCharacter.test(String.fromCodePoint(segment.text.codePointAt(text.length)!))) continue;
      const term = match[0].replace(/[_-]+$/, '');
      if (term.length < 2 || term.length > 64) continue;
      const distinctive = /^\p{Lu}{2,}$/u.test(term) || /\p{Ll}\p{Lu}/u.test(term)
        || (/\p{L}/u.test(term) && /\p{N}/u.test(term));
      const key = term.toLowerCase();
      if (!distinctive || seen.has(key)) continue;
      seen.add(key);
      let start = Math.max(0, match.index - 48);
      if (/[\uDC00-\uDFFF]/.test(text[start])) start--;
      let end = Math.min(text.length, Math.max(match.index + term.length, start + 160));
      if (end < text.length && /[\uD800-\uDBFF]/.test(text[end - 1])) end++;
      candidates.push({ term, transcriptId: segment.id, audioStartTime: segment.audio_start_time,
        excerpt: `${start ? '…' : ''}${text.slice(start, end)}${end < segment.text.length || segment.text_truncated ? '…' : ''}` });
      if (candidates.length === MAX_CANDIDATES) { truncated = true; break; }
    }
    if (candidates.length === MAX_CANDIDATES) break;
  }
  return { candidates: candidates.slice(0, MAX_VISIBLE), truncated: truncated || candidates.length > MAX_VISIBLE };
}
