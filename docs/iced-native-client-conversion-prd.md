# PRD: Iced Native Client Conversion

## Problem Statement

Hamlet currently ships a desktop client as a Tauri v2 shell around a SolidJS web frontend. That client works well with the existing HTTP/SSE backend, but it carries the complexity of a web stack inside a desktop wrapper: Vite, Tailwind, browser APIs, DOM-driven interaction behavior, WebView constraints, and a separate Rust shell that does not participate in application logic.

The team is interested in understanding and planning a conversion to a native Rust GUI built with Iced. The goal is to determine how Hamlet's existing Discord-like client features can map onto a native application architecture, which pieces can be ported directly, which pieces require redesign, and which risks must be resolved before committing to a full rewrite.

The user needs a concrete product and implementation plan that preserves the important Hamlet workflows—authentication, channel navigation, real-time text chat, profile management, embeds, and voice channels—while recognizing that some browser-powered behavior, especially LiveKit/WebRTC voice UX, iframe embeds, drag-and-drop, accessibility tooling, and DOM-based testing, will not translate one-to-one.

## Solution

Create a new native Iced desktop client alongside the existing Tauri/SolidJS client, keeping the current client available until the native client reaches sufficient parity. The native client will continue to use the existing Hamlet server API over HTTP and SSE. It will not require a server rewrite for the initial conversion.

The native client will use an Elm-style architecture: a single top-level application state, typed messages, update functions for state transitions, Iced tasks for one-shot asynchronous work, and Iced subscriptions for long-lived event sources such as Hamlet's SSE stream and the eventual voice worker.

The conversion will be phased:

1. Validate feasibility with focused spikes for Iced authentication, cookie-backed HTTP, authenticated SSE, LiveKit/native audio, and avatar/image handling.
2. Build a native text-chat MVP with login, channel list, message history, sending messages, and live SSE updates.
3. Expand toward text-chat parity with message edit/delete, typing indicators, channel creation/reordering, profile settings, avatar upload/delete, embeds, and emoji insertion.
4. Add voice-channel support behind a feature flag or separate milestone once the LiveKit Rust SDK and native audio/device UX are validated.
5. Package and polish the native app for desktop release, including platform permissions, application metadata, icons, and release automation.

The existing server remains the source of truth for users, sessions, channels, messages, avatars, embeds, voice tokens, participants, and broadcast events. The native client replaces only the client shell and UI/runtime architecture.

## User Stories

