# SolidJS to React Client Rewrite PRD

## Problem Statement

Hamlet's Electron renderer is currently implemented with SolidJS, Solid-specific routing, Solid-specific testing helpers, and Solid runtime dependencies. The product goal is to move the desktop/web renderer to React while preserving the existing Hamlet user experience, Rust HTTP API contract, SSE real-time behavior, LiveKit voice/video behavior, Electron shell security model, packaging workflow, and local developer/test commands. Users should experience the rewrite as a framework migration, not as a product reset or protocol change.

The rewrite is complete only when no Solid runtime, Solid tooling, Solid source patterns, or Solid test utilities remain in the client. Keeping a mixed Solid/React renderer would leave duplicate mental models, larger bundles, and migration risk for future Hamlet features.

## Solution

Rewrite the Hamlet Electron renderer as a React application with equivalent routes, state ownership, real-time subscriptions, accessibility semantics, media behavior, tests, and build outputs. The existing Rust server remains the source of truth for authentication, channels, messages, read states, reactions, embeds, custom emoji, mentions, attachments, typing, threads, voice state, camera streams, screen shares, CSRF, cookies, and SSE events. The Electron main/preload contracts remain unchanged: the renderer continues to load from the configured loopback origin, normal Hamlet APIs continue to use HTTP/SSE/WebRTC directly from the renderer, and the preload boundary remains intentionally minimal.

The implementation should migrate deep modules first where possible: API clients, pure message/read-state/reaction/emoji/linkification helpers, media control adapters, and test fixtures should remain stable and framework-agnostic. UI and provider layers should be rewritten to React idioms using explicit hooks, reducers, contexts, effects, and component tests that assert user-visible behavior rather than implementation details.

## User Stories

1. As an existing Hamlet desktop user, I want the app to launch after the rewrite, so that I can keep using Hamlet without learning a new product.
2. As an existing Hamlet web-renderer user, I want the same routes to load, so that links and bookmarks continue to work.
3. As a user, I want the login page to keep accepting a configurable server URL, username, and password, so that I can connect to my chosen Hamlet server.
4. As a user, I want registration to keep working when the server allows it, so that I can create an account from the client.
5. As a user, I want disabled-registration errors to remain clear, so that I understand when account creation is unavailable.
6. As a user, I want saved loopback server URLs and cookies to keep working, so that I do not lose sessions because of the rewrite.
7. As an authenticated user, I want route guards to keep redirecting me away from login and into the app, so that navigation feels unchanged.
8. As an unauthenticated user, I want protected routes to redirect to login, so that private chat data is not shown.
9. As a user, I want logout to clear my visible session state, so that I can switch accounts safely.
10. As a user, I want the channel sidebar to list text and voice channels, so that I can navigate the workspace.
11. As a user, I want channel creation to keep working, so that I can add new places to chat.
12. As a user, I want channel ordering updates to appear in real time, so that the sidebar matches other clients.
13. As a user, I want opening the root route to select the first text channel, so that the app lands in a useful view.
14. As a user, I want a channel header to show the current channel name, so that I know where I am posting.
15. As a user, I want message history to load for a channel, so that I can read prior conversation.
16. As a user, I want messages to render authors, timestamps, content, attachments, embeds, replies, reactions, mentions, and deleted states correctly, so that conversation context is preserved.
17. As a user, I want new SSE messages to appear without refreshing, so that chat remains real time.
18. As a user near the bottom of a message list, I want new messages to auto-scroll into view, so that live chat remains convenient.
19. As a user reading older messages, I want new messages to show a jump-to-bottom affordance instead of yanking my scroll position, so that I do not lose my place.
20. As a user, I want sending a text message to work with the same API and validation behavior, so that chat remains reliable.
21. As a user, I want photo attachment selection, preview, removal, and upload behavior to remain unchanged, so that image sharing still works.
22. As a keyboard user, I want the composer to keep focus after sending or canceling replies, so that I can chat efficiently.
23. As a user, I want typing notifications to appear and expire correctly, so that I can tell when others are composing.
24. As a user, I want inline replies to target top-level messages, so that threaded context is clear.
25. As a user, I want reply previews to update or disappear when target messages are edited or deleted, so that stale references are not misleading.
26. As a user, I want thread panels to open from message actions and URLs, so that I can follow side conversations.
27. As a user, I want thread replies and thread summaries to update in real time, so that thread counts and latest replies remain accurate.
28. As a user, I want invalid thread URLs to return me to the channel view, so that broken thread links do not strand me.
29. As a user, I want message edit, delete, embed suppression, and embed refresh behavior to remain intact, so that message management works as before.
30. As a user, I want reactions to add, remove, summarize, and update in real time, so that lightweight feedback remains accurate.
31. As a user, I want user mentions to autocomplete, render, and survive message updates, so that calling attention to people still works.
32. As a user, I want custom emoji to load, search, render, and update in real time, so that workspace expression remains available.
33. As a user, I want unread markers and read-state updates to preserve the current viewport rules, so that read tracking is accurate.
34. As a user, I want read state to update when I view the bottom of a channel, focus the window, or return to a visible tab, so that unread badges clear predictably.
35. As a user, I want unread state from other channels to update through events, so that the sidebar reflects activity.
36. As a user, I want accessible labels, live regions, roles, focus rings, and modal behavior to remain intact, so that assistive technologies continue to work.
37. As a user, I want the 404 page to remain available for unknown routes, so that navigation errors are understandable.
38. As a user, I want renderer error boundaries to show useful failure messages, so that recoverable failures do not blank the app.
39. As an operator, I want Sentry renderer telemetry to remain opt-in and privacy-preserving, so that deployments can observe failures without leaking body content.
40. As a voice user, I want joining and leaving voice channels to keep working through LiveKit, so that voice chat remains usable.
41. As a voice user, I want mute and deafen controls to serialize updates correctly, so that rapid clicks do not create stale server state.
42. As a voice user, I want speaking indicators to update from LiveKit/server events, so that participants can see who is talking.
43. As a voice user, I want selected microphone, input gain, and noise suppression settings to persist, so that my audio setup survives app restarts.
44. As a voice user, I want camera preview and publishing to keep working, so that video calls remain available.
45. As a voice user, I want remote camera tiles to appear, sort, update, and disappear correctly, so that I can see active camera participants.
46. As a screen-share presenter, I want starting and stopping screen share to use the existing trusted Electron/browser paths, so that screen sharing remains secure.
47. As a screen-share viewer, I want discovering, watching, switching, and stopping remote screen shares to keep working, so that collaboration remains usable.
48. As an Electron user, I want media permission behavior to remain governed by the existing shell policy, so that the renderer rewrite does not weaken security.
49. As an Electron user, I want external links and blocked top-level navigations to behave unchanged, so that the shell remains safe.
50. As an Electron user, I want packaged static renderer mode to keep serving the built app on the configured loopback origin, so that packaged dogfooding still works.
51. As an Electron user, I want single-instance and renderer-port conflict behavior to remain unchanged, so that launch failures stay predictable.
52. As a developer, I want the client build, dev server, preview server, Electron dev launch, and package commands to keep their public behavior, so that existing workflows keep working.
53. As a developer, I want Vite, Vitest, Playwright, oxlint, oxfmt, TypeScript, and size checks to run against React, so that the migrated stack is maintainable.
54. As a developer, I want no Solid dependencies, plugins, imports, JSX typings, or testing libraries to remain, so that future work has one framework model.
55. As a developer, I want framework-agnostic helper modules to remain covered by unit tests, so that the rewrite does not obscure business-rule regressions.
56. As a developer, I want React context and hook layers to be tested through behavior, so that tests tolerate internal refactors.
57. As a developer, I want MSW-backed integration tests to keep exercising auth, channels, messages, threads, media state, and real-time events, so that API contracts stay honest.
58. As a QA tester, I want existing renderer and Electron E2E smoke flows to pass, so that login, sending messages, shell launch, and media flows are validated end to end.
59. As a maintainer, I want bundle size to stay within budget after replacing Solid with React, so that the rewrite does not silently bloat the client.
60. As a maintainer, I want the migration to land with clear validation gates, so that the branch is not considered done until Solid removal and parity checks are verified.

