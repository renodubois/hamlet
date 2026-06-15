# Webcam Video Calls PRD

## Problem Statement

Hamlet users can join LiveKit-backed voice channels, mute and deafen, see speaking indicators, and use screen sharing, but they cannot turn on a webcam or see other callers on camera. This makes voice calls feel incomplete for a Discord-like app and forces users to use a separate tool when face-to-face conversation, visual presence, or lightweight show-and-tell would help.

Hamlet also has an intentionally strict Electron media policy: trusted audio and display capture are supported, while camera capture has been denied so far. Adding webcam video must therefore be opt-in, source-aware, privacy-preserving, and tightly scoped to joined voice calls without regressing existing voice, mute/deafen, speaking indicators, device settings, screen sharing, or Electron shell security.

## Solution

Add webcam video streaming to voice channels using the existing LiveKit/WebRTC voice room. A user who has joined a voice channel can start or stop camera video from the voice UI. Other joined participants can see active camera tiles in a bounded grid or strip, including their own local camera tile and remote tiles for other participants. Camera video is separate from microphone audio and screen sharing: turning on camera does not change mute/deafen state, does not auto-start screen sharing, and does not interfere with the selected screen-share viewer.

The server remains a LiveKit control plane. It mints least-privilege tokens that allow microphone, camera, and screen-share video publishing while continuing to deny unrelated sources. It tracks ephemeral camera publication state from LiveKit webhooks, exposes authenticated discovery for active camera streams, and broadcasts camera lifecycle updates over SSE. The client renderer owns camera device selection, capture, local publishing, explicit remote camera subscriptions, video attachment, cleanup, rendering, and user-facing errors.

Voice & Video settings gain camera input selection and an explicit camera preview/test action. Opening settings may enumerate devices, but must not prompt for camera permission until the user explicitly starts preview. Electron allows camera capture only for the trusted Hamlet renderer origin and keeps untrusted origins plus unrelated device APIs denied.

## User Stories