1. As a Hamlet desktop user, I want to launch a native desktop app, so that I can use Hamlet without a browser/WebView-based interface.
2. As a Hamlet desktop user, I want to configure the server URL during login, so that I can connect to my local or deployed Hamlet server.
3. As a Hamlet desktop user, I want the app to remember my preferred server URL, so that I do not need to re-enter it every launch.
4. As a Hamlet desktop user, I want to log in with my username and password, so that I can access my authenticated channels and messages.
5. As a Hamlet desktop user, I want to register a new account, so that I can start using Hamlet from the native client.
6. As a Hamlet desktop user, I want failed login attempts to show a clear error, so that I know whether my credentials or server connection are wrong.
7. As a Hamlet desktop user, I want failed registration attempts to show a clear error, so that I can recover from duplicate usernames or invalid input.
8. As a Hamlet developer, I want debug builds to preserve convenient dev login shortcuts, so that local testing remains fast.
9. As a signed-in Hamlet user, I want the app to restore or validate my session on startup, so that I can continue where I left off when possible.
10. As a signed-in Hamlet user, I want stale or invalid sessions to return me to login, so that the app does not get stuck in an unauthorized state.
11. As a signed-in Hamlet user, I want to log out, so that I can switch accounts or end my session.
12. As a signed-in Hamlet user, I want to see a list of text and voice channels, so that I can navigate the workspace.
13. As a signed-in Hamlet user, I want channels to appear in server-defined order, so that the native client matches the shared channel layout.
14. As a signed-in Hamlet user, I want the app to navigate to the first text channel when no channel is selected, so that I land in a useful view after login.
15. As a signed-in Hamlet user, I want text channels and voice channels to be visually distinct, so that I understand which channels have message history and which are joinable voice rooms.
16. As a signed-in Hamlet user, I want to create a text channel, so that I can start a new conversation space.
17. As a signed-in Hamlet user, I want to create a voice channel, so that I can add a new voice room.
18. As a signed-in Hamlet user, I want channel creation errors to be visible, so that I can fix invalid names or connectivity issues.
19. As a signed-in Hamlet user, I want to reorder channels, so that I can organize the channel list.
20. As a signed-in Hamlet user, I want channel reordering to update optimistically but recover on failure, so that the app feels responsive without lying about server state.
21. As a signed-in Hamlet user, I want channel creations from other clients to appear live, so that I do not need to restart or refresh.
22. As a signed-in Hamlet user, I want channel reorder changes from other clients to appear live, so that every client shares the same channel order.
23. As a text-channel participant, I want to open a text channel and see its message history, so that I can catch up on the conversation.
24. As a text-channel participant, I want the channel header to show the current channel name, so that I know where I am posting.
25. As a text-channel participant, I want loading states for message history, so that I understand when messages are being fetched.
26. As a text-channel participant, I want message loading errors to be visible, so that I know when a channel failed to load.
27. As a text-channel participant, I want to send a message, so that I can participate in chat.
28. As a text-channel participant, I want my message draft to clear after sending, so that I can continue typing naturally.
29. As a text-channel participant, I want incoming messages to appear in real time, so that conversations feel live.
30. As a text-channel participant, I want messages edited by their authors to update in real time, so that I see the latest text.
31. As a text-channel participant, I want deleted messages to disappear in real time, so that the channel reflects current server state.
32. As a text-channel participant, I want embed updates to appear after the server fetches metadata, so that link previews populate without a manual refresh.
33. As a text-channel participant, I want to edit my own messages, so that I can fix mistakes.
34. As a text-channel participant, I want to delete my own messages, so that I can remove content I posted.
35. As a text-channel participant, I want message actions to be shown only when applicable, so that I do not see controls I cannot use.
36. As a text-channel participant, I want API failures for edit/delete actions to be visible, so that I understand when the action did not complete.
37. As a text-channel participant, I want URLs in messages to be recognizable and openable, so that I can follow links shared in chat.
38. As a text-channel participant, I want safe native link-opening behavior, so that external links do not unexpectedly execute inside the app.
39. As a text-channel participant, I want link embeds to show title, site, description, and images when available, so that shared links are easier to understand.
40. As a text-channel participant, I want photo embeds to render as image previews when supported, so that image links are useful in-line.
41. As a text-channel participant, I want video/rich embeds that cannot render natively to offer an external-open action, so that I can still view the content.
42. As a message author, I want to suppress embeds on my own message, so that I can hide unwanted previews.
43. As a text-channel participant, I want to insert emoji into a message draft, so that chat feels expressive.
44. As a keyboard user, I want the emoji picker to be usable without disrupting the message draft, so that emoji insertion does not slow down typing.
45. As a text-channel participant, I want typing indicators to show when other users are typing, so that I can see conversation activity.
46. As a text-channel participant, I want my typing notifications to be throttled, so that the app does not spam the server.
47. As a text-channel participant, I want typing indicators to expire automatically, so that stale indicators do not remain forever.
48. As a user with an avatar, I want my avatar to appear next to my messages and in the sidebar, so that I am recognizable.
49. As a user without an avatar, I want a deterministic fallback avatar, so that the UI still has a useful identity marker.
50. As a signed-in Hamlet user, I want to open settings, so that I can manage my profile and voice preferences.
51. As a signed-in Hamlet user, I want to update my display name, so that others see my preferred name.
52. As a signed-in Hamlet user, I want display-name validation errors to be clear, so that I can fix invalid input.
53. As a signed-in Hamlet user, I want to clear my display name, so that my username is used again.
54. As a signed-in Hamlet user, I want to upload an avatar, so that I can personalize my profile.
55. As a signed-in Hamlet user, I want to delete my avatar, so that I can revert to the fallback identity image.
56. As a signed-in Hamlet user, I want profile changes to update my existing visible messages, so that the UI reflects my latest display name and avatar immediately.
57. As a voice-channel participant, I want to see who is connected to each voice channel, so that I know where people are talking.
58. As a voice-channel participant, I want participant joins to appear live, so that the sidebar reflects voice presence.
59. As a voice-channel participant, I want participant leaves to appear live, so that stale users do not remain in voice lists.
60. As a voice-channel participant, I want to join a voice channel, so that I can talk with other users.
61. As a voice-channel participant, I want to leave a voice channel, so that I can disconnect cleanly.
62. As a voice-channel participant, I want to switch voice channels, so that I can move between conversations.
63. As a voice-channel participant, I want to mute and unmute my microphone, so that I control when I transmit audio.
64. As a voice-channel participant, I want to deafen and undeafen, so that I control whether remote audio plays.
65. As a voice-channel participant, I want speaking indicators, so that I can tell who is talking.
66. As a voice-channel participant, I want voice connection errors to be visible, so that I know when LiveKit or microphone access failed.
67. As a voice-channel participant, I want voice state to clean up on disconnect, so that the UI does not show me as still active after leaving.
68. As a voice-channel participant, I want device preferences to persist when supported, so that my chosen microphone/output settings survive restarts.
69. As a voice-channel participant, I want microphone permission failures to be understandable, so that I can fix operating-system permissions.
70. As a Hamlet maintainer, I want the native client to use the existing HTTP and SSE contracts, so that the server does not need a parallel API surface.
71. As a Hamlet maintainer, I want API DTOs and event types to be strongly typed in Rust, so that client/server contract mismatches are caught early.
72. As a Hamlet maintainer, I want the SSE parser and reconnect behavior isolated behind a simple interface, so that realtime behavior can be tested without the UI.
73. As a Hamlet maintainer, I want the API transport isolated behind a simple interface, so that cookie handling, base URL handling, and error mapping are tested once.
74. As a Hamlet maintainer, I want app state transitions to be tested as pure reducers where possible, so that UI behavior is reliable without fragile pixel tests.
75. As a Hamlet maintainer, I want the existing Tauri/Solid client to remain available during development, so that users are not blocked by incomplete native parity.
76. As a Hamlet maintainer, I want the voice implementation feature-gated until proven stable, so that text-chat progress is not blocked by native media complexity.
77. As a release engineer, I want native packaging to include app icons and platform metadata, so that the app can be distributed as a real desktop application.
78. As a release engineer, I want macOS microphone permission metadata when voice is enabled, so that voice works without confusing OS-level failures.
79. As a QA tester, I want deterministic test fixtures for auth, channels, messages, SSE events, and voice presence, so that regressions can be reproduced.
80. As a QA tester, I want a manual acceptance checklist for native-only behavior such as windowing, focus, drag/reorder, file pickers, and microphone access, so that WebView-specific tests are not assumed to cover native behavior.
81. As a future contributor, I want the native client modules to be documented at their boundaries, so that I can understand where to add features safely.
82. As a future contributor, I want browser-specific compromises documented, so that iframe embeds, WebAudio behavior, drag-and-drop, and accessibility regressions are explicit decisions rather than surprises.

