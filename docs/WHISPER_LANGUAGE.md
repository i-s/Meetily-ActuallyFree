# Whisper language default

Settings > Transcription > Default Whisper language is always available, including
when Parakeet is selected for live recording. It reuses `LanguageSelection` and
`ConfigContext.selectedLanguage`; the context persists the choice under the existing
`primaryLanguage` localStorage key and synchronizes `set_language_preference`.
The selector invokes its supplied persistence callback once.

With no saved preference, both the frontend and native `LANGUAGE_PREFERENCE`
default to `auto` (original spoken language). Browser/OS English locale no longer
forces English. Existing explicit choices, including English and Auto Translate,
are retained; users with such a choice must select Russian or Auto Detect themselves.

Live Parakeet ignores this preference. Live Whisper uses the native preference;
`PostCallProcessingDialog` passes the selected language to automatic Whisper
retranscription, with `auto` represented as null. Selecting a speaker count does
not select a transcription language. The manual Enhance transcript dialog keeps
its per-meeting language override. Already processed recordings are unchanged
until the user runs enhancement again on their saved audio.

## Verification and limits

- `tests/hooks/whisper-language-default.test.tsx`: English browser locale defaults
  to auto; Russian selection with live Parakeet persists across provider remount;
  native sync is invoked once per change; explicit old preferences remain intact.
- `scripts/test-whisper-language-browser.cjs`: real settings dropdown, reload,
  Start/Pause/Stop and post-call Continue; verifies `language: ru` reaches Whisper
  large-v3 retranscription IPC. Uses Preview fixtures for available models and
  post-call configuration, intercepting inference rather than running a model.
- Run the browser script against the development Preview using `--base-url` or
  `PREVIEW_URL`; optional `PLAYWRIGHT_MODULE`, `CHROMIUM_PATH` and
  `PREVIEW_SCREENSHOT` follow the existing browser regression runner conventions.
- Full isolated frontend tests and native library tests cover surrounding behavior.
  Native model/audio tests remain ignored unless their fixtures are provided.
  This change does not establish real Russian speech accuracy or installed macOS
  behavior; those need a new Mac build and an actual recording.