1. As a voice participant, I want to turn on my camera, so that other people in the call can see me.
2. As a voice participant, I want to turn off my camera immediately, so that I can stop broadcasting video whenever I choose.
3. As a voice participant, I want camera controls to appear only after I join voice, so that video is tied to call presence.
4. As a voice participant, I want Hamlet to show a busy state while my camera starts, so that I know permission or publishing is in progress.
5. As a voice participant, I want a clear error when camera permission is denied, so that I understand why video did not start.
6. As a voice participant, I want a clear error when no camera is available, so that I can fix my device setup.
7. As a voice participant, I want a clear error when camera publishing fails, so that I can retry without leaving voice.
8. As a voice participant, I want canceling a camera prompt to leave me connected to voice, so that backing out of video does not drop the call.
9. As a voice participant, I want my microphone mute state to remain unchanged when I start camera, so that video does not unexpectedly affect audio.
10. As a muted participant, I want to turn on camera while staying muted, so that I can be seen without being heard.
11. As a deafened participant, I want camera state to remain understandable, so that audio output state does not hide video state.
12. As a voice participant, I want camera to stop when I leave the voice channel, so that video never continues outside the call.
13. As a voice participant, I want camera to stop when I switch voice channels, so that video does not leak into the wrong room.
14. As a voice participant, I want Hamlet to reflect when the browser or operating system revokes the camera track, so that the UI stays accurate.
15. As a voice participant, I want camera start and stop controls to be keyboard reachable, so that I can use them without a mouse.
16. As an assistive technology user, I want the camera toggle to have a clear accessible name, so that I know whether it starts or stops camera.
17. As an assistive technology user, I want the camera toggle to expose its pressed or active state, so that video state is perceivable.
18. As a low-vision user, I want visible active-camera indicators, so that I can tell who has video on.
19. As a privacy-conscious user, I want camera access to require an explicit user action, so that Hamlet never starts camera passively.
20. As a privacy-conscious user, I want joining voice to avoid a camera prompt, so that audio participation does not imply video consent.
21. As a privacy-conscious user, I want opening settings to avoid a camera prompt, so that passive configuration does not request video access.
22. As a privacy-conscious user, I want camera preview to require an explicit button, so that testing video is intentional.
23. As a privacy-conscious user, I want camera preview tracks to stop when I close settings, so that preview does not continue hidden.
24. As a privacy-conscious user, I want Hamlet to persist no camera frames, so that video calls remain real-time only.
25. As a voice participant, I want to see my own local camera tile, so that I can verify what I am sending.
26. As a voice participant, I want my local camera tile to disappear when I stop camera, so that local UI reflects broadcast state.
27. As a voice participant, I want remote camera tiles to appear when others enable camera, so that the call updates live.
28. As a voice participant, I want remote camera tiles to disappear when others disable camera, so that stale video does not linger.
29. As a voice participant, I want a connecting placeholder for a known active camera before the media track arrives, so that I understand the stream is loading.
30. As a voice participant, I want camera tiles to use participant names, so that I know who each video belongs to.
31. As a voice participant, I want camera tiles to be ordered consistently, so that the layout does not jump unnecessarily.
32. As a voice participant, I want a bounded camera layout, so that many cameras do not take over the app.
33. As a voice participant, I want camera video to coexist with text chat, so that I can keep chatting while in a video call.
34. As a voice participant, I want camera video to coexist with the screen-share viewer, so that faces and shared screens can be used together.
35. As a voice participant, I want screen sharing to remain a separate action, so that turning on camera never shares my screen.
36. As a screen-share presenter, I want camera publishing to keep my screen share active, so that I can present and be visible at the same time.
37. As a screen-share viewer, I want watching a screen share to continue while cameras are visible, so that video chat does not interrupt the presentation.
38. As a participant, I want speaking indicators to keep working while cameras are on, so that the call remains understandable.
39. As a participant, I want mute and deafen controls to keep working while cameras are on, so that existing voice behavior is preserved.
40. As a participant, I want remote microphone audio to keep subscribing normally, so that video does not break voice audio.
41. As a low-bandwidth participant, I want Hamlet to subscribe only to relevant camera video, so that unused or off-channel video does not waste bandwidth.
42. As a low-power-device user, I want camera video to detach when tiles are removed, so that CPU and battery usage stay controlled.
43. As a user in another voice channel, I want not to receive cameras from a channel I have not joined, so that media stays scoped to my call.
44. As a user not in voice, I want to see that people have cameras enabled, so that I know a video call is active.
45. As a user not in voice, I want to be prompted to join before viewing camera video, so that video viewing remains tied to call participation.
46. As a user not in voice, I want Hamlet not to silently join me when someone turns on camera, so that I control voice presence.
47. As a voice participant, I want active camera indicators in the participant list, so that I can see who is on video even if tiles are elsewhere.
48. As a voice participant, I want camera indicators to update live, so that I do not have to reload to see video state.
49. As a late joiner, I want existing active cameras to appear after I join, so that ongoing video state is discovered.
50. As a user who reloads the app, I want active cameras to be rediscovered, so that the UI converges after refresh.
51. As a user with two Hamlet windows, I want camera start and stop to update in both windows, so that local multi-client testing mirrors real-time behavior.
52. As a user with two Hamlet windows, I want only the window that starts camera to publish local video, so that duplicate clients do not surprise me.
53. As a user with multiple cameras, I want to choose a camera device, so that I can use the correct webcam.
54. As a user with no saved camera, I want Hamlet to use the system default, so that video works without setup.
55. As a user who changes camera selection, I want the choice saved, so that future camera starts use my preferred device.
56. As a user with a disconnected selected camera, I want Hamlet to fall back or show a clear error, so that I can recover.
57. As a user in settings, I want camera devices listed alongside voice devices, so that Voice & Video settings cover all call media.
58. As a user in settings, I want camera device labels to appear when the platform allows them, so that I can choose the right camera.
59. As a user in settings, I want unlabeled cameras to have sensible fallback names, so that the selector remains usable before permission.
60. As a user in settings, I want to start a camera preview, so that I can test framing before joining a call.
61. As a user in settings, I want to stop a camera preview, so that I can release the camera when I am done testing.
62. As a user in settings, I want preview errors to be announced accessibly, so that permission or device problems are clear.
63. As a user in settings, I want microphone testing to keep working, so that video settings do not regress audio setup.
64. As a user in settings, I want output-device testing to keep working, so that speaker routing remains available.
65. As an Electron user, I want trusted Hamlet camera capture to work, so that desktop video calls are possible.
66. As an Electron user, I want untrusted origins to be unable to access the camera, so that embedded or external content cannot spy on me.
67. As an Electron user, I want untrusted origins to remain unable to request display capture, so that screen-share security does not regress.
68. As an Electron user, I want unrelated device APIs to remain denied, so that adding camera does not broaden shell capabilities.
69. As an Electron user, I want the packaged renderer policy to allow trusted camera use, so that packaged video calls match development behavior.
70. As a macOS user, I want camera permission prompts to use clear Hamlet metadata, so that the operating-system prompt is understandable.
71. As a Windows user, I want camera permission and fake-device testing paths to behave predictably, so that desktop QA is reliable.
72. As a Linux user, I want Hamlet to handle browser or WebView camera differences gracefully, so that common desktop setups can recover.
73. As a browser user, I want camera calls to work on supported secure/local origins, so that renderer development and web testing are practical.
74. As a browser user, I want permission denial recovery to be documented, so that I can unblock camera access after denying it.
75. As a call participant, I want camera state to be ephemeral, so that restarted servers do not remember old cameras.
76. As a call participant, I want camera stop events to clean up after disconnects, so that abandoned tiles disappear.
77. As a call participant, I want camera stop events to clean up after LiveKit connection aborts, so that failed sessions do not leave stale video.
78. As a call participant, I want duplicate LiveKit events not to duplicate tiles, so that the UI remains clean.
79. As a call participant, I want out-of-order LiveKit events not to resurrect stopped cameras, so that stale video state does not return.
80. As a call participant, I want camera and screen share to coexist for one user, so that one video source does not overwrite the other.
81. As a call participant, I want replacing my camera track to update only my camera, so that screen sharing remains unaffected.
82. As a call participant, I want replacing my screen-share track to update only my screen share, so that camera remains unaffected.
83. As a developer, I want camera tokens to use least-privilege LiveKit grants, so that allowed publish sources match the product scope.
84. As a developer, I want unsupported LiveKit sources to be ignored, so that unexpected webhook data does not corrupt state.
85. As a developer, I want non-voice channels to reject camera-capable voice tokens, so that video remains a voice-channel feature.
86. As a developer, I want active camera discovery to require authentication, so that stream metadata is not public.
87. As a developer, I want camera SSE events to be distinct from screen-share events, so that client logic remains source-aware.
88. As a developer, I want camera stream identity to include participant identity and track identity, so that track churn is distinguishable.
89. As a developer, I want stream state keyed by source, so that camera and screen share cannot overwrite each other.
90. As a developer, I want a reusable active-media registry, so that camera and screen-share state transitions share robust logic.
91. As a developer, I want camera helpers for keys, equality, display names, and sorting, so that UI components do not duplicate identity logic.
92. As a developer, I want camera publishing isolated behind a small interface, so that permission, constraints, and cleanup are testable.
93. As a developer, I want remote camera subscription isolated behind a small interface, so that LiveKit subscription rules are testable.
94. As a developer, I want video attachment and detachment isolated, so that DOM cleanup can be verified without real devices.
95. As a developer, I want MSW fixtures for active cameras, so that UI tests can cover discovery and SSE without LiveKit.
96. As a developer, I want mocked LiveKit rooms and tracks in unit tests, so that camera behavior can be tested deterministically.
97. As a developer, I want existing screen-share tests to stay green, so that camera work proves non-regression.
98. As a tester, I want browser fake-media E2E for camera start and stop, so that the core renderer flow is smoke-tested.
99. As a tester, I want two-client E2E for remote camera tiles where practical, so that real-time camera discovery is verified.
100. As a tester, I want Electron fake-media smoke coverage, so that trusted camera permission paths are checked without native camera fragility.
101. As a tester, I want manual QA instructions for browser and Electron development, so that local workflows are reproducible.
102. As a tester, I want manual QA instructions for packaged Electron, so that static headers and runtime policy are verified together.
103. As a tester, I want manual QA instructions for permission denial and recovery, so that privacy edge cases are validated.
104. As an operator, I want webcam video to use the existing LiveKit deployment, so that no new media service is introduced.
105. As an operator, I want production notes to preserve WebRTC hardening expectations, so that camera is not assumed to work without secure origins, WSS, ICE, and TURN readiness.
106. As an operator, I want bandwidth and CPU caveats documented, so that video calls are treated as heavier than voice.
107. As a future product owner, I want recording excluded from MVP, so that privacy and storage can be designed separately.
108. As a future product owner, I want virtual backgrounds excluded from MVP, so that core video calling ships before effects.
109. As a future product owner, I want moderation controls excluded from MVP, so that camera stop-by-moderator can be designed deliberately later.
110. As a future product owner, I want advanced layout controls excluded from MVP, so that the first version stays bounded and reliable.