## Iced Project Structure Research

Research sources included the official Iced README, Iced book, current `iced` API docs, `Task` and `Subscription` docs, `iced_test` docs, official examples, and the Iced discussion about splitting state/message/update/view into modules:

- <https://github.com/iced-rs/iced>
- <https://book.iced.rs/first-steps.html>
- <https://docs.rs/iced/latest/iced/>
- <https://docs.rs/iced/latest/iced/struct.Task.html>
- <https://docs.rs/iced/latest/iced/struct.Subscription.html>
- <https://docs.rs/iced_test/latest/iced_test/>
- <https://github.com/iced-rs/iced/blob/master/examples/README.md>
- <https://github.com/iced-rs/iced/discussions/1572>

Findings to carry into Hamlet's native client structure:

- Iced's fundamental application shape is the Elm Architecture: state, messages, view logic, and update logic. The official book emphasizes modeling the application's data states carefully and using Rust enums to make impossible UI states impossible.
- Current Iced applications should be organized around the `iced::application` builder, with explicit boot/new, update, view, title, theme, and subscription functions. Older examples and discussions may use `Sandbox`, `Application` traits, or `Command`; Hamlet should pin an Iced release before implementation and follow that release's `Task`-based APIs rather than copying master-only or legacy examples blindly.
- `Task` is the right Iced primitive for one-shot asynchronous effects such as login, session validation, fetching channels/messages, image fetches, avatar uploads, and saving config. Tasks can be mapped, batched, chained, and aborted, which supports child-module composition without leaking child messages to unrelated modules.
- `Subscription` is the right Iced primitive for passive or long-lived event streams such as SSE, keyboard/window events, timers for typing indicator expiry, and the future voice worker. Subscriptions are declarative stream builders identified by the runtime; when a subscription stops being returned, Iced tears down its stream. This fits Hamlet's signed-in-only SSE and voice lifetimes.
- Iced's scaling guidance composes applications by splitting screens or features into their own state/message/update/view units. The parent app wraps child messages, calls child update functions, maps child tasks with `Task::map`, maps child elements with `Element::map`, maps child subscriptions with `Subscription::map`, and handles child `Action` enums for route changes or cross-feature effects.
- Official Iced examples are intentionally compact and often single-file, but they demonstrate reusable patterns Hamlet should keep: loading/loaded/error state enums, background persistence via tasks, keyboard/window subscriptions, runtime operations for focus/scroll/window behavior, `Subscription::run` workers for bidirectional background connections, and headless UI tests through `iced_test`.
- For a non-trivial chat client, splitting files only by Iced primitive (`state.rs`, `message.rs`, `update.rs`, `view.rs`) is not enough. Hamlet should combine Iced's MVU composition with domain/deep-module boundaries so API transport, protocol DTOs, SSE parsing, storage, image handling, and voice worker logic stay testable outside the widget tree.

