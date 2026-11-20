# Ask AI language

Live calls (`LivePanel`) and saved meetings (`MeetingDocument`) use the same
persistent meeting chat and `useAskAiLanguage` selection. There is no application-wide UI
language setting in this app. The existing Summary language setting controls
Ask AI output; the transcription language remains an input-recognition setting.

## Preference order and refresh

- A saved meeting uses its explicit summary language first, then its cached
  detected summary language, then the existing native transcript detector.
- A live call uses the pinned Summary language default first, then the existing
  native detector over the current transcript.
- Saved meetings in Auto do not inherit a subsequently changed global pin. The
  pin is already copied into new meetings by the existing creation flow.
- Unknown, empty, unsupported, or unavailable detection falls back to English.
  Detection uses the existing summary detector, not a Cyrillic-only heuristic;
  Russian is not inferred merely from a Ukrainian or other Cyrillic transcript.

`resolveAskAiLanguage` reuses `summary-language-preferences` read/detection APIs
without changing saved preferences or transcript data. Selection is debounced by
150 ms during live transcript updates. A cancelled effect cannot overwrite a
newer meeting or preference. Successful explicit/default saves emit
`summary-language-changed`; open Ask AI views refresh without a reload. Storage
changes from another window also refresh. Sending is disabled while the initial
selection or a changed preference is resolving, so a fast click cannot submit
with the previous language.

## Copy and model prompts

`lib/ask-ai-language.ts` contains English and Russian Ask AI copy: tab label,
empty state, all suggested questions, placeholders, model/privacy footnote,
reading/error/retry text and send/clear labels. Other supported output languages
currently use English UI copy while still requesting answers in the selected
language. This is a localized panel, not application-wide translation. Raw
provider diagnostics and existing conversation history retain their source text.

Clicking a suggested question sends exactly the displayed text. A manually typed
question is not translated. `send_meeting_chat` accepts optional `answerLanguage`
(`answer_language` in Rust). It appends a system
instruction generated from the existing allowlisted language-code mapping.
Unsupported/missing codes request the transcript language, falling back to the
question language when unclear. Raw arbitrary language text is never interpolated
into a system instruction. Provider routing and model selection are unchanged.

The source transcript, speaker labels, stored questions and citation text are not
translated. Native prompts explicitly preserve citations such as `[12:34]`; the
existing Markdown citation buttons retain their seek behavior. A model can still
ignore an instruction; no output translation pass is applied.

## Persistent meeting context

`meeting_chat.rs` owns SQLite chat requests, completed answers/errors, context
settings and snapshots. Live and saved views use the same durable meeting ID and
`send_meeting_chat` command. It commits the question and pending row before model
resolution/inference. A native runtime task owns completion independently of the
invoking WebView, then emits `meeting-chat-updated`; the database remains the
source of truth if an event is missed. On native startup only,
`recover_interrupted` marks unfinished rows visibly interrupted. A WebView reload
does not run recovery. The existing provider resolver, including Codex CLI, is
reused without adding tools or search.

A transaction snapshots persisted transcript, complete Meeting notes, summary,
action items, settings and conversation history. The default includes the latest
10,000 Unicode scalar characters of transcript text; timestamps, speaker labels
and generated separators do not consume that limit. Settings select a positive
character count or the whole transcript. Notes are always complete and outside
the transcript budget. Empty transcripts still allow notes-based questions.
Summary input uses its own complete snapshot and does not use this chat limit.

`get_meeting_chat_context` previews the current boundary. Every turn permanently
stores the settings, source row IDs, included/total character counts, global
start/end character offsets, offset within the first row and nullable source
start/end seconds. An intra-turn cut keeps the turn's real start timestamp;
there are no word timestamps to justify a more precise time. The displayed
transcript is never rewritten by context selection. Later transcript or settings
changes affect the next question, not earlier answer metadata.

All chat remains in SQLite; prompts include the latest 20 completed turns in
full. The included and total counts are stored in answer metadata for display.
Failed/interrupted turns remain visible but are not represented as successful
model history. Only one pending turn per meeting is allowed. Deleting a meeting
cascades chat rows (also on connections with foreign keys disabled); a late
completion only updates an existing pending row and cannot recreate anything.

Source instructions require timestamp citations for conversation claims, the
label «Из Meeting notes» (English: “From Meeting notes”) for user notes, explicit
contradictions and missing information, and clear attribution of any external
information supplied by existing provider capabilities. Previous assistant
answers and generated summaries are not treated as transcript evidence.

## Verification

Run mock-heavy files separately from `frontend/`:

```sh
pnpm dlx bun@1.3.10 test tests/chat/language.test.tsx
pnpm dlx bun@1.3.10 test tests/chat/language-selection.test.tsx
pnpm dlx bun@1.3.10 test tests/chat/live-panel.test.tsx
pnpm dlx bun@1.3.10 test tests/lib/summary-language-preferences.test.js
```

These cover Russian UI, unchanged preset/typed questions, citation seeking,
live/saved IPC language propagation, unchanged transcript text, blocking sends
while language loads, explicit/Auto/cached preference precedence,
regional codes, detection failure and refresh after meeting/default changes.
The original English placeholder and error prefix reproduced the regression
before the Russian component tests passed.

Native prompt tests: `cargo test -p meetily --lib
live_assistant::answer_language_tests` with native build prerequisites. A portable
alternative is `python3 frontend/scripts/test-ask-ai-language-portable.py` from the
repository root. It compiles the actual mapping, helper and tests extracted
verbatim; it does not link the desktop application or contact any model. Native
unit tests validate Russian/regional codes, other supported languages, citation
instructions and rejected arbitrary input. Neither this harness nor mocked UI
tests qualify a real provider's answer quality or an installed desktop build.

`python3 frontend/scripts/test-meeting-chat-portable.py` compiles the actual
production chat module and Codex CLI module with the repository lockfile and all
SQLite migrations, without linking desktop/audio dependencies. It covers Unicode
cuts, settings, immutable per-answer ranges, complete notes, snapshot isolation,
history bounds, real database reopen/recovery, legacy-connection deletion and
late completions. A synthetic executable receives a prompt through the actual
Codex adapter, then its answer persists after the observing task handle is
dropped and survives database reopen. This qualifies the storage/task/provider
fixture contract; it does not prove installed WebView behavior or authenticated
model answer quality. The real installed CLI tests remain explicitly ignored.