## Implementation Decisions

- The rewrite targets the renderer client only. The Rust API, database schema, server routes, SSE payload shapes, authentication cookies, CSRF behavior, LiveKit token/status endpoints, and persistent data model are unchanged.
- Electron main, preload, shell security, display capture trust boundaries, static renderer serving, package identity, renderer origin rules, and local profile behavior remain unchanged except for build wiring necessary to load the React renderer output.
- The final client dependency graph must use React and React-compatible libraries for rendering, routing, component tests, and dev tooling. Solid runtime packages, Solid router, Solid Vite plugins, Solid devtools, Solid Sentry package, Solid icons, Solid testing utilities, and Solid-specific source imports must be removed or replaced.
- The Vite and Vitest configuration should be converted to React tooling while preserving renderer host/port configuration, strict port behavior, source-map flags, default server URL injection, browser conditions needed by tests, and existing test inclusion patterns.
- The app shell should be implemented as a React route tree with an authentication boundary, nested providers, suspense/loading handling where appropriate, and error boundaries that preserve the current user-visible recovery affordances.
- Authentication should be represented by a React provider/hook that owns current-user loading, login, register, logout, refresh, and server URL selection while continuing to use the existing HTTP API client and credentialed fetch behavior.
- Real-time events should be represented by a deep event hub module plus a React provider/hook. The event hub should encapsulate EventSource lifecycle, typed SSE dispatch, connected notifications, malformed-payload handling, listener registration, and cleanup behind a stable subscription interface.
- Channels should be represented by a React provider/hook that fetches channel state, listens for channel creation and reorder events, and exposes mutation/refresh behavior without leaking framework-specific details to the rest of the UI.
- Read states should be represented by a React provider/hook and pure viewport/read-state helpers. The provider should preserve mark-read debounce behavior, connected-event refresh behavior, and event-driven unread updates.
- Custom emoji should be represented by a React provider/hook backed by the existing API contracts and pure search/shortcode helpers. Event-driven create/update/delete behavior should remain centralized.
- Message list state should be managed with React-friendly immutable updates or a reducer that preserves the existing semantics for initial fetches, SSE appends, updates, deletes, embed updates, reaction updates, mention cache priming, thread summary updates, reply target invalidation, and scroll-follow behavior.
- Composer state should be rewritten around React controlled inputs and refs while preserving typing throttling, photo validation, photo preview/removal, inline reply targeting, submit disabling, focus restoration, and accessibility descriptions.
- Thread panel state should remain URL-addressable and should validate that root messages exist in the current channel before rendering an open thread.
- Voice chat should be implemented as a React provider/hook with an internal imperative LiveKit adapter. The adapter should encapsulate room lifecycle, track publication cleanup, screen-share and camera state, remote track mapping, audio routing, settings application, speaking state transitions, and serialized control updates.
- Media UI components should consume voice context state and actions without owning LiveKit rooms directly, so that rendering tests can use fake context values and media logic can be tested separately.
- Sentry should be migrated to the React SDK while preserving opt-in initialization and the current privacy posture.
- Icons should be replaced with React-compatible icon components or local SVG components while preserving visual meaning and accessible labels.
- Styling should continue to use the existing CSS and utility classes unless a specific component requires a small structural adjustment for React.
- TypeScript types for API payloads should remain shared by API modules and UI layers. Framework migration should not weaken payload typing or replace typed discriminated SSE unions with untyped event handling.
- Tests should migrate from Solid testing utilities to React Testing Library while preserving MSW fixtures, fake SSE helpers, axe checks, user-event tests, and Playwright flows.
- Completion requires a repository search and package audit showing no remaining Solid code, dependencies, tool plugins, JSX runtime assumptions, or Solid-specific testing helpers in the client.

