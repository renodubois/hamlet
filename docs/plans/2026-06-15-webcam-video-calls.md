# Webcam video streaming in voice calls

## Goal

Add webcam/video streaming to Hamlet voice channels so a user who has joined a voice call can turn their camera on, see local and remote camera tiles, and keep using existing voice, mute/deafen, speaking indicators, and screen sharing without regressions.

This is a LiveKit/WebRTC feature. The Hamlet server remains a control-plane service: it mints least-privilege LiveKit tokens, tracks ephemeral publication state from LiveKit webhooks, and broadcasts state changes over SSE. The client renderer owns capture, publishing, subscription, and rendering.

## Current project state

Hamlet already has these pieces:

- Voice channels, participant join/leave, mute/deafen, speaking indicators, and LiveKit token minting.
- LiveKit self-hosted dev support through `server/docker-compose.yml` and per-worktree port allocation.
- Screen sharing end to end:
  - Server grants `microphone` and `screen_share` publish sources.
  - Server tracks active screen-share video tracks from LiveKit webhooks.
  - Server exposes authenticated active screen-share discovery and SSE started/stopped events.
  - Client voice context publishes/unpublishes screen share, manually subscribes selected remote screen-share video, and renders an attached `<video>` viewer.
  - Browser/Electron E2E and MSW fixtures cover screen-share start/stop and discovery.
- Voice settings for microphone input, output device, noise suppression, input gain, and speaking-indicator preferences.
- Electron has deliberately denied camera capture so far:
  - Runtime permission handlers only allow trusted audio media requests.
  - Packaged renderer security headers use `Permissions-Policy: camera=()`.
  - Electron smoke tests assert camera capture is denied.

The new feature should extend this existing architecture instead of introducing a new media transport.

## Recommended MVP scope

In scope:

1. Users in a voice channel can start and stop webcam video from the voice UI.
2. Camera publishing uses LiveKit camera tracks and honors the selected camera device when one is configured.
3. The server grants `camera` as an allowed LiveKit publish source while keeping unrelated sources denied.
4. The server tracks active camera tracks from LiveKit `track_published` / `track_unpublished` webhooks, exposes authenticated active-camera discovery, and broadcasts camera started/stopped SSE events.
5. The voice UI shows which participants have camera enabled.
6. Joined voice participants see a bounded grid/strip of active camera video tiles, including their own local camera tile and remote tiles for other participants.
7. Remote camera subscriptions are explicit and source-aware so microphone audio and selected screen-share subscription behavior remain unchanged.
8. Voice settings include camera device selection and an explicit camera preview/test action. Opening settings must not prompt for camera permission.
9. Electron allows camera capture only from the trusted renderer origin and keeps untrusted origins and unrelated device APIs denied.
10. Existing screen sharing continues to work alongside camera video.
11. Automated tests cover server grants/state/events, client settings/context/UI behavior, Electron security headers/permissions, and practical voice E2E smoke where fake media is available.
12. Manual QA documentation covers browser, Electron dev, packaged Electron, camera permission denial/recovery, and cross-platform notes.

Out of scope for this MVP:

- Recording, clips, or server-side media storage.
- Virtual backgrounds, blur, filters, reactions over video, or per-tile pinning.
- Remote camera resolution controls exposed in UI.
- Multi-room video viewing without joining the voice channel.
- A public production LiveKit deployment story beyond the existing local/dev Compose stack.

## Required changes by area

### Server / LiveKit control plane

- Add `camera` to LiveKit token `can_publish_sources` for voice channel tokens.
- Keep `screen_share_audio`, unknown LiveKit sources, and non-voice channel token requests denied/ignored.
- Generalize or duplicate the current screen-share publication cache so camera and screen share cannot overwrite each other:
  - A user may have one active camera track and one active screen-share track in the same channel at the same time.
  - Replacement should be per `(channel, user, participant identity, source)` rather than per participant for all video sources.
  - Stale/out-of-order webhook protection should apply independently per source.
- Add a server DTO for active camera streams with the same user/participant/track metadata shape as screen-share streams, using `source: "camera"`.
- Add an authenticated discovery endpoint for active camera streams, likely parallel to `/voice/screen-shares`, with optional `channel_id` filtering.
- Add SSE events for camera lifecycle, e.g. `camera_video_started` and `camera_video_stopped`, instead of overloading screen-share events.
- On participant leave/abort, remove both camera and screen-share streams and broadcast corresponding stop events.
- Keep server state ephemeral; persistent storage is not required.

### Client API, SSE, and fixtures

- Add camera stream types, list API, and SSE event variants.
- Add EventsProvider subscriptions for camera started/stopped.
- Extend MSW fixtures/handlers so component tests can bootstrap and mutate active camera state.
- Add source-aware helpers similar to existing screen-share helpers for stable keys, equality, display names, and sorting.

### Voice context and LiveKit publishing/subscription

- Add camera state and actions to `VoiceChatProvider`:
  - `isCameraEnabled`
  - `isCameraStarting`
  - `startCamera`
  - `stopCamera`
  - `toggleCamera` or equivalent UI-facing flow
  - `localCameraTrack`
  - `remoteCameraTracks` keyed by camera stream