### Proposed Native Client Crate Layout

Create the native Iced client as a new sibling directory named `client-iced/`, instead of placing it inside the existing Tauri/Solid `client/` directory. The current client remains available during the conversion. Do not introduce a repository-root `Cargo.toml` or root Cargo workspace for the initial conversion; keep the Iced crate self-contained under `client-iced/`.

```text
client-iced/
  Cargo.toml
  README.md
  src/
    main.rs                 # process setup, tracing/logging, app::run()
    app/
      mod.rs                # iced::application builder and public App type
      state.rs              # top-level App state, boot/signed-out/signed-in shape
      message.rs            # top-level Message wrappers and runtime events
      update.rs             # top-level orchestration and child Action handling
      view.rs               # top-level route/shell composition
      subscription.rs       # batches active subscriptions from app state
      route.rs              # native route enum; no browser router semantics
    feature/
      auth/                 # login/register/session/logout state + view
      channels/             # channel list, creation, reorder, channel SSE events
      chat/                 # selected text channel, messages, drafts, typing
      profile/              # profile/settings/display-name/avatar flows
      embeds/               # native embed view models and actions
      voice/                # UI state plus command/event contract for voice worker
    protocol/               # serde DTOs matching server payloads and SSE events
    api/                    # typed HTTP transport, cookies, multipart, errors
    realtime/               # SSE stream, parser, reconnect/backoff, event mapping
    storage/                # typed config/preferences persistence and paths
    image/                  # avatar/image fetch, cache, fallback generation
    platform/               # file dialogs, open-external-link, OS permissions
    ui/
      theme.rs              # Hamlet palette, typography, widget styles
      layout.rs             # shell/sidebar/content sizing constants
      widget/               # reusable avatar, message row, modal, popover widgets
    test_support/           # fixtures, fake API, fake SSE, fake storage, builders
  tests/
    api_transport.rs
    protocol_roundtrip.rs
    sse_parser.rs
    reducers.rs
    iced_smoke.rs
```