## Implementation Decisions

- Webcam video is an extension of the existing LiveKit voice-channel room. No new media transport, signaling service, or server-side media storage is introduced.
- A user must be joined to a voice channel before publishing camera video.
- A user must be joined to the same voice channel before viewing remote camera video. Non-participants may see active-camera indicators and discovery metadata, but not video playback.
- Camera publishing is opt-in and starts only from an explicit user action in the voice UI or an explicit preview action in settings.
- Camera preview in settings is local-only. It does not connect to LiveKit, does not join voice, and does not publish to other users.
- The server grants LiveKit publish sources for microphone, camera, and screen-share video for authenticated voice-channel tokens.
- The server continues to deny screen-share audio, unknown LiveKit sources, and voice-token requests for non-voice channels.
- Token grant construction remains least-privilege and testable as a separate behavior from UI availability.
- Active camera state is ephemeral room state derived from LiveKit webhooks and local client cleanup. It is not persisted in the database.
- The voice state model should be generalized into a source-aware active media stream registry rather than treating screen share as the only video source.
- The active media stream registry is a deep module: it accepts source-tagged publish, unpublish, participant leave, and participant abort inputs; it returns deterministic added, replaced, removed, or unchanged outcomes.
- Active media stream identity includes channel id, publisher user id, LiveKit participant identity, LiveKit track sid, and source.
- The registry allows one active camera track and one active screen-share track for the same participant in the same channel at the same time.
- Replacement is scoped per source. Replacing a camera track does not replace an active screen-share track, and replacing a screen-share track does not replace an active camera track.
- Duplicate LiveKit publish and unpublish webhooks are idempotent.
- Delayed publish webhooks for tracks that already stopped must not resurrect stale camera or screen-share state.
- Participant-level leave and abort cleanup remove all active camera and screen-share streams for that participant and emit source-appropriate stop events.
- LiveKit webhook classification recognizes camera and screen-share sources explicitly and ignores microphone-only tracks for active video-stream state.
- Unknown LiveKit rooms, malformed participant identities, missing users, non-voice channels, empty track ids, and unsupported sources are ignored safely.
- The server exposes an authenticated active-camera discovery API, parallel to active screen-share discovery, with optional channel filtering.
- Active camera stream DTOs include channel id, publisher user id, display metadata, participant identity, track sid, track name, source set to camera, and started timestamp.
- Camera lifecycle SSE events are distinct from screen-share lifecycle events. The event names are `camera_video_started` and `camera_video_stopped`.
- Camera start events include enough stream metadata for clients to render indicators and placeholders without an immediate refetch.
- Camera stop events include enough identity to remove the exact camera stream.
- Existing voice participant, speaking, mute/deafen, and screen-share event payloads continue to work as before.
- The client API layer gains camera stream types, active-camera listing, and camera stop payload types.
- The client SSE event union and EventsProvider gain typed subscriptions for camera started and stopped events.
- MSW voice fixtures gain active camera stream state and camera SSE helpers so component tests can bootstrap and mutate camera state.
- Source-aware media helpers provide stable keys, equality, display names, and sorting for camera streams and screen-share streams without duplicating logic in UI components.
- The voice chat context gains local camera state, camera starting state, local camera track access, remote camera track state, and start, stop, and toggle actions.
- Camera publishing uses LiveKit camera publishing APIs or explicit local camera video publication with LiveKit camera source metadata.
- Camera capture honors the saved camera device when one is configured and otherwise uses the system default.
- Camera start errors are mapped into concise user-facing messages for denied permission, missing devices, and publishing failures.
- Local camera tracks are monitored for ended events so browser or operating-system revocation clears local state and unpublishes as needed.
- Leaving voice, switching channels, and LiveKit disconnection stop local camera, detach local and remote video tracks, clear remote camera subscriptions, and reset camera UI state.
- Camera publishing is independent from microphone publishing. Starting or stopping camera must not change mute, deafen, input gain, noise suppression, speaking detection, or remote audio attachment.
- Camera publishing is independent from screen sharing. Starting or stopping camera must not start, stop, select, watch, or unwatch a screen share.
- The LiveKit room keeps selective subscription behavior. Microphone tracks are subscribed for voice; camera video tracks are subscribed only for active cameras in the joined voice channel; screen-share video remains selected-stream opt-in.
- Remote camera subscription management is a deep module: it accepts current channel, active camera streams, LiveKit remote participants/publications, and join/leave events, then owns source-aware subscribe, unsubscribe, enable, disable, and track mapping decisions.
- Remote camera subscription reconciliation is idempotent. Server/SSE state and LiveKit publication events may arrive in either order and should converge on the correct tile state.
- Remote camera video attachment is a deep module: it attaches a known local or remote video track to a video element, detaches on cleanup, and guarantees tracks are not left attached to hidden DOM.
- Local camera preview/tile attachment reuses the same attachment semantics where practical, while keeping local preview separate from remote subscriptions.
- The voice UI adds a camera toggle to joined voice controls with accessible labels, disabled/busy states, and active styling.
- The participant list shows a camera indicator for users with active camera streams.
- The main voice UI renders a bounded camera tile section when the user is joined and active cameras exist.
- The camera tile section includes the local user's tile while local camera is enabled.
- The camera tile section includes remote participant tiles for active camera streams in the joined channel.
- The camera tile section shows connecting placeholders when server state is known before a LiveKit track is subscribed.
- Camera tile labels use participant display names or usernames and identify the media as camera video.
- Camera tiles may include mute, deafen, and speaking overlays when doing so remains legible and not overcrowded.
- The existing screen-share viewer remains separate from the camera tile section and continues to support explicit watch/stop-watching behavior.
- Users not joined to a voice channel see camera indicators or a join-to-view message rather than remote video playback.
- Voice settings gain camera input device enumeration and saved camera device selection.
- Opening Voice & Video settings enumerates devices only and must not call camera getUserMedia unless the user starts preview.
- Voice settings gain an explicit camera preview start/stop control that requests video, renders local preview, reports errors accessibly, and stops all preview tracks on stop, modal close, or component cleanup.
- Microphone warm-up behavior for labels remains audio-only. Camera label availability is handled without passive video capture.
- Electron runtime permission policy allows trusted renderer media requests for audio, video, or audio plus video, and continues to deny untrusted origins.
- Electron media permission checks allow trusted camera/video checks and device enumeration paths while keeping untrusted checks denied.
- Electron packaged renderer permissions policy allows camera for self and keeps microphone, speaker selection, and display capture constrained to self.
- Electron top-level navigation, popup policy, untrusted origins, HID, USB, serial, geolocation, and unrelated capabilities remain denied.
- Electron package metadata and user-facing documentation are updated to say camera is supported for trusted voice/video flows rather than denied.
- The feature assumes LiveKit adaptive stream and dynacast remain enabled to reduce bandwidth and CPU usage.
- The first release does not expose remote camera resolution, bitrate, or quality controls in the UI.
- No camera frames, thumbnails, recordings, or call history are stored by Hamlet.

