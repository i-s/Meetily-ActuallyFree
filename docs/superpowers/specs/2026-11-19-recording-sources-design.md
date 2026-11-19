# #23 — ordinary recording sources

## Approved direction
The owner rejected extra source pills and split-start controls in favor of the existing microphone/output icon buttons and their existing device/source chevrons. On 2026-11-19 they asked to continue after this direction was described. No new selector, mode, upgrade dialog or banner is added.

## Contract
- Idle icon buttons persist microphone/system inclusion for every ordinary recording entrypoint. New defaults keep both enabled. Legacy `in_person:true` with no explicit new system choice migrates to system disabled; explicit modern choices take precedence. Preserve app targets/devices/gains.
- Native startup freezes source selection before device resolution and opening capture. Neither selected-device fallback nor automation may broaden this selection. Both off fails visibly. Missing chosen apps never becomes global audio.
- During a recording the existing buttons mute included streams. An excluded stream remains unavailable for that take (tooltip explains stop/enable/next recording). No live stream addition is introduced.
- New recordings have no in-person mode. Microphone-only recording retains neutral microphone identity provenance, without fabricating room identity. Archived `in_person` metadata and transcript-version identity restrictions remain intact.
- Existing Settings switch and new-recording in-person copy disappear. No transcript, database migration or archival metadata rewriting.

## Approaches
A separate source pill and a split-start menu were demonstrated and rejected. Reuse existing controls, as directed by the owner: least visual change, source buttons become durable pre-start choices. Frozen stream selection deliberately avoids adding unqualified live capture reconfiguration.

## Constraints and qualification
Only #23 on `meetily/ux-23`, based on verified `c05a32f64d703a29083186110b72a01f6eca1449`. No agents, push, PR21 changes/watch, Actions, packaging, account or owner-Mac operations. Synthetic portable native tests and isolated frontend tests only. Physical device capture, update/install and archived-meeting playback remain a manual milestone.