Module conventions:

- `app` is the only module that knows the whole application. It owns the selected route, authenticated session, service handles, and cross-feature orchestration.
- Each `feature/*` module exposes a small public surface: `State`, `Message`, `Action`, `update`, `view`, and optionally `subscription`. Feature views emit feature-local messages; the app maps them to top-level messages.
- Feature update functions should be reducer-like and testable. When they need parent-owned effects, they return an `Action`; when a self-contained async effect is acceptable, they return a mapped `Task` plus state changes. Cross-feature events, route transitions, and logout/session-expiration handling stay in `app::update`.
- `protocol`, `api`, `realtime`, `storage`, `image`, and `platform` are deep modules. They expose typed capabilities and events while hiding implementation details such as request construction, cookie jars, SSE frame parsing, retry timers, filesystem paths, image handles, and OS integrations.
- View code must not perform side effects. It should build widgets from immutable state, produce messages, and use shared `ui` helpers for styling and layout. Runtime effects such as focusing inputs, snapping scrollables, opening files, or changing windows happen through `Task`s returned by update functions.
- Long-lived resources are owned by subscriptions or explicit workers, not by widgets. SSE starts only when signed in and stops on logout/session expiration. The voice worker follows the same pattern and reports typed events to the app.
- Keep DTO names and broadcast-event names close to the server contract. UI-specific view models can live in feature modules, but wire-format types belong in `protocol`.
- Small modules may temporarily collapse `state`, `update`, and `view` into a single `mod.rs` during spikes, but the public shape should still follow the convention above so issues can be implemented independently without restructuring.

## Implementation Decisions