- Publish local camera using LiveKit camera APIs (`setCameraEnabled` or explicit local video track publication) with saved camera device constraints.
- Attach cleanup to local track `ended` events so OS/browser device revocation updates local state.
- Stop camera on voice leave and channel switch.
- Keep mute/deafen and mic publishing independent from camera state.
- Preserve `autoSubscribe: false`; subscribe to remote camera video only for active camera streams in the joined channel and unsubscribe/detach when streams stop or when leaving.
- Keep screen-share video subscription opt-in and selected-stream behavior intact.

### Voice UI and layout

- Add camera toggle in the joined voice controls with accessible labels/states.
- Show a participant-level camera indicator in the voice channel participant list.
- Render a bounded camera tile section when joined to a voice channel and active cameras exist:
  - Show local/self tile when the user’s camera is enabled.
  - Show remote participant tiles as their tracks subscribe.
  - Show connecting placeholders while LiveKit track subscription catches up to server/SSE state.
  - Use stable ordering and labels based on participant display names.
  - Include muted/deafened/speaking overlays when reasonable without overcrowding.
- Keep the existing screen-share viewer separate and usable while camera tiles are visible.
- Non-participants should see camera indicators/discovery but should be prompted to join voice before viewing video.

### Voice settings

- Add video input device enumeration and saved camera device storage.
- Opening Voice & Video settings should enumerate devices only; it must not ask for camera permission.
- Add an explicit camera preview button that calls `getUserMedia({ video })`, shows a local preview, and stops all tracks on cleanup or when stopped.
- Surface permission/device errors accessibly.
- Preserve existing microphone testing and output routing behavior.

### Electron shell and security policy

- Runtime permission policy should allow trusted renderer `media` requests for audio, video, or both.
- Permission checks should allow trusted camera/video checks and enumeration paths.
- Packaged static renderer `Permissions-Policy` should allow `camera=(self)`.
- Keep top-level navigation, popup, untrusted origins, HID/USB/serial, and untrusted camera requests denied.
- Update Electron smoke tests that previously expected camera denial so they now verify trusted camera access and continued untrusted denial.
- Update package metadata/README language that currently says camera capture is denied/not expected.

## UX behavior

- A user must join a voice channel before enabling camera.
- The camera toggle shows a starting/busy state while permission/capture/publish is in progress.
- If permission is denied or no camera exists, the UI shows a concise error and leaves camera off.
- Stopping camera immediately removes the local tile and eventually removes remote tiles through LiveKit/SSE cleanup.
- Leaving voice stops camera, screen share, mic, speaking state, and remote subscriptions.
- A remote camera tile can show a “Connecting…” placeholder if the server knows about the camera before LiveKit track subscription is complete.
- Screen share remains a deliberate separate action and does not auto-start with camera.

## Privacy, safety, and performance considerations

- Camera access is opt-in and requires a user click; settings preview is explicit.
- The app must not prompt for camera during passive settings/device enumeration or while simply joining audio voice.
- Electron camera permission is constrained to the trusted local renderer origin.
- Use LiveKit adaptive stream/dynacast and manual subscription to avoid subscribing to off-channel or inactive video.
- The initial layout should be bounded so a large number of active cameras does not unboundedly expand the sidebar or main channel view.
- No camera frames or recordings are persisted by Hamlet.

## Testing plan

Server:

- Token tests assert `camera`, `microphone`, and `screen_share` are allowed while `screen_share_audio` remains absent.
- Webhook unit tests cover camera publish, replace, unpublish, duplicate events, unpublish-before-publish, participant leave/abort cleanup, non-voice channels, missing users, unsupported sources, and camera/screen-share coexistence for the same participant.
- Endpoint tests cover authenticated camera stream listing and empty initial state.

Client renderer:

- API/event typing tests cover camera SSE dispatch and MSW active camera fixtures.
- Voice context tests cover camera start/stop, selected device constraints, permission denial, leave cleanup, remote camera subscription/unsubscription, and screen-share non-regression.
- Voice channel/component tests cover participant camera indicators, camera toggle labels, active camera bootstrap, SSE started/stopped updates, join-to-view messaging, and accessible video tile labels.
- Voice settings tests cover camera device selection, explicit preview start/stop, cleanup, and no camera prompt on open.

Electron:

- Security unit tests cover trusted camera permission requests/checks, untrusted denial, and static `Permissions-Policy` camera allowance.
- Electron smoke test uses fake media devices to verify trusted camera getUserMedia succeeds and tracks are stopped.

E2E/manual:

- Browser voice E2E should add a fake-media smoke for start/stop camera and remote camera tile visibility when LiveKit is available.
- Manual QA should cover macOS, Windows, and Linux camera permission prompts and recovery paths, plus browser/Electron dev/packaged behavior.

## Suggested implementation order

1. Server grants and ephemeral camera stream state/API/SSE.
2. Electron trusted camera policy and static headers.
3. Camera settings device selector and explicit preview.
4. Voice context local camera publishing and cleanup.
5. Camera stream discovery/SSE in voice UI.
6. Remote camera tile grid with source-aware subscriptions.
7. E2E smoke coverage and manual QA docs.
8. Full client/server checks, voice browser E2E where practical, and final manual QA notes.