## Testing Decisions

- Good tests should assert externally observable behavior: API status codes, token grant claims, SSE payloads, discovery responses, visible controls, accessible labels, permission decisions, LiveKit publish/subscribe calls, track attachment cleanup, and user-facing error states.
- Good tests should avoid asserting private signal names, exact map internals, fragile CSS classes, or incidental component structure unless those details are user-visible contract.
- Server token tests should verify that voice-channel tokens allow microphone, camera, and screen-share video publishing while denying screen-share audio and unrelated sources.
- Server token tests should verify that non-voice channels do not mint camera-capable voice tokens.
- Server active-camera endpoint tests should verify authentication, optional channel filtering, empty initial state, and returned camera stream DTO shape.
- Server webhook tests should cover camera track publication adding active camera state and broadcasting a camera started event.
- Server webhook tests should cover camera track unpublication removing active camera state and broadcasting a camera stopped event.
- Server webhook tests should cover participant left and participant connection aborted cleaning camera and screen-share streams for that participant.
- Server webhook tests should cover duplicate camera publication and duplicate camera unpublication idempotency.
- Server webhook tests should cover unpublish-before-publish and delayed publish-after-stop protection for camera tracks.
- Server webhook tests should cover replacing a camera track without replacing an active screen-share track from the same participant.
- Server webhook tests should cover replacing a screen-share track without replacing an active camera track from the same participant.
- Server webhook tests should cover camera and screen-share coexistence for the same participant in the same channel.
- Server webhook tests should cover ignored microphone tracks, screen-share audio tracks, unknown sources, unknown rooms, malformed participant identities, missing users, and non-voice channels.
- Server voice-state unit tests should cover source-aware keys, per-source replacement, per-source stop markers, participant-wide cleanup, stable sorting, and channel filtering.
- Server broadcast tests should use the existing quiet broadcaster pattern rather than a production ping loop.
- Client API tests should cover the active-camera listing helper request path, channel query serialization, response parsing, and error handling.
- Client event tests should cover camera started and camera stopped dispatch through typed event subscriptions.
- Client source-helper tests should cover stable camera keys, equality, display-name fallback, and sorting.
- Client MSW tests should cover active camera fixtures, camera SSE mutation helpers, and interaction with existing screen-share fixtures.
- Voice settings tests should verify camera device selection persistence, fallback labels, devicechange refresh, explicit preview start, preview stop, cleanup on unmount, and accessible error rendering.
- Voice settings tests should verify that opening settings does not request camera permission or start a camera track.
- Voice settings tests should verify that existing microphone test, input gain, noise suppression, output selection, and test sound behavior still work.
- Voice context tests should use mocked LiveKit rooms, participants, publications, and tracks to cover local camera start, local camera stop, selected camera constraints, permission denial, no-device errors, publish failure, and local track-ended cleanup.
- Voice context tests should cover leave, channel switch, LiveKit disconnect, and provider cleanup stopping camera and detaching tracks.
- Voice context tests should cover camera independence from mute, deafen, speaking state, microphone publishing, and screen-share publishing.
- Voice context tests should cover remote camera subscription when an active stream exists in the joined channel.
- Voice context tests should cover remote camera unsubscription and detachment when streams stop, participants leave, channels switch, or the user leaves voice.
- Voice context tests should cover server/SSE camera state arriving before LiveKit publications and LiveKit publications arriving before server/SSE camera state.
- Voice context tests should cover continued selected-screen-share behavior while camera streams are active.
- Video attachment tests should verify that local and remote video tracks attach to visible video elements and detach on tile removal, track change, stop, and component cleanup.
- Voice channel component tests should cover camera toggle rendering, labels, pressed state, busy state, error state, and disabled behavior before join.
- Voice channel component tests should cover participant camera indicators, active-camera bootstrap, camera started and stopped SSE updates, and removal on participant leave.
- Camera tile component tests should cover local tile rendering, remote tile rendering, connecting placeholders, accessible video labels, stable participant names, and bounded layout behavior.
- Camera UI tests should cover non-participant join-to-view messaging and absence of remote video playback before joining.
- Accessibility tests should cover keyboard activation, visible focus, named regions, named video elements, polite announcements where practical, and no color-only state.
- Electron security tests should verify trusted media requests for video-only and audio-plus-video are allowed, trusted audio remains allowed, and untrusted media requests remain denied.
- Electron permission-check tests should verify trusted camera/video checks and device enumeration are allowed while untrusted checks remain denied.
- Electron static-renderer tests should verify packaged permissions policy allows camera for self and does not broaden unrelated capabilities.
- Electron device-permission tests should verify HID, USB, serial, and unrelated device APIs remain denied.
- Electron smoke tests should use fake media devices to verify trusted camera getUserMedia succeeds, returns video tracks, and stops tracks after use.
- Electron smoke tests should continue to verify display capture still works through the trusted path and preload globals remain unavailable.
- Browser voice E2E should add a fake-media smoke for joining voice, starting camera, seeing a local tile, stopping camera, and tile removal.
- Browser two-client voice E2E should verify remote camera discovery and tile visibility when the LiveKit stack is available.
- Browser or Electron E2E may skip camera media assertions when fake media or LiveKit prerequisites are unavailable, but the skip reason should be explicit.
- Manual QA should cover browser renderer development, Electron development, and packaged Electron.
- Manual QA should cover macOS, Windows, and Linux camera permission prompts, denial, recovery, camera disconnect, and selected-device behavior.
- Manual QA should cover camera with microphone muted, camera with deafen, camera with screen sharing, leave cleanup, channel switch cleanup, and two-client live updates.
- Relevant pre-completion checks are the server formatter, server lint/clippy, server tests, client formatter, client lint, client typecheck, client tests, and voice-related E2E where practical. Run the client size check if camera UI changes materially affect bundle size.