- Build the native Iced client as a new application alongside the existing Tauri/Solid client. Do not remove the current client until the native client reaches a defined parity milestone.
- Keep the existing Hamlet server as the source of truth. The native client will consume the same session-cookie HTTP API and the same authenticated SSE stream.
- Do not introduce schema changes for the initial native client conversion. Users, sessions, channels, messages, embeds, avatars, and voice state remain server-owned.
- Use an Elm-style Iced architecture: top-level state, typed app messages, update functions, view functions, Iced tasks for async requests, and Iced subscriptions for passive streams.
- Model navigation with a small native route enum rather than a browser router. Initial routes are login and text channel.
- Preserve the existing authentication flow: login, register, logout, session validation through the profile endpoint, and server URL configuration.
- Store native client preferences in a desktop-appropriate config store rather than browser local storage. Preferences include server URL, voice settings, and any UI preferences introduced by the native client.
- Use an HTTP API transport module as a deep module. Its public responsibility is to expose typed Hamlet API methods while encapsulating base URL resolution, cookie handling, JSON/multipart encoding, response validation, and error mapping.
- Use a protocol/DTO module as a deep module. It defines Rust types matching the server's user, channel, message, embed, typing, voice, and broadcast-event payloads.
- Consider extracting shared protocol types later so the server and native client compile against the same DTO/event definitions. This is optional for the first spike because it would touch server architecture.
- Use a dedicated SSE module as a deep module. Its public responsibility is to produce typed broadcast events and connection-state events while encapsulating SSE frame parsing, initial connected-frame handling, ping-comment handling, cookie-authenticated requests, and reconnect/backoff logic.
- Use an auth state module to represent signed-out, session-checking, signed-in, login-submitting, register-submitting, and auth-error states.
- Use a channels state module to represent channel loading, loaded channels, channel creation, channel reorder, optimistic reorder rollback, and channel events from SSE.
- Use a chat state module to represent current channel selection, per-channel message lists, message loading state, message draft, message actions, typing indicators, and message updates from SSE.
- Use a voice state module to represent inactive, connecting, connected, muted, deafened, speaking, participant lists, and voice errors.
- Implement voice through a long-lived worker/subscription rather than directly inside view code. The worker owns LiveKit room state and native audio/device interaction, while the app sends commands and receives typed voice events.
- Treat voice as a high-risk milestone. The LiveKit Rust SDK supports Rust client functionality and major desktop platforms, but native microphone capture, remote playback, device selection, output routing, input gain, noise suppression, and OS permissions must be proven before voice parity is promised.
- Implement a storage module as a deep module. Its public responsibility is to load and save typed native client configuration while hiding platform-specific paths and persistence details.
- Implement an avatar/image module as a deep module. It should handle relative URL resolution, image fetching/cache, fallback avatar generation, local file selection, and optional crop/resize behavior.
- For the avatar upload MVP, rely on the server's existing center-crop and resize behavior if native cropping is not ready. Native crop UI can be added later.
- Implement the UI as composable Iced view modules: login, shell/sidebar, channel view, message row, settings, voice channel row, modal/popover widgets, emoji picker, and reusable avatar/image widgets.
- Recreate Hamlet's dark sidebar/light channel visual identity through an explicit native theme module. Tailwind classes will not be ported directly.
- Implement embeds natively where feasible: link cards, photo previews, title/site/description rendering, image previews, and external-open actions.
- Do not attempt to embed arbitrary provider iframes in Iced. Video/rich iframe embeds should degrade to a native preview card with an external-open action.
- Replace browser EventSource reconnect behavior with explicit native reconnect/backoff handling.
- Replace browser fetch credentials behavior with a cookie-capable native HTTP client.
- Replace browser file inputs with native file dialogs.
- Replace browser drag-and-drop channel reordering with either native pointer-driven reordering or an MVP-accessible reorder interaction such as move up/down controls.
- Replace DOM modals, focus traps, and portals with native modal/popover state. Keyboard behavior must be modeled explicitly.
- Replace browser localStorage voice settings with native persisted settings.
- Resolve relative upload/avatar URLs against the configured Hamlet server base URL.
- Preserve the server's broadcast-event taxonomy exactly so the native client can consume events produced for the existing client.
- Preserve the current message semantics: no pagination in the initial port, no timestamps unless the server adds them later, and no client-generated message IDs.
- Preserve the current typing semantics: send best-effort typing pings with throttling and expire typing indicators client-side.
- Preserve the current channel semantics: voice channels do not have message views, and the default route should choose the first text channel.
- Preserve the current voice server flow: request a token, connect to the returned LiveKit room, fetch participant list, maintain participants via voice SSE events, and post speaking transitions to the server.
- Package the native client separately from the current Tauri app. Candidate tools include Rust-native release packaging tools and platform-specific installers.
- Add platform-specific app metadata and permissions when voice is enabled, especially microphone permission metadata on macOS.
- Keep the native client's first usable milestone scoped to text chat. Voice may be feature-gated or omitted from the first alpha if the feasibility spike exposes significant risk.

## Testing Decisions

