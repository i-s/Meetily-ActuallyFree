# macOS Preview follow-up

The user tested candidate `e24cb12decc5e38ade06b3ee04623877f7fd829a`
and reported an indefinitely stopping recording, an absent Codex CLI provider,
and English Ask AI suggestions for Russian meetings.

## Implementation and verification

1. Trace stop from the UI command through device monitoring, capture, queued
   audio, transcription, and file finalization. Reproduce specific blocking
   conditions before changing ownership. Preserve final audio and metadata;
   log individual shutdown stages so a physical-device stall can be localized.
2. Add Codex CLI as a real provider for summaries and Ask AI, including discovery,
   login status, persisted executable path, subprocess limits, and mock-process
   regressions. Use the existing installed CLI and ChatGPT authentication.
3. Use the existing summary output-language preference for Ask AI. Auto follows
   transcript language detection. Localize Russian/English panel copy and preset
   questions, and explicitly pass the selected answer language to native prompts.
4. Run isolated frontend tests, sequential production build/type checks, and
   production native regressions. Review integration before committing.
5. Update the existing fork PR and build an Apple Silicon Preview through CI.
   Preserve version 0.2.18, bundle identity, upstream base, and release settings.
   Verify the exact committed candidate and provide its artifact link.

Hardware reproduction and a real authenticated Codex generation are distinct
from synthetic regressions and a successful packaged build; record those limits.
