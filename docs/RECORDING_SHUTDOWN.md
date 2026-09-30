# Recording shutdown and the macOS checkpoint deadlock

## Observed failure

The installed `e24cb12` macOS candidate reached recording save after a roughly
37-second recording. Its log confirmed capture stopped, the Core Audio stream
was dropped, the audio pipeline ended, all six queued transcription turns
completed, and Parakeet unloaded. It then stopped at `Stopping recording saver`.
There was no completed 30-second audio checkpoint.

A process listing identified the bundled FFmpeg encoding the first microphone
checkpoint. A one-second process sample captured all 872 samples in this stack:

```text
start -> av_log_default_callback -> fputs -> __sfvwrite
      -> _swrite -> __swrite -> __write_nocancel
```

This establishes FFmpeg was blocked writing its diagnostic output. The parent
encoder wrote the entire PCM input before reading piped stdout/stderr. Once
FFmpeg's diagnostic pipe filled, it stopped reading audio; the parent waited for
space in stdin, so neither process could progress. The saver accumulation task
could not finish its checkpoint, and recording Stop awaited that task. The
existing five-minute save timeout eventually allowed transcript persistence but
left audio unavailable to post-call diarization.

This was not the prior idle Core Audio timeout or a missing final audio-channel
closure. The original encoder did close stdin before waiting for process exit;
the deadlock happened earlier, inside the blocking input write. Device monitoring
was also confirmed to complete in this incident. Private logs and recordings are
not repository fixtures.

## Ownership and correction

`audio/encode.rs` calls `audio/encoder_process.rs::write_audio_and_wait` for every
checkpoint. A scoped thread writes the borrowed PCM input while the caller's
`Child::wait_with_output` concurrently drains stdout/stderr and reaps FFmpeg.
Closing the writer provides EOF. The scope joins the writer before returning;
there is no detached encoder, copied recording-sized input, guessed delay, or
new timeout. A failed encoder retains its stderr and exit status instead of
hiding the diagnostic behind the corresponding broken-pipe error.

The existing ordering remains: stop capture and close pipeline input, drain and
flush the pipeline, complete queued transcription, drain saver accumulation,
then finalize microphone/system/mixed tracks. Final partial checkpoints remain
part of normal finalization. The fix does not skip the saver wait or declare
success while recording audio is still pending.

`RecordingState::stop_recording` now freezes elapsed capture time and includes
an outstanding pause in the pause total. Repeated stop calls keep the same end
time. `RecordingManager` retains this state until final save instead of clearing
it immediately after pipeline drain. The observed `Recording duration from
state: None` was a separate consequence of that early cleanup.

The frontend consumes native shutdown progress and displays the current stage.
When native Stop temporarily takes the manager out of its global slot, polling
keeps the last known duration instead of showing `0:00`. Progress events do not
trigger save completion; existing completion events continue to own persistence.

## Diagnostics

INFO messages now identify checkpoint/encoder start and completion, saver drain
start and completion, and each track's finalization. Capture stderr by launching
the installed app's executable from Terminal; standard Finder launches do not
currently provide a persisted `env_logger` logfile. `show_console` / macOS
unified logging should not be assumed to contain these stderr messages.

These stages distinguish a pending checkpoint from pipeline, ASR, or native
capture shutdown. Do not replace an unfinished stage with a successful UI state.

## Regression coverage and limits

`frontend/scripts/test-recording-shutdown-portable.py` directly includes the
production encoder and recording-state sources using repository-locked Rust
dependencies. Its subprocess fixture writes one MiB of diagnostics before
reading two MiB of PCM. The original sequential handler deadlocks until the
fixture's test-only watchdog exits; the corrected handler completes and the
received PCM matches byte for byte. Separate coverage checks encoder failure
status/diagnostics, frozen stopped duration, stopping during pause, and queued
input draining before EOF. The harness also uses an installed real FFmpeg to
encode/decode 35 seconds of synthetic signal and verify sample count and tail
energy. Its device metadata/path discovery stubs do not replace audio logic.

The frontend test `tests/hooks/recording-shutdown.test.tsx` first failed on the
missing stage and zeroed duration, then passed after correction. Run it in its
own Bun process to avoid shared mock contamination.

Native filters are `audio::encode`, `audio::recording_state`, and
`audio::recording_saver`. The saver test queues 35 seconds for all three tracks,
waits for the actual accumulation owner, finalizes the 30-second checkpoint plus
five-second tail, and decodes all resulting files to check retained duration and
signal. macOS CI must run against the bundled FFmpeg executable. Portable tests
and CI are distinct from installing and verifying a corrected capture on a
physical Mac. No hardware retest or publication is implied by the source fix.