- Good tests should assert external behavior and stable module contracts, not internal implementation details or incidental widget structure.
- Prefer tests around deep modules with simple public interfaces: API transport, protocol/DTO serialization, SSE parsing/reconnect, app reducers, storage, avatar/image handling, and voice worker command/event behavior.
- Test the API transport with a mock HTTP server. Cover login/register/logout, profile fetch/update, channel list/create/reorder, message list/send/edit/delete, suppress embeds, typing pings, avatar multipart upload/delete, voice token fetch, participant list, and speaking updates.
- Test cookie behavior at the transport boundary: session cookies from login/register must be reused for authenticated requests, and unauthorized responses must be surfaced as auth/session failures.
- Test base URL handling at the transport boundary: configured server URLs must be normalized consistently, and relative upload/avatar URLs must resolve against the configured server.
- Test server error mapping at the transport boundary: JSON error bodies should become typed client errors, and empty unauthorized responses should be handled correctly.
- Test protocol serialization/deserialization for all DTOs consumed from the server, including users, channels, messages, embeds, voice participants, typing events, and broadcast events.
- Test the SSE parser with raw SSE input. Cover data frames, the initial connected payload, ping comments, malformed JSON, unknown event kinds if supported, and stream termination.
- Test SSE reconnect behavior with a fake stream source. Cover successful reconnect, backoff timing decisions, auth expiration, and shutdown when the user logs out.
- Test app reducer behavior without rendering where possible. Cover boot/session check, login success/failure, logout, route changes, channel loading, channel events, message loading, message events, typing expiration, and profile updates.
- Test optimistic channel reorder externally: local order changes immediately, successful server response commits, failed server response rolls back or refetches.
- Test chat behavior externally: incoming messages append to the active channel, events for other channels do not mutate the active message list, edits replace the correct message, deletes remove the correct message, and embed updates patch the correct message.
- Test typing behavior externally: typing pings are throttled, empty drafts do not ping, and indicators expire after the configured timeout.
- Test settings/profile behavior externally: display-name update success changes visible profile state, validation errors are shown, avatar upload/delete refreshes profile state, and logout clears signed-in state.
- Test avatar/image behavior with local fixtures. Cover fallback avatar determinism, relative URL resolution, image fetch/cache behavior if implemented, and optional crop/resize output if native cropping is added.
- Test voice state behavior as a reducer/worker contract before testing real audio. Cover join command, connected event, mute/deafen commands, speaking events, participant join/leave events, disconnect cleanup, and error handling.
- Add a feature-gated LiveKit integration/manual test path once voice implementation begins. Real microphone, speaker, and device selection behavior should be validated manually or with dedicated integration tooling because it depends on OS permissions and audio hardware.
- Use existing client tests as behavioral prior art. The current client already has integration coverage for login, channel flows, SSE message updates, edit/delete, typing, emoji, and embeds using a fake backend and fake EventSource.
- Use existing server tests as contract prior art. The server already tests auth, channels, messages, avatars, voice behavior, and broadcaster events; the native client should align with those contracts instead of inventing parallel semantics.
- Maintain a manual QA checklist for native-only behavior that automated DOM tests no longer cover: keyboard focus, modal dismissal, popovers, channel reorder interactions, file dialogs, image previews, window resizing, high-DPI rendering, app shutdown cleanup, and microphone permissions.
- Do not attempt to reproduce the old browser accessibility test stack directly. Instead, define native keyboard and focus acceptance criteria and test reducer-level state where possible, with manual inspection for screen-reader and OS accessibility behavior until better Iced tooling is selected.

## Out of Scope

- Rewriting the Hamlet server API specifically for the native client.
- Changing the database schema as part of the native client conversion.
- Implementing persistent server-side SQLite storage; that is a separate backend concern.
- Adding message pagination, timestamps, search, reactions, threads, roles, permissions, or moderation workflows unless handled by separate product work.
- Removing the existing Tauri/Solid client before the native client reaches an agreed parity milestone.
- Rendering arbitrary web iframes inside the Iced UI.
- Guaranteeing full browser accessibility/tooling parity in the first native alpha.
- Guaranteeing full voice parity before the LiveKit/native audio feasibility spike is complete.
- Recreating Tailwind or DOM styling semantics in Rust.
- Supporting mobile platforms as part of this desktop native conversion.
- Building a separate offline mode or local-first cache beyond ordinary config and optional image caching.
- Introducing end-to-end encryption or changing Hamlet's authentication model.

## Further Notes

- The conversion is attractive because Hamlet's application logic already uses HTTP and SSE rather than Tauri IPC. This makes a native Iced client feasible without changing most server contracts.
- Iced is a good conceptual fit for the current provider/resource architecture because Solid contexts map cleanly to explicit Rust state modules and Iced subscriptions.
- The biggest risks are not basic UI or HTTP; they are native voice, iframe/embed parity, DOM accessibility/testing parity, native drag-and-drop behavior, and large message-list performance.
- The native client should start with a text-chat alpha so the project can validate the Iced architecture and user experience before tackling full LiveKit voice parity.
- Voice should be treated as its own milestone with a prototype-first gate. If native audio proves too costly, Hamlet can still ship a useful native text client while preserving the existing Tauri/Solid client for voice during the transition.
- A future shared protocol crate would reduce drift between server and native client DTOs, but it should be introduced carefully to avoid coupling the server to an unfinished client architecture too early.
- The current server's in-memory development database and seeded users remain useful for native client development and smoke testing.