## Out of Scope

- Recording, clips, server-side media storage, replay, or call history.
- Virtual backgrounds, blur, filters, beauty effects, reactions over video, or camera effects.
- Remote camera resolution, bitrate, framerate, or quality controls exposed in UI.
- Per-tile pinning, pop-out video, picture-in-picture, spotlight mode, or advanced layout presets.
- Moderation controls for forcing another user to stop camera.
- Watch-only camera viewing without joining voice.
- Viewing cameras from multiple voice channels at once.
- Public production LiveKit deployment work beyond the existing WebRTC hardening requirements for voice and screen sharing.
- Mobile-specific camera behavior.
- New media infrastructure outside LiveKit.
- Persistence of active camera state across server restarts.
- Public distribution, signing, notarization, installers, or auto-update work for Electron.

## Further Notes

- This PRD supersedes the earlier camera-denial assumption used while screen sharing was introduced. Screen sharing should remain source-aware and independent, but camera is now an intentional trusted voice/video feature.
- The main product risk is privacy. Camera access must remain explicit, visible, revocable, scoped to trusted origins, and cleaned up on leave or failure.
- The main technical risk is accidental media subscription or stale stream state. Source-aware state and selective LiveKit subscriptions are core requirements, not optimizations.
- The existing in-memory development database does not affect camera stream state; active cameras should remain intentionally ephemeral even after persistent SQLite is introduced.
- Local development should continue to use the existing server and LiveKit Compose workflows. Production-quality camera calls inherit the same secure-origin, WSS, ICE advertisement, and TURN considerations as voice and screen sharing.