## Testing Decisions

- Good tests should assert external behavior visible to users, API callers, Electron shell boundaries, and accessibility tooling. They should not assert React implementation details such as hook ordering, internal reducer action names, component private state, or exact provider composition unless that composition is part of a public test harness.
- Pure helper modules for linkification, mentions, emoji search/shortcodes, reaction summaries, read-state transitions, viewport read markers, photo validation, voice camera sorting, screen-share matching, audio routing decisions, and API URL/CSRF behavior should keep or gain focused unit tests.
- React component tests should cover avatar rendering, channel sidebar behavior, message input/composer behavior, message text and embeds, reply previews, modals/settings, emoji picker behavior, channel messages, voice controls, camera tiles, screen-share viewer behavior, and typing indicators through accessible queries and user events.
- Provider/integration tests should cover auth state transitions, channel route behavior, thread routing, custom emoji updates, events/SSE dispatch, read-state updates, and voice-chat state transitions using MSW and fake media/event fixtures.
- Accessibility tests should continue using axe against representative app surfaces and should preserve coverage for modal focus behavior, message controls, composer affordances, live regions, and navigation landmarks.
- Renderer E2E tests should continue validating login, registration/auth flows, channel navigation/reorder, sending messages, mentions, reactions, custom emoji, photo upload, and browser renderer smoke behavior against the Rust server.
- Electron E2E tests should continue validating shell launch, renderer origin, package smoke behavior, trusted display-media path, and Electron-specific media automation where available.
- Voice browser E2E should continue validating LiveKit-backed camera and screen-share flows with prerequisite skips for unavailable media infrastructure.
- Static checks for the migrated client must include formatting, linting, TypeScript typechecking, Vitest, relevant Playwright suites, and size budget checks. The expected default gates are `npm run fmt`, `npm run lint`, `npm run typecheck`, `npm run test`, `npm run test:e2e` when smoke-tested flows are affected, and `npm run size` for bundle-budget confidence.
- A Solid-removal check should be part of final validation: dependency manifests, lockfile, source, tests, configs, and docs that describe the current client stack should no longer reference Solid as active renderer technology.

## Out of Scope

- Rewriting the Rust server or changing any server endpoint, SSE schema, authentication/session contract, CSRF behavior, database schema, or LiveKit server integration.
- Changing Hamlet's product surface beyond framework-parity fixes required by the rewrite.
- Introducing a new Electron IPC API for normal Hamlet application traffic.
- Changing the packaged app identity, signing/notarization posture, installer strategy, release channels, or public distribution plan.
- Bundling, supervising, or auto-starting the Rust server from Electron.
- Redesigning the UI, replacing Tailwind-style utility styling wholesale, or introducing a new design system.
- Migrating to a different desktop shell, backend protocol, state-management framework, or test runner beyond React-compatible replacements required by the rewrite.
- Keeping an incremental mixed Solid/React production renderer after the rewrite is declared complete.

## Further Notes

- This PRD was synthesized without a clarification interview per instruction. The referenced rewrite investigation plan path was not present in this worktree during drafting, so the PRD is based on the repository guidance, client documentation, current renderer code, current Electron contracts, and current test stack.
- The implementation should prefer small parity slices with continuously green checks over a long-lived untestable rewrite branch.
- Because the migration touches nearly every renderer component, strong module boundaries around API, events, message state, read-state logic, emoji/search helpers, and voice media adapters are important to keep tests readable and future Hamlet work maintainable.
