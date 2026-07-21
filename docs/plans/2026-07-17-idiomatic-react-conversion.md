# Idiomatic React conversion implementation plan

Date: 2026-07-17
Status: Ready for implementation handoff
Related specification: [`../solid-to-react-client-rewrite-prd.md`](../solid-to-react-client-rewrite-prd.md)

## Goal

Finish the renderer migration by removing the Solid-shaped compatibility model from `client/src/` and replacing it with ordinary React state, effects, contexts, reducers, refs, rendering, and tests.

The migration is complete when:

- `client/src/hooks/react-state.tsx` and its test-only static-signal machinery are gone.
- Components consume render snapshots as values rather than callable signal getters.
- Long-lived browser, SSE, and LiveKit callbacks use explicit refs, reducers, or external stores when they need the latest state.
- Every effect has intentional ownership, dependencies, and cleanup.
- Contexts expose plain values and stable actions.
- Message/thread realtime state has one authoritative reducer per view.
- LiveKit is isolated behind an imperative adapter rather than mixed into React render state.
- Tests use normal React Testing Library harnesses and do not invoke hooks outside components.
- React hook, dependency, key, and type-safety lint rules are blocking.

This is a behavior-preserving renderer refactor. It is not a product redesign.

## Starting point

The renderer already uses React 19, React Router, the React Vite plugin, React Testing Library, and `@sentry/react`. There are no Solid runtime dependencies left. The remaining migration debt is source-level:

- `client/src/hooks/react-state.tsx` is a 400-line compatibility layer.
- 44 files import it directly: 30 production files and 14 tests/helpers.
- Current call-site inventory includes approximately:
  - 155 `useSignalState` calls;
  - 13 `useComputedValue` calls;
  - 7 `useCallableResource` calls;
  - 26 `useAfterRenderEffect` calls;
  - 8 `useMountEffect` calls;
  - 17 `registerCleanup` calls;
  - 150 `<If>` sites, 24 `<List>` sites, 10 `<Case>` sites, and 4 `<Choose>` sites;
  - one `useStoreState` message collection and one no-op `preserveIdentity` call.
- `client/src/test/testing-library.tsx` subscribes every test root to a global static-signal rerender bridge.
- Several tests deliberately call compatibility hooks outside React components.
- `client/src/test/setup.ts` globally hides React `act()` diagnostics.
- The application itself is mounted in `StrictMode` in `client/src/index.tsx`, but most tests do not exercise Strict Mode lifecycle replay.

Baseline at plan creation:

```text
pnpm run lint       pass
pnpm run typecheck  pass
pnpm run test       50 files / 757 tests pass
```

## Scope

### In scope

- Renderer source, tests, Storybook stories, and active client documentation.
- React state/context/effect/resource architecture.
- Pure reducers needed to make SSE plus HTTP snapshot state deterministic.
- An internal imperative LiveKit adapter, as already called for by the rewrite PRD.
- Test harness and lint enforcement changes.
- Fixing lifecycle, race, and identity bugs exposed by replacing the compatibility semantics.

### Out of scope

- Rust API, database, migrations, or SSE wire-format changes.
- Electron IPC, preload, security policy, package identity, or renderer-origin changes.
- Route URL changes.
- Visual redesign or CSS token changes.
- Adding a third-party state/query framework solely for this conversion.
- Weakening accessibility behavior to simplify component rewrites.
- Replacing LiveKit or adding `@livekit/components-react`.

## Required behavior invariants

Every phase must preserve these externally visible contracts unless a test demonstrates that the current behavior is a bug explicitly corrected by this plan:

1. Auth remains a three-state boundary: unresolved, anonymous, or authenticated.
2. Downstream authenticated providers do not mount while auth is unresolved or anonymous.
3. The root route selects the first text channel; protected routes redirect to login.
4. Channel history never shows rows from a different channel.
5. HTTP responses and SSE delivery for the same message/reply do not create duplicates.
6. SSE updates delivered during an in-flight snapshot are not lost when that snapshot resolves.
7. Hard-deleted messages/replies are not resurrected by stale history or pagination.
8. Channel switching clears the inline reply target but preserves the main composer draft and selected photos, matching existing integration coverage.
9. Thread-local drafts, photos, menus, and pending operations never leak to another thread root.
10. Scroll-follow, jump-to-bottom, mark-read debounce, typing expiry, and composer focus behavior remain intact.
11. Object URLs, timers, EventSources, DOM listeners, media tracks, `AudioContext`s, animation frames, LiveKit rooms, and publication listeners have balanced cleanup.
12. Mute/deafen operations remain serialized and preserve the current deafen-forces-mute behavior.
13. Camera and screen-share discovery may race LiveKit publication events; either arrival order must converge on the same UI.
14. Existing labels, roles, live regions, focus traps, keyboard behavior, and axe coverage remain intact.
15. No React warning is solved by globally filtering `console.error` in the final state.

## Architecture decisions

These decisions avoid replacing one compatibility facade with another:

1. **Keep `react-state.tsx` until its last consumer is migrated.** Remove exports incrementally only when no call sites remain; delete the file in a logic-free final cleanup.
2. **Use direct React values in public APIs.** Context state is exposed as `user`, `channels`, `isMuted`, etc., never `user()` or `isMuted()`.
3. **Use render-time derivation by default.** Use a plain constant first, `useMemo` only when a computation is materially expensive or referential stability is part of a child/context contract.
4. **Use refs only for imperative/latest-value needs.** A ref is appropriate for a timer, DOM node, current request generation, active media handle, or state needed by a long-lived callback. It is not a substitute for render state.
5. **Use reducers where snapshots and realtime events share ownership.** Channel messages, thread messages, and voice-channel presence require explicit transitions and stale-operation handling.
6. **Keep the generic resource hook focused on request lifecycle.** Domain-specific SSE reconciliation belongs in providers/reducers, not in a hidden generic cache.
7. **Isolate LiveKit behind a `VoiceSession`.** The session owns rooms, publications, tracks, queues, and stale-operation epochs; React observes an immutable snapshot through `useSyncExternalStore`.
8. **Effects own the resources they create.** Setup and teardown live in the same `useEffect`/`useLayoutEffect` return path.
9. **Tests assert behavior, not compatibility APIs.** Stateful test controls live inside harness components or explicit stores; tests do not call hooks at module scope or from ordinary setup functions.
10. **No broad lint quarantine.** Temporary exceptions name exact legacy files and are removed as those files migrate.

## Dependency graph and ownership

Recommended critical path:

```text
Phase 1 guardrails
  -> Phase 2 rendering/types
  -> Phase 3 local state/lifecycle
  -> Phase 4 resources/core contexts
  -> Phase 5 messages/threads ----\
  -> Phase 6 voice ---------------+-> Phase 7 final cleanup/enforcement
```

Phases 5 and 6 may proceed in parallel only after Phase 4 context contracts are stable. They must have explicit ownership for shared files:

- `client/src/App.tsx`
- `client/src/components/channel-sidebar.tsx`
- `client/src/components/settings-modal.tsx`
- `client/src/components/message-input.tsx`
- `client/src/test/render.tsx`

Do not split these high-conflict clusters between simultaneous workers:

- Message cluster: `pages/channel.tsx`, `components/channel-messages.tsx`, `components/thread-panel.tsx`, `components/composer-photo-selection.tsx`, `components/message-input.tsx`.
- Voice cluster: `contexts/voice-chat.tsx`, `components/voice-channel.tsx`, `components/voice-settings.tsx`, media tile/viewer components, and their structural mocks.
- Context cluster: a provider, every direct consumer, and all test mocks for its public value shape.

## Agent working protocol

An agent taking a phase should:

1. Read this plan, `AGENTS.md`, `client/README.md`, and the files listed for that phase.
2. Confirm a clean worktree with `git status --short`.
3. Run the focused baseline tests before changing behavior-sensitive files.
4. Keep the phase buildable; do not leave mixed old/new context contracts in a landed commit.
5. Add tests with the implementation, especially for races and cleanup that the shim previously hid.
6. Remove temporary lint overrides for each migrated file in the same commit.
7. Run, at minimum, from `client/`:

   ```bash
   pnpm run fmt
   pnpm run lint
   pnpm run typecheck
   pnpm run test
   ```

8. Run the phase-specific E2E/build/size checks listed below.
9. If any repository check fails, fix it before handoff, even if the failure appears pre-existing.
10. Report changed files, behavior decisions, tests run, and any deferred items explicitly.

---

# Phase 1 — Establish migration guardrails

## Objective

Make new React debt detectable without forcing a big-bang rewrite, and add test tools capable of exercising native React/Strict Mode behavior independently of static signals.

## Prerequisites

None.

## Primary files

- `client/oxlint.config.json`
- `client/oxlint.config.ts`
- `client/package.json`
- `client/tsconfig.json`
- `client/src/test/setup.ts`
- `client/src/test/testing-library.tsx`
- `client/src/test/render.tsx`
- `client/src/test/msw/sse.ts`

## Tasks

### 1.1 Consolidate the active lint configuration

`client/package.json` invokes `oxlint -c oxlint.config.json`; `client/oxlint.config.ts` is not active. Treat the JSON file as authoritative during the migration and delete the unused TypeScript config in Phase 7.

Add Oxlint's built-in `react` plugin and enable:

- `react/rules-of-hooks`
- `react/exhaustive-deps`
- `react/jsx-key`

Use exact-file temporary overrides for existing debt. At minimum, quarantine:

- Compatibility implementation:
  - `src/hooks/react-state.tsx`
- Misnamed hook:
  - `src/components/composer-photo-selection.tsx`
- Tests that currently call compatibility hooks outside React:
  - `src/components/channel-sidebar.test.tsx`
  - `src/components/local-camera-tile.test.tsx`
  - `src/components/modal.test.tsx`
  - `src/components/remote-camera-tiles.test.tsx`
  - `src/components/screen-share-viewer.test.tsx`
  - `src/components/voice-channel.test.tsx`
  - `src/components/voice-status-controls.test.tsx`
- Files with known dependency-free React effects:
  - `src/components/emoji-picker.tsx`
  - `src/components/message-input.tsx`
  - `src/components/message-text.tsx`
  - `src/components/modal.tsx`
  - `src/contexts/read-states.tsx`
  - `src/pages/channel.tsx`

Run the lint command after configuring it and add only additional exact paths actually reported. Every override must include a comment naming the phase that removes it. New paths must not be added after Phase 1 without an explicit plan update.

Do not enable `react/no-array-index-key` globally until Phase 2 has converted the compatibility lists. Do not enable `typescript/no-explicit-any` or `noImplicitAny` globally until Phase 2 has typed the known event handlers.

### 1.2 Add a native Strict Mode render path

Keep the existing static-signal-aware render helper temporarily, because legacy tests still depend on it. Add a separate helper for migrated tests that:

- renders a normal React element, not a callback factory;
- wraps it in `StrictMode` or uses Testing Library's `reactStrictMode` option;
- has no `useStaticSignalRerender` subscription;
- composes `MemoryRouter`/providers without invoking UI callbacks eagerly.

New tests and any test migrated in Phases 2–6 should use the native helper. The legacy helper must not gain new consumers.

### 1.3 Make React diagnostics observable

Inventory the warnings currently hidden by `client/src/test/setup.ts`. Do not replace the filter with another broad filter.

Preferred sequence:

1. Add a temporary test-only counter/spy that can be enabled for a focused test to assert no unexpected React diagnostics.
2. Fix warnings in each migrated test file as it moves to the native helper.
3. Remove the global filter completely in Phase 7 after the legacy static tests are gone.

Externally driven fake SSE updates should be delivered inside `act`. Keep that responsibility in the fake SSE helper so every consumer does not need bespoke wrapping.

### 1.4 Record migration searches

Add the following commands to the phase handoff notes; they become final zero-result gates in Phase 7:

```bash
rg -n "hooks/react-state|useSignalState|useCallableResource|useStaticSignalRerender" client/src
rg -n "<If|<List|<Choose|<Case" client/src --glob '*.tsx'
rg -n "from ['\"]solid|solid-js" client client/pnpm-lock.yaml
```

Do not fail CI on the first two searches yet; they document the burn-down.

## Tests

- Existing full unit suite.
- One small test proving the new native render helper runs effect setup/cleanup under Strict Mode.
- One small test proving fake SSE delivery is wrapped in `act` without relying on the global warning filter.

## Acceptance criteria

- The React plugin and hook/key rules run for all non-quarantined files.
- Temporary overrides are exact, commented, and tied to later phases.
- A native Strict Mode test render path exists and has no static-signal bridge.
- No production behavior changes.
- Standard format, lint, typecheck, and test commands pass.

## Suggested commit

`test(client): add React migration lint and StrictMode guardrails`

---

# Phase 2 — Replace compatibility rendering and types

## Objective

Remove the syntax-level Solid compatibility surface while deliberately leaving state, resources, and lifecycle behavior unchanged. This phase should be mostly mechanical and easy to review.

## Prerequisites

Phase 1.

## Production files in scope

Pages/shell:

- `client/src/pages/login.tsx`
- `client/src/pages/threads.tsx`
- `client/src/pages/channel.tsx`

General components:

- `client/src/components/add-channel-modal.tsx`
- `client/src/components/attachment-grid.tsx`
- `client/src/components/avatar.tsx`
- `client/src/components/channel-messages.tsx`
- `client/src/components/channel-sidebar.tsx`
- `client/src/components/composer-photo-selection.tsx`
- `client/src/components/cropper-dialog.tsx`
- `client/src/components/emoji-picker.tsx`
- `client/src/components/message-embed.tsx`
- `client/src/components/message-reference-preview.tsx`
- `client/src/components/modal.tsx`
- `client/src/components/reaction-row.tsx`
- `client/src/components/settings-modal.tsx`
- `client/src/components/thread-panel.tsx`
- `client/src/components/typing-indicator.tsx`

Voice/media presentation:

- `client/src/components/local-camera-tile.tsx`
- `client/src/components/remote-camera-tiles.tsx`
- `client/src/components/screen-share-viewer.tsx`
- `client/src/components/voice-channel.tsx`
- `client/src/components/voice-settings.tsx`
- `client/src/components/voice-status-controls.tsx`

Types/re-exports:

- `client/src/contexts/voice-chat.tsx`
- `client/src/components/message-text.tsx`
- `client/src/hooks/react-state.tsx`

Tests with compatibility rendering:

- `client/src/contexts/custom-emojis.test.tsx`
- `client/src/contexts/voice-chat.test.tsx`
- `client/src/components/channel-sidebar.test.tsx`

## Tasks

### 2.1 Replace `<If>`

Use normal conditional JSX:

- Use a ternary when there is a fallback.
- Use `condition ? <Content /> : null` where the current condition may be `0`, `""`, or another falsey non-boolean value and current truthiness must be preserved.
- Bind a narrowed value once before JSX when a function child currently receives a getter.
- Do not infer remount semantics from the compatibility `keyed` prop. It only changes the callback argument shape today; it does not create a React key.

Examples of behavior that must remain ordered:

- Channel loading/error/data rendering.
- Embed iframe before photo before generic content.
- Settings section selection.
- Empty/error/loading thread states.

### 2.2 Replace `<Choose>` and `<Case>`

Use an ordered helper function, `switch`, early return, or nested ternary. Preserve first-truthy-branch behavior. In particular:

- A channel error wins over retained channel data.
- Embed media precedence does not change.
- Exactly one settings section renders.

Do not create a new generic `Switch` component.

### 2.3 Replace `<List>` and assign domain keys

Use native `.map((item, index) => ...)`. Put the key on the outermost mapped element or keyed fragment.

Required key choices:

| Collection               | Key                                                                                   |
| ------------------------ | ------------------------------------------------------------------------------------- |
| Channels                 | `channel.id`                                                                          |
| Messages and replies     | `message.id`                                                                          |
| Embeds                   | `embed.id`                                                                            |
| Attachments              | `attachment.id`                                                                       |
| Reactions                | `reactionKey(reaction)`                                                               |
| Selected composer photos | `photo.id`                                                                            |
| Custom emojis            | `emoji.id`                                                                            |
| Settings sections        | `section.id`                                                                          |
| Voice participants       | `participant.user_id`                                                                 |
| Screen shares            | `screenShareKey(stream)`                                                              |
| Camera tiles             | `cameraKey(tile.stream)`                                                              |
| Emoji entries            | custom: `custom:${entry.id}`; native: `native:${entry.emoji}`                         |
| Device options           | `${device.kind}:${device.deviceId}`; use a provider-assigned stable fallback if blank |
| Shortcodes               | shortcode string, with a deterministic duplicate suffix only if duplicates are valid  |
| Thread previews          | `thread.root.id`                                                                      |

Update `emojiGridcellId` to use the same custom-ID/native-emoji distinction; shortcode-derived DOM IDs can collide when native and custom emoji share a shortcode.

Update `AutocompleteMenu` in `client/src/components/message-input.tsx` to accept `getOptionKey(option)` rather than using the array index as the React key. Its positional ARIA option ID may remain index-based because it represents current keyboard position, not component identity. Do not use the raw map index as a device key; if the platform returns a blank `deviceId`/`groupId`, assign and retain a synthetic key when normalizing the enumerated device list.

### 2.4 Replace compatibility React types

Remove imports of `Component`, custom `JSX`, `Getter`, and compatibility `ValueUpdater` where state conversion is not required.

Use native React types:

- `ReactNode` for renderable children and rich-text arrays containing strings and elements.
- `CSSProperties` for style objects.
- `HTMLAttributes<...>` for DOM prop extraction.
- `FormEvent`, `ChangeEvent`, `KeyboardEvent`, `MouseEvent`, `PointerEvent`, and `DragEvent` for handlers.
- `Dispatch<SetStateAction<T>>` only where that is the actual contract.

Convert `Component<P>` declarations to named functions or ordinary typed function components. Import `createContext`, `useContext`, and `lazy` from React directly rather than through `react-state.tsx`.

### 2.5 Replace compatibility utilities

- `PortalRoot` in `emoji-picker.tsx` -> direct `createPortal(..., document.body)`.
- `ignoreReactiveTracking` -> remove the wrapper and evaluate the expression normally.
- `useStableDomId` -> `useId`; sanitize only if the ID is interpolated into an unescaped CSS selector. Colons are valid for `id`, `aria-labelledby`, and `getElementById`.
- `preserveIdentity` remains for Phase 5 because its call is coupled to message state, but document that it is a no-op.

### 2.6 Normalize obvious Solid-era prop/type residue

- Remove unused `class?: string` aliases from `MessageInput`, `MessageText`, `MessageReferencePreview`, and `PhotoAttachControl`; retain `className`.
- Use `onChange` for ordinary controlled `<input>`, `<select>`, and `<textarea>` fields. Keep `onInput` for the contenteditable message editor where it is the correct browser event.
- Type all existing production `any` event handlers and any test helpers touched by this phase.
- Do not enable `typescript/no-explicit-any` globally yet: Phase 7-owned tests still contain deliberate explicit `any` compatibility code. Keep exact-file lint overrides and remove them as those tests migrate. Enable `noImplicitAny: true` and make explicit `any` blocking only in Phase 7, after source and tests are clean.

### 2.7 Keep this phase behavior-neutral

Do not change:

- effect dependencies;
- state ownership;
- context shapes;
- resource semantics;
- request cancellation;
- LiveKit behavior;
- message reducer behavior.

Those changes belong to later phases and make a mechanical review harder.

## Safe parallel work packages

1. Presentational content: avatar, message embed, message reference, attachment grid, reaction row.
2. Forms/overlays: add channel, modal, cropper, composer photo preview.
3. Shell/settings: channel sidebar, settings modal, login, threads page.
4. Voice JSX only: voice channel/settings/status and media tiles.
5. Chat JSX only: channel messages, thread panel, channel page.

Each package must include its tests and should not change public state/context contracts.

## Tests

Run focused tests for each package plus the full suite. Add or strengthen:

- A reaction reorder/removal test proving focus/tooltip state stays with the reaction key.
- A remote camera reorder test proving a video element is not reused for the wrong stream.
- A channel reorder test proving each `VoiceChannel`/text row retains identity by channel ID.
- An emoji filter/reorder test proving keyboard selection follows the intended emoji.
- Existing axe tests after markup conversion.

## Acceptance criteria

- No imports or usages remain for `If`, `List`, `Choose`, `Case`, `Component`, `PortalRoot`, `ignoreReactiveTracking`, or the custom `JSX` namespace.
- Every rendered collection has an explicit reviewed key.
- No `class` compatibility prop aliases remain.
- Production event handlers are typed.
- State/resource/effect helpers may still remain; rendering behavior and accessible output are unchanged.
- All temporary lint overrides for rendering/types are removed.

## Suggested commits

- `refactor(client): replace compatibility JSX control flow`
- `refactor(client): use native React types and stable list keys`

---

# Phase 3 — Convert local state and lifecycle ownership

## Objective

Replace isolated signal/getter state and custom cleanup wrappers with native React state/effects before changing shared resource or domain context contracts.

## Prerequisites

Phases 1–2.

## First-wave files

- `client/src/components/add-channel-modal.tsx`
- `client/src/components/attachment-grid.tsx`
- `client/src/components/channel-sidebar.tsx`
- `client/src/components/reaction-row.tsx`
- `client/src/components/settings-modal.tsx`
- `client/src/components/emoji-picker.tsx`
- `client/src/components/typing-indicator.tsx`
- `client/src/components/modal.tsx`
- `client/src/components/cropper-dialog.tsx`
- `client/src/components/composer-photo-selection.tsx`
- `client/src/components/message-reference-preview.tsx`
- `client/src/components/message-text.tsx`
- `client/src/pages/login.tsx`

Defer these ownership-heavy files:

- Resource providers and pages -> Phase 4.
- Channel/thread timeline state -> Phase 5.
- Voice provider/settings/media attachment -> Phase 6.
- Contenteditable editor internals -> Phase 7.

## Tasks

### 3.1 Replace `useSignalState` in leaf components

For each state cell:

1. Replace the getter/setter pair with `useState`.
2. Replace render reads such as `error()` with `error`.
3. Keep functional updates for updates based on prior state.
4. Move any side effects out of functional updater callbacks.
5. Use lazy initializers for localStorage/device-derived initial values where appropriate.
6. Do not add a generic hook that returns `[() => value, setter]`.

Audit every handler that sets state and reads it again synchronously. Native React reads the current render snapshot until the next render. If an operation needs a synchronously updated current value, calculate it before `setState` or store the imperative value in a narrowly scoped ref.

### 3.2 Replace `useComputedValue`

Use a plain constant for inexpensive calculations such as filtered arrays, labels, selected sections, or booleans. Use `useMemo` only for materially expensive search/chunk/map construction or where stable identity prevents an effect/context loop.

Do not preserve getter wrappers around derived values.

### 3.3 Rename and rewrite the composer photo hook

Rename `createComposerPhotoSelection` to `useComposerPhotoSelection` and update:

- `client/src/pages/channel.tsx`
- `client/src/components/thread-panel.tsx`
- `client/src/components/composer-photo-selection.test.tsx`

The hook should return plain values:

```ts
interface ComposerPhotoSelection {
  photos: readonly SelectedComposerPhoto[];
  error: string | null;
  addFiles(...): void;
  removePhoto(id: string): void;
  clearPhotos(): void;
}
```

Ownership requirements:

- Removing a photo revokes only that photo's URL.
- Clearing revokes every selected URL once.
- Unmount revokes every still-owned URL once.
- A ref mirrors the latest owned photo list solely for unmount cleanup.
- Strict Mode setup/cleanup must not revoke a URL that the currently mounted hook still intends to render.

### 3.4 Convert effects by ownership

#### Login server configuration

In `client/src/pages/login.tsx`:

- Fetch public server configuration only when the normalized server URL changes, not when username/password/email changes.
- Use an `AbortController` if the API helper supports it by this point, otherwise a request generation/cancel flag local to the effect.
- Ignore stale completion after server changes/unmount.
- If registration becomes disabled while register mode is open, switch to login in a dependency-complete effect or in the config completion path.
- Type submit events with `FormEvent<HTMLFormElement>`.

#### Modal focus trap

In `client/src/components/modal.tsx`:

- Run focus setup/stack registration only when the modal opens.
- Do not restore/reapply focus on every parent rerender.
- Keep the latest `onClose` callback in a ref or stable callback so callback identity changes do not rebuild the trap.
- Register one keydown listener and return its cleanup from the same layout effect.
- Remove the exact modal instance from `modalStack` and restore the element focused before that instance opened.
- Preserve nested-modal topmost handling.

#### Cropper lifecycle

In `client/src/components/cropper-dialog.tsx`:

- One effect owns the object URL for `[open, file]` and revokes the previous URL on replacement/close/unmount.
- One effect owns the Cropper instance for the current image element/URL and destroys that exact instance on replacement/unmount.
- Saving/error state changes must not recreate the cropper or reset the crop selection.
- Ignore/save-guard asynchronous canvas completion after close or file replacement.

#### Typing indicator

In `client/src/components/typing-indicator.tsx`:

- Subscribe and start the expiry timer in one effect with returned cleanup.
- Reset entries when `channelId` changes so users from the old channel do not linger.
- Use functional state transitions for incoming typing/message events.
- Remove unnecessary `flushSync`; Testing Library should await visible updates.
- Keep injectable `now` support and current-user filtering.

#### Emoji picker

In `client/src/components/emoji-picker.tsx`:

- Scope global Escape/outside-click/resize/scroll listeners to `open`.
- Use refs for current anchor/callbacks if needed to avoid reinstalling listeners for unrelated render changes.
- Derive filtered entries and rows from direct state.
- Reset query/active index deliberately on open and query changes, not through an every-render effect.
- Preserve composition, focus restoration, keyboard grid movement, and portal behavior.

#### Message text preview

In `client/src/components/message-text.tsx`:

- Keep global pointer/key/scroll listeners installed once with current preview read from a ref, or scope them to preview-open state.
- Close the preview when the mentioned user disappears from the current text.
- Preserve the React synthetic `MouseEvent` and button `currentTarget`; do not replace it with `nativeEvent` before consumers have read the anchor.

#### Settings/object URLs

In `client/src/components/settings-modal.tsx`:

- Scope draft resets to the identity of the edited emoji/user rather than every render.
- Scope custom-emoji preview URL ownership to the selected file.
- Do not set React state during unmount solely to clear it.
- Keep async operation completions tied to the row/file/user they started for.

### 3.5 Rewrite affected tests as native harnesses

Any test in a migrated cluster that calls `useSignalState` outside a component must be converted immediately. Use:

- a named harness component with ordinary `useState`;
- an imperative test handle created with a ref when the test must trigger state externally; or
- Testing Library `rerender` with new props.

Wrap external setter calls in `act`. Move migrated tests from the legacy static-signal render helper to the native Strict Mode helper.

## Tests to add

- `modal.test.tsx`: rerendering controlled content while open neither restores outside focus nor loses the focused field.
- `cropper-dialog.test.tsx`: saving/error rerenders preserve one Cropper instance; replacement/close destroys it once.
- `composer-photo-selection.test.tsx`: remove, clear, unmount, and Strict Mode URL ownership.
- `typing-indicator.test.tsx`: channel switch clears old entries and subscriptions/timers are cleaned.
- `login.integration.test.tsx`: password typing does not refetch server config; stale server response cannot override the newest server.
- `emoji-picker.test.tsx`: open/close listener cleanup and focus restoration under rerender.
- `message-text.test.tsx`: mention preview anchors to the clicked button and closes after mention removal.

## Acceptance criteria

- The first-wave files use `useState` and direct render values.
- `useComputedValue`, `useAfterRenderEffect`, `useMountEffect`, and `registerCleanup` are gone from the first-wave files.
- `useComposerPhotoSelection` follows hook naming/rules and exposes values.
- Modal, cropper, login configuration, object URL, typing, and picker lifetimes are dependency-scoped and Strict Mode-safe.
- Migrated tests no longer depend on static signals.
- Remaining compatibility state/effects are confined to resource, message/thread, voice, or explicitly deferred editor files.

## Suggested commits

- `refactor(client): convert leaf signal state to React state`
- `fix(client): make modal cropper and object URL lifecycles React-owned`
- `refactor(client): migrate typing picker and login effects`

---

# Phase 4 — Replace resources and core context contracts

## Objective

Replace callable resources and getter-shaped core context state with a tested React resource primitive, direct values, stable actions, and explicit request/race behavior.

## Prerequisites

Phases 1–3.

## Primary files

Resource/API:

- `client/src/hooks/use-resource.ts`
- New: `client/src/hooks/use-resource.test.tsx`
- `client/src/api/client.ts`
- `client/src/api/auth.ts`
- `client/src/api/channels.ts`
- `client/src/api/emojis.ts`
- `client/src/api/read-states.ts`
- relevant list/get functions in `client/src/api/messages.ts`

Providers:

- `client/src/contexts/auth.tsx`
- `client/src/contexts/events.tsx`
- `client/src/contexts/channels.tsx`
- `client/src/contexts/custom-emojis.tsx`
- `client/src/contexts/read-states.tsx`

Direct consumers/mocks include:

- `client/src/App.tsx`
- `client/src/pages/login.tsx`
- `client/src/pages/channel.tsx`
- `client/src/pages/threads.tsx`
- `client/src/components/channel-sidebar.tsx`
- `client/src/components/settings-modal.tsx`
- `client/src/components/message-input.tsx`
- `client/src/components/message-text.tsx`
- `client/src/components/message-reference-preview.tsx`
- `client/src/components/channel-messages.tsx`
- `client/src/components/thread-panel.tsx`
- `client/src/contexts/read-states.tsx`
- all tests mocking these contexts.

## Tasks

### 4.1 Define and test the resource contract before migrating callers

Replace the current positional overloads with a keyed options API or an equivalently explicit API. Recommended shape:

```ts
type ResourceStatus = "idle" | "loading" | "ready" | "error";

type ResourceState<T> = {
  data: T | undefined;
  status: ResourceStatus;
  error: unknown | null;
  loading: boolean;
  refreshing: boolean;
};

type ResourceControls<T> = {
  refetch(): Promise<T | undefined>;
  update(value: T | undefined | ((current: T | undefined) => T | undefined)): void;
};

function useResource<K extends string | number, T>(options: {
  key: K | null;
  load: (key: K, signal: AbortSignal) => Promise<T>;
  keepDataOnRefetch?: boolean;
}): [ResourceState<T>, ResourceControls<T>];
```

Required semantics:

1. Export `ResourceStatus`, `ResourceState`, and `ResourceControls`; later contexts/reducers consume the shared status type.
2. `key: null` disables fetching, aborts the active request, and returns `{ data: undefined, status: "idle", error: null }` for the disabled key.
3. A key change starts one logical load for the new key and never exposes prior-key data as current data.
4. `refetch()` starts exactly one request; remove the current `load()` plus `reloadTick` double-fetch behavior.
5. Latest-started request wins, including overlapping same-key refetches.
6. Key change and unmount abort work where supported and always prevent stale commits.
7. Abort is not surfaced as a user-visible error.
8. A same-key refresh may retain current data and report `refreshing`; initial/key-change loading reports `loading`.
9. `update` and controls have stable identities and functional updates see the latest committed data.
10. Strict Mode may replay setup, but it must not produce stale commits, unhandled rejections, or state updates after cleanup. Exact transport request count under Strict Mode is less important than correctness; `refetch()` count is exact.

Keep provider-specific SSE journaling out of this hook.

Add tests before the first caller migration for:

- null key;
- initial load;
- key change hiding old data;
- same-key refresh retaining data;
- exactly one request from `refetch`;
- out-of-order success and error;
- functional update from `undefined`;
- transition to a null key;
- abort/unmount;
- Strict Mode replay.

### 4.2 Add abort and error plumbing to read APIs

`apiFetch` already accepts `RequestInit`; add optional `AbortSignal` parameters to list/get helpers and pass `signal` through. Do not alter request paths, credentials, CSRF behavior, response DTOs, or mutation calls.

Normalize read-helper error behavior so resources can distinguish anonymous/empty data from transport failure:

- `getMe` returns `null` only for HTTP 401; abort, network errors, malformed responses, and other non-2xx statuses reject.
- `listChannels`, `listMessages`, and every other list/get helper used by the resource hook reject non-2xx responses before parsing DTOs.
- Preserve intentional best-effort behavior only at the provider/UI boundary where it is explicit and tested; do not silently turn a failed read into an empty canonical snapshot.

### 4.3 Migrate Auth first

Recommended public contract:

```ts
type AuthStatus = "loading" | "authenticated" | "anonymous";

interface AuthContextValue {
  user: User | null;
  status: AuthStatus;
  error: unknown | null;
  login(server: string, username: string, password: string): Promise<string | null>;
  register(
    server: string,
    username: string,
    password: string,
    email?: string,
  ): Promise<string | null>;
  logout(): Promise<void>;
  refresh(): Promise<void>;
}
```

Requirements:

- Preserve the unresolved/authenticated/anonymous distinction; `App` must not flash authenticated UI or login while initial lookup is unresolved.
- Login/register sets the server URL before the request and awaits the session refresh on success.
- An auth completion tied to an older server URL or older attempt cannot replace newer state.
- A same-key background `refresh()` retains the currently authenticated user/status while reporting refresh/error state; it must not transiently unmount Events, ReadStates, or Voice providers.
- Logout clears visible authenticated state even if a follow-up request fails.
- Preserve existing user-facing login/register error strings.
- Memoize the context value; actions use `useCallback` or are otherwise stable.
- Update `App`, login, channels, custom emojis, read states, channel/threads pages, and all auth mocks atomically from `auth.user()` to `auth.user`/`auth.status`.

Prefer declarative `<Navigate>` or route-bound rendering for auth redirects where it preserves current routes; if an effect remains, give it exact dependencies.

### 4.4 Confirm EventsProvider as the stable realtime boundary

`EventsProvider` is already mostly native React. Keep its public typed subscription methods. While updating dependent contexts, add coverage for:

- one EventSource per mounted provider;
- idempotent unsubscribe;
- close and listener cleanup on unmount/Strict Mode replay;
- malformed payload isolation;
- a throwing listener not preventing later listeners;
- reconnect callback behavior.

The server and browser may both provide connection evidence (`onopen` and a `"connected"` sentinel). Emit one logical recovery notification per connection; do not accidentally double-refetch every context.

Treat that logical notification as a **freshness barrier** in every snapshot-owning consumer: a snapshot started before the new connection cannot prove that it includes events missed during the gap. Abort/invalidate it and start a post-connection snapshot, or queue exactly one trailing refresh that must run after it settles. Never suppress the reconnect merely because an older request is already in flight. Add tests where a pre-reconnect request remains pending across reconnection.

### 4.5 Migrate ChannelsProvider

Recommended contract:

```ts
interface ChannelsContextValue {
  channels: readonly Channel[];
  status: ResourceStatus;
  error: unknown | null;
  reordering: boolean;
  refresh(): Promise<void>;
  reorder(ids: readonly number[]): Promise<void>;
}
```

Requirements:

- Fetch only for an authenticated user key; clear on logout/account change.
- Sort by position then ID.
- Handle `channel_created` and `channels_reordered` idempotently.
- Journal/replay channel events that arrive during an initial/background snapshot, or reconcile by provider reducer generation, so a delayed snapshot cannot erase a create/reorder event.
- Refresh on SSE reconnect because the stream has no replay cursor.
- Preserve optimistic reorder and rollback behavior.
- Validate reorder IDs are a permutation of the current IDs before sending.
- Serialize overlapping local reorder calls or generation-guard their responses so an older response cannot overwrite the newest user intent.
- Memoize context values/actions.

Update ChannelSidebar, AppShell, ChannelView, MessageInput, MessageText, and tests/mocks atomically.

### 4.6 Migrate CustomEmojisProvider

Recommended contract:

```ts
interface CustomEmojisContextValue {
  allEmojis: readonly CustomEmoji[];
  activeEmojis: readonly CustomEmoji[];
  byId(id: number): CustomEmoji | null;
  status: ResourceStatus;
  error: Error | null;
  refresh(): Promise<void>;
  create(...): Promise<CustomEmoji>;
  rename(...): Promise<CustomEmoji>;
  remove(...): Promise<CustomEmoji>;
  restore(...): Promise<CustomEmoji>;
}
```

Requirements:

- Derive active list and ID map with `useMemo` from the direct array.
- Upsert HTTP and SSE results by emoji ID; duplicate delivery is idempotent.
- A snapshot resolving after an SSE update must not erase that update. Use provider-level request generation plus an event journal/reducer, or refetch after connection/bootstrap; make the chosen policy explicit and tested.
- Clear state on auth key change.
- Keep action errors behavior-compatible with settings UI.
- Update settings, message input/text/reference, channel messages, thread panel, and every custom emoji mock atomically.

Channels and custom emoji migrations should be serialized because both touch `message-input.tsx` and `message-text.tsx`, unless one preparatory commit adds additive value-shaped fields before parallel consumer changes.

### 4.7 Migrate ReadStatesProvider

Use plain `auth.user` and retain the existing pure transition module. Recommended contract adds explicit `status`, `error`, and `refresh` while preserving `readState`, `hasUnread`, and `mentionCount`. Change `markRead(channelId, messageId)` to resolve with the accepted `ReadStateSummary` or `null` on a logged request failure, so ChannelView can distinguish success from failure and retry.

Requirements:

- Clear state when auth becomes anonymous or changes user.
- Latest snapshot wins; an older snapshot cannot replace newer auth state.
- Incoming messages/SSE summaries delivered during a snapshot must survive completion. Extend `read-state-transitions.ts` with version-aware merge helpers if needed.
- Preserve reconnect, focus, and visibility recovery.
- Focus/visibility triggers may coalesce, but a new logical SSE connection is a freshness barrier: abort/restart any older snapshot or queue one mandatory trailing refresh after it settles.
- A stale mark-read response must not regress a newer read-state summary.
- A failed mark-read returns `null`; ChannelView must not cache the failed key, and a subsequent visibility/scroll trigger can retry it.
- Keep selectors/actions stable and memoize the context value.

### 4.8 Remove core callable resource contracts

Once all core consumers are migrated:

- Remove `CallableResource` from Auth, Channels, and CustomEmojis types.
- Remove test mocks shaped as callable functions with attached `.loading`/`.error` fields.
- Leave `useCallableResource` in `react-state.tsx` only for ChannelView, ThreadPanel, and ThreadsPage until Phase 5.

## Test additions

- New `hooks/use-resource.test.tsx` matrix above.
- Auth provider initial/anonymous/authenticated, latest-attempt, logout, and server-switch tests.
- Channels provider delayed-snapshot plus create/reorder event races, optimistic reorder overlap/rollback, auth-change tests, and a pre-reconnect snapshot that remains pending across a new connection.
- Custom emoji snapshot/SSE race and duplicate HTTP/SSE upsert tests.
- Read-state snapshot/event/mark-read race tests, including a pre-reconnect snapshot followed by mandatory post-connection recovery.
- Events provider Strict Mode and reconnect deduplication tests.
- Existing App/login/channel sidebar/settings/message input/message text integration tests.

## Acceptance criteria

- Auth, Channels, CustomEmojis, and ReadStates contexts expose direct values and stable actions.
- Core consumers do not call context data as functions.
- Resource refetch issues one request and stale requests cannot cross keys/unmounts.
- Realtime updates cannot be silently replaced by a late provider snapshot.
- Context values are memoized; effects do not loop on recreated actions/objects.
- `useCallableResource` remains only in Phase 5 page/thread consumers.
- Full renderer tests and login/channel renderer E2E pass.

## Suggested commit order

1. `refactor(client): harden native resource hook`
2. `refactor(client): expose native auth state`
3. `test(client): harden EventSource lifecycle`
4. `refactor(client): expose native channel state`
5. `refactor(client): expose native custom emoji state`
6. `refactor(client): migrate read-state context to auth snapshots`

---

# Phase 5 — Make reducers own messages and threads

## Objective

Remove the duplicated callable-resource/store model from channel and thread views. Make pure reducers the only owners of snapshot plus SSE message state, with explicit channel/root generations and race handling.

## Prerequisites

Phases 1–4.

## Primary files

New pure state modules:

- `client/src/messages/channel-message-reducer.ts`
- `client/src/messages/channel-message-reducer.test.ts`
- `client/src/messages/thread-reducer.ts`
- `client/src/messages/thread-reducer.test.ts`

Existing integration/UI:

- `client/src/pages/channel.tsx`
- `client/src/components/channel-messages.tsx`
- `client/src/components/thread-panel.tsx`
- `client/src/pages/threads.tsx`
- `client/src/pages/channel.integration.test.tsx`
- `client/src/pages/threads.integration.test.tsx`
- `client/src/components/channel-messages.test.tsx`

## Tasks

### 5.1 Add a channel timeline reducer

Recommended state:

```ts
interface ChannelMessageState {
  channelId: number;
  generation: number;
  status: ResourceStatus;
  error: unknown | null;
  messages: readonly Message[];
  liveActionsDuringLoad: readonly ChannelLiveAction[];
}
```

Actions must cover:

- channel/load started;
- load succeeded/failed;
- message created/updated/hard-deleted;
- embeds updated;
- reactions updated;
- thread summary created/deleted;
- current-user profile updated.

Reducer rules:

1. Wrong-channel and stale-generation actions are identity no-ops.
2. Load start for a new channel clears visible old-channel rows immediately.
3. Live actions apply immediately while loading and are journaled.
4. Load success applies the journal over the fetched snapshot before commit.
5. Upsert/dedupe by message ID handles SSE-before-HTTP and HTTP-before-SSE.
6. Order by the server chronology key `(created_at ?? id, id)`.
7. Hard delete removes a row and marks inline references unavailable; tombstone update retains the row.
8. Embed/reaction/thread-summary actions patch only their scoped fields.
9. Unaffected message objects retain identity.
10. Deleting the final message leaves a truly empty list; there is no fallback to an old resource array.

### 5.2 Refactor ChannelView around one timeline

Remove:

- `useCallableResource`;
- `useStoreState`;
- `preserveIdentity`;
- `displayedMessages()` fallback between store and resource.

Loading effect:

- Parse and validate the route channel ID.
- Dispatch load start with a new generation.
- Fetch using an abort signal.
- Dispatch success/failure with captured channel/generation.
- Never render or commit an old-channel completion.

SSE effect:

- Subscribe once to the stable Events context.
- Dispatch typed reducer actions carrying event channel IDs.
- Subscribe to `events.onConnected`; start a same-channel, data-retaining history refresh and reconcile it through the same generation/journal rules.
- Treat each logical reconnect as a freshness barrier. Abort/invalidate a history request begun before it and start a newer request, or queue one mandatory trailing refresh; do not drop reconnect recovery because a request is already in flight.
- Keep current user ID in a ref only where reaction viewer merging requires the latest identity.
- Return all unsubscriptions from the same effect.

Composer behavior:

- Preserve main text/photos when changing channels, matching the existing test.
- Clear inline reply target, typing throttle, new-message indicator, and pending mark-read work on channel change.
- Prefer storing `replyTargetId` and deriving the current target from reducer state so update/delete automatically affects selection.
- Capture channel ID, text, photos, and reply target when submitting.
- A completion from an old channel cannot clear the new channel's draft/photos.
- A successful HTTP response may be reducer-upserted as a fallback and must dedupe with SSE.
- Do not erase text entered after a send started.

Scroll/read behavior:

- Decide whether to auto-follow before dispatching a genuinely new message.
- Perform pending scrolling in `useLayoutEffect` after rows commit.
- Keep `hasNewMessagesBelow` as explicit state rather than comparing resource/list lengths.
- Mark-read timers capture channel generation and message ID, then revalidate both before sending.
- Cancel timers on switch/unmount; cache the mark-read key only when `markRead` resolves with a successful/current summary. A `null`/failed result remains retryable.

Thread URL behavior:

- Normalize malformed `?thread=` values with replace navigation.
- Validate a thread root only after current-channel history is ready.
- Do not redirect merely because history failed, but also do not mount/fetch an unvalidated ThreadPanel while history is in an error state.
- A valid root is top-level and belongs to the current channel.
- Independently validate every successful `getThread(rootId)` payload (`root.id === rootId`, `root.channel_id === currentChannelId`, and `root.parent_id == null`) before publishing it. This closes the cross-channel hole because the endpoint itself is keyed only by root ID.

### 5.3 Keep ChannelMessages as UI-local state only

`ChannelMessages` may own edit drafts, menus, pending delete, and reaction-picker UI. It must not own the canonical timeline.

- Consume the direct message array.
- Use stable message IDs as row keys.
- Optionally extract a `React.memo` row after measuring/confirming props can be stable; memoization is not required merely to call the conversion complete.
- Preserve optimistic reaction guards; add an operation token/generation so an older HTTP completion cannot overwrite a newer SSE/second click.
- Keep inline reply, thread, edit, delete, embed suppression, mentions, attachments, and accessibility behavior unchanged.

### 5.4 Add a thread reducer

Recommended state:

```ts
interface ThreadState {
  channelId: number;
  rootMessageId: number;
  generation: number;
  status: ResourceStatus;
  error: unknown | null;
  root: Message | null;
  replies: readonly Message[];
  hasMoreReplies: boolean;
  liveActionsDuringLoad: readonly ThreadLiveAction[];
  deletedReplyIds: ReadonlySet<number>;
  olderStatus: "idle" | "loading" | "error";
  olderError: unknown | null;
}
```

Support:

- initial root/reply snapshot;
- reply created/deleted;
- root/reply update/tombstone;
- embed and reaction updates;
- inline-reference invalidation;
- updates to messages referenced by the root or any reply, even when that referenced target is not itself the root/reply;
- older-page start/success/failure.

Rules mirror the channel reducer:

- journal live actions during initial load;
- dedupe HTTP/SSE by ID;
- ignore stale root/generation responses;
- prevent an older page from resurrecting a reply deleted while it was pending;
- prepend sorted older replies and retain current replies;
- preserve unaffected object identity.

### 5.5 Refactor ThreadPanel

- Remove `useCallableResource` and use the thread reducer.
- Key the panel in ChannelView by `${channelId}:${rootMessageId}` so all thread-local draft/photo/edit/menu/pending state resets on root switch.
- Capture root/generation for initial load, pagination, send, edit, delete, and reaction operations.
- Old-thread completions cannot mutate the newly opened thread.
- On failed reply send, restore submitted text only if the same thread is active and the user has not entered replacement text.
- Preserve pagination scroll position with a post-commit `useLayoutEffect`, not an unguarded microtask.
- A hard-deleted root closes the panel; a tombstoned root remains renderable.
- Refetch/reconcile on SSE reconnect. A reconnect invalidates or schedules a mandatory trailing refresh after any initial/page snapshot that began before the connection.

The deliberate product decision is that switching thread roots discards thread-local draft/photos. This is safer than retaining a draft that can be posted to the wrong root. Main channel composer state remains preserved across channel switches.

### 5.6 Migrate Participated Threads

Use the Phase 4 resource API in `pages/threads.tsx`:

- direct `user?.id`;
- no callable resource;
- retain current data only for a same-key background refresh;
- clear on auth key change;
- ignore stale completion;
- refresh after reconnect and thread lifecycle events when preview membership/recent replies may have changed; ordinary events may coalesce, but reconnect must invalidate an older in-flight snapshot or queue one mandatory trailing refresh.

### 5.7 Remove message-specific compatibility exports

After all Phase 5 consumers migrate, remove external use of:

- `CallableResource` / `useCallableResource`;
- `useStoreState`;
- `preserveIdentity`.

The definitions may be deleted immediately if no other call sites remain, or left for the final logic-free file deletion.

## Pure reducer test matrix

Channel reducer:

- live create before snapshot completion survives exactly once;
- HTTP response plus SSE create dedupes;
- hard delete before snapshot completion prevents resurrection;
- update, embeds, reactions, and thread summaries patch one row;
- reply references update and become unavailable correctly;
- wrong channel/stale generation is a no-op;
- deleting the last row produces an empty timeline;
- unaffected row identity is preserved.

Thread reducer:

- live reply before initial load survives exactly once;
- deleted reply cannot return through initial/older page;
- root and reply updates are scoped correctly;
- updates to external inline-reference targets patch every referencing root/reply;
- same-channel and cross-channel/mismatched-root `getThread` payload validation;
- reaction/embed events target root or reply correctly;
- old root/generation responses are ignored;
- pagination dedupes and preserves order.

## Integration tests to add

In `pages/channel.integration.test.tsx`:

- delayed channel A fetch resolves after navigation to B;
- SSE create/delete arrives while initial history is pending;
- pending mark-read timer followed by channel navigation;
- send completion after navigation;
- user types new text while prior send is pending;
- rapid thread A -> B switch with delayed initial, older-page, and send responses;
- malformed thread query;
- history load error does not wrongly close a valid-looking thread URL;
- reconnect snapshot reconciliation.

Retain all existing coverage for drafts/photos, inline replies, mentions, reactions, tombstones, pagination, multiline editing, scrolling, and accessibility.

## Acceptance criteria

- Channel and thread reducers are the sole rendered timeline sources.
- No previous-channel/thread rows appear during navigation.
- HTTP/SSE duplicates render once.
- Delivered live events cannot be overwritten by a later snapshot.
- Deletes cannot be resurrected by snapshots or pagination.
- Main composer preservation and thread-local reset behavior match the decisions above.
- No production references remain to callable resources, store updater, or `preserveIdentity`.
- Renderer E2E for login/channel/send/reactions/mentions/photos passes.

## Suggested commit order

1. `refactor(client): add pure channel timeline reducer`
2. `refactor(client): make ChannelView reducer-owned`
3. `refactor(client): add pure thread reducer`
4. `refactor(client): make ThreadPanel reducer-owned`
5. `refactor(client): migrate participated threads resource`

---

# Phase 6 — Isolate voice/media behind native React boundaries

## Objective

Replace the getter-shaped voice context and render-coupled LiveKit lifecycle with an imperative `VoiceSession`, a native preferences context, pure channel-presence state, and Strict Mode-safe media ownership.

## Prerequisites

Phases 1–4. May run alongside Phase 5 with shared-file ownership coordinated.

## Architecture choice

Use an imperative external store observed with `useSyncExternalStore`.

Rejected alternatives:

- A mechanical `useState` conversion would introduce stale closures in long-lived LiveKit callbacks.
- A giant provider-level reducer plus refs would leave room/publication lifecycle coupled to React and keep the current ~1,000-line provider difficult to test.
- LiveKit React components add dependency/bundle cost and conflict with Hamlet's `autoSubscribe: false`, source-aware subscription, custom audio routing, and Electron screen-capture path.

Target boundary:

```text
VoiceSession (Room/publications/queues/epochs)
  -> immutable snapshot + subscribe/getSnapshot
  -> useSyncExternalStore
  -> VoiceChatContext with plain values and stable actions
  -> presentation components
```

## New files

- `client/src/voice/voice-state.ts`
- `client/src/voice/voice-state.test.ts`
- `client/src/voice/voice-session.ts`
- `client/src/voice/voice-session.test.ts`
- `client/src/contexts/voice-preferences.tsx`
- `client/src/contexts/voice-preferences.test.tsx`
- `client/src/voice/channel-presence.ts`
- `client/src/voice/channel-presence.test.ts`
- `client/src/components/attached-video-track.tsx`
- `client/src/components/attached-video-track.test.tsx`

## Existing files

- `client/src/contexts/voice-chat.tsx`
- `client/src/contexts/voice-chat.test.tsx`
- `client/src/components/voice-channel.tsx`
- `client/src/components/voice-channel.test.tsx`
- `client/src/components/voice-settings.tsx`
- `client/src/components/voice-settings.test.tsx`
- `client/src/components/voice-status-controls.tsx`
- `client/src/components/local-camera-tile.tsx`
- `client/src/components/remote-camera-tiles.tsx`
- `client/src/components/screen-share-viewer.tsx`
- corresponding media tests
- `client/src/voice/settings.ts`
- `client/src/voice/livekit.ts`
- `client/src/voice/audio-routing.ts`
- `client/src/App.tsx`

## Tasks

### 6.1 Define the immutable voice snapshot

Use explicit statuses internally:

```ts
type ConnectionStatus = "idle" | "connecting" | "connected";
type MediaStatus = "off" | "starting" | "on" | "stopping";
```

Snapshot fields:

- active channel ID;
- connection status;
- muted/deafened;
- screen-share status/publication-visible state;
- camera status/local track;
- remote camera tiles;
- watched screen-share metadata and subscribed track;
- speaking user IDs;
- user-visible error.

The snapshot object must retain identity until a field actually changes. Expose immutable maps/sets/arrays or replace them on change; never mutate a published snapshot in place.

Context may expose derived booleans (`isConnecting`, `isCameraBusy`, etc.) for convenient UI use, but they are values, not getters.

### 6.2 Build `VoiceSession`

Constructor rules:

- No network request, media capture, room creation, DOM append, or publication occurs during React render/session construction.
- Dependencies are injected: token/status/speaking APIs, Room factory, audio router, input-gain adapter, and preference snapshot access.

Stable interface:

```ts
interface VoiceSession {
  subscribe(listener: () => void): () => void;
  getSnapshot(): VoiceSnapshot;
  activate(): void;
  deactivate(): Promise<void>;
  join(channelId: number): Promise<void>;
  leave(): Promise<void>;
  toggleMuted(): Promise<void>;
  toggleDeafened(): Promise<void>;
  startScreenShare(): Promise<void>;
  stopScreenShare(): Promise<void>;
  startCamera(): Promise<void>;
  stopCamera(): Promise<void>;
  syncRemoteCameraStreams(channelId: number, streams: readonly CameraStream[]): void;
  applyPreferences(preferences: VoicePreferencesSnapshot): void;
  watchScreenShare(stream: ScreenShareStream): Promise<void>;
  stopWatchingScreenShare(): Promise<void>;
}
```

Session-owned mutable fields:

- current Room and room-listener disposers;
- local camera/share publications and track-ended cleanup;
- remote stream/publication/track maps;
- audio router and input-gain disposable;
- desired mute/deafen state and prior mute before deafen;
- serialized control queue;
- room/media operation generation (epoch);
- participant speaking listeners/state;
- last posted local speaking state.

Every asynchronous operation captures the epoch. A stale completion must either do nothing or explicitly dispose the resource it created. A joining room must become session-owned early enough that failure/deactivation can disconnect it; it must not remain only in a local variable through multiple awaited setup steps.

`activate`/`deactivate` must be idempotent and support Strict Mode's setup-cleanup-setup probe. `deactivate`, explicit leave, and `RoomEvent.Disconnected` may race and must converge without duplicate errors or leaked tracks.

### 6.3 Make low-level media helpers disposable

`voice/livekit.ts` currently creates a stream and `AudioContext` for input gain without returning ownership. Change it to return a disposable handle containing any source stream, processed track, publication, and context it owns. Dispose on leave, switch, disconnect, replacement, and error.

Pass device/noise/gain preferences as arguments rather than reading localStorage inside low-level media helpers.

Harden `voice/audio-routing.ts`:

- retain track-to-element ownership explicitly;
- avoid duplicate attachment for the same track SID;
- detach LiveKit tracks as well as remove audio elements;
- make `detach`/`detachAll` idempotent;
- expose `setOutputDevice(id)` (or equivalent) and reroute existing elements when output preference changes;
- never use a random key for an owned active track when a stable object/SID key is available.

### 6.4 Add VoicePreferencesProvider

Move mutable preference state out of `voice/settings.ts`. The provider owns direct values/actions for:

- input device;
- output device;
- camera device;
- noise suppression;
- input gain;
- show-speaking-indicators-everywhere.

`voice/settings.ts` retains constants and pure parse/load/save helpers only. Remove module-global `showSpeakingEverywhere` and `setShowSpeakingEverywhereSignal`.

Mount preferences outside `VoiceChatProvider` so VoiceSettings, VoiceChannel, and VoiceSession read one current source. Persist changes in dependency-scoped effects or action functions. Same-tab preference changes must rerender consumers without `useStaticSignalRerender`.

Bridge preference updates explicitly into the active session with `applyPreferences` (or a session subscription). Output-device changes reroute already attached audio without reconstructing/disconnecting the room. Input/camera device, noise-suppression, and gain changes are captured by the next relevant capture/publication operation unless current behavior already applies them live; document and test that boundary.

### 6.5 Replace VoiceChatProvider

- Instantiate one render-pure session per provider lifetime.
- Subscribe with `useSyncExternalStore`.
- Activate/deactivate in one effect.
- Memoize a context value containing snapshot values and stable session commands.
- Move most current LiveKit behavior tests from `contexts/voice-chat.test.tsx` to deterministic `voice-session.test.ts` tests.
- Keep provider tests for snapshot projection, action forwarding, missing-provider errors, and Strict Mode lifecycle.

A two-commit transition is acceptable:

1. Move internals to `VoiceSession` while temporarily retaining getter-shaped context fields.
2. Convert the context to values and update every consumer/mock atomically.

Do not leave a mixed getter/value context in a landed commit without an additive compatibility period and explicit cleanup commit.

### 6.6 Convert VoiceChannel presence to a pure reducer

The current component mutates local variables inside state updaters to decide announcements. React updater callbacks must be pure.

`channel-presence.ts` should own:

- participants;
- camera/screen-share metadata;
- remote speaking IDs;
- a bootstrap generation and ordered live-event journal;
- stopped-stream tombstones used to reject stale bootstrap results;
- announcement payloads.

Effects own API bootstrap and SSE subscriptions. Reducer transitions decide whether an event is new/removed and return any announcement state. Requirements:

- Ignore bootstrap completion after cleanup/channel change.
- Journal participant join/leave/status/speaking and camera/screen-share start/stop events during bootstrap, then replay them over delayed participant/media snapshots.
- Merge bootstrap with SSE without resurrecting stopped participants or tracks.
- Refresh presence snapshots on `events.onConnected` because SSE has no replay cursor. A reconnect invalidates any bootstrap/snapshot begun before the connection or queues one mandatory trailing refresh; it is never dropped merely because work is already in flight.
- Use stable participant/media keys.
- Sync active-channel camera metadata to VoiceSession in a dependency-complete effect.
- Clear presence/listeners for the correct channel on unmount.

Keep per-channel presence ownership for parity; a global presence provider is not required.

### 6.7 Rewrite VoiceSettings resource ownership

Renderable state:

- device lists;
- loading/errors;
- mic/output/camera test statuses;
- mic level;
- current preview stream if needed for rendering.

Imperative refs:

- microphone test stream;
- microphone `AudioContext`;
- animation frame ID;
- camera preview stream authoritative handle;
- output-test audio/context resources;
- request/lifecycle generations;
- video element.

Requirements:

- Stop/unmount after any rerender still reaches and disposes the active mic resources.
- Opening settings never requests camera permission.
- Camera preview starts only from explicit action.
- A preview resolving after stop/unmount has every track stopped.
- Device selection uses controlled React `<select>` values; remove manual DOM `.value` reapplication.
- No cleanup-only permanent `isDisposed` flag remains true after Strict Mode replay.
- Prime microphone labels at most once per mounted logical settings session where practical; duplicate Strict Mode setup must still cleanly stop every obtained stream.

### 6.8 Consolidate video-track attachment

Create `AttachedVideoTrack` used by local camera, remote cameras, and screen share:

- own a `<video>` ref;
- in `useEffect([track])`, attach the exact track to the exact element;
- returned cleanup detaches that same pair;
- replacement detaches old before attaching new;
- Strict Mode may attach-detach-attach, but ownership remains balanced;
- preserve `muted`, `playsInline`, `autoPlay`, object-fit classes, and labels as props.

Use explicit camera/screen keys so React never reuses a video component for another stream.

## VoiceSession test matrix

- Join, leave, channel switch, server disconnect.
- Token failure, connect failure, microphone failure, input-gain failure.
- Join A followed by join B; A never becomes current afterward.
- Leave/deactivate while token, connect, camera, share, or gain work is pending.
- Joining room gets disconnected on every stale/error path.
- Pre-join mute/deafen and rapid serialized toggles.
- Deafen forces mute; undeafen restores prior mute; unmuting while deafened also undeafens according to current behavior.
- Speaking transitions post once and use the captured channel.
- `autoSubscribe: false` plus microphone automatic subscription.
- Camera discovery before/after publication converges.
- Exactly the selected screen share is enabled/subscribed.
- Camera and screen share coexist and stop independently.
- Track-ended events clean state/publications.
- Audio/gain resources dispose on leave/switch/disconnect/failure.
- Strict Mode provider activate/deactivate leaves no room/listener leak.

## Component tests

- Presence bootstrap/SSE ordering and announcements, including a bootstrap request pending across reconnect and a guaranteed post-connection refresh.
- Voice status controls consume direct values.
- VoiceSettings mic stop/unmount closes context, cancels RAF, and stops tracks after rerender.
- Camera preview stale completion cleanup.
- Shared video attachment replacement/unmount.
- Camera tile reorder identity.
- Screen-share watcher switching and stop.
- Existing axe/accessibility coverage.

## E2E/manual validation

Run:

```bash
pnpm run test:e2e:voice:browser
pnpm run test:e2e:electron
```

Extend practical E2E where deterministic fake media allows:

- join -> camera on -> disconnect -> track ends;
- rejoin -> camera on again without duplicate tile/stale error;
- settings preview -> close settings -> track ends -> reopen and preview again;
- existing two-client screen-share discover/watch/stop flow.

Complete the existing webcam and screen-sharing manual QA checklists when certifying the phase.

## Acceptance criteria

- No voice production/test file imports `react-state`.
- Voice context exposes plain values and stable actions.
- UI components do not own or import Room lifecycle/event/publication APIs.
- LiveKit callbacks cannot observe stale render closures.
- Every created room, publication track, media stream, audio element/context, animation frame, and listener has idempotent cleanup.
- Voice preferences are reactive without static signals.
- Voice unit/component/E2E coverage passes without warning suppression.
- No server or Electron security contract changes.

## Suggested commit order

1. `refactor(client): add native voice preferences`
2. `fix(client): make audio and input gain resources disposable`
3. `refactor(client): isolate LiveKit in VoiceSession`
4. `refactor(client): expose native voice context values`
5. `refactor(client): reducer-own voice channel presence`
6. `fix(client): make voice settings and video attachment StrictMode-safe`

---

# Phase 7 — Remove compatibility infrastructure and enforce native React

## Objective

Finish remaining native React cleanup, migrate the last tests, delete compatibility/dead files, update active documentation, and make regression checks blocking.

## Prerequisites

Phases 1–6.

## Tasks

### 7.1 Modernize MessageInput's imperative boundary

Primary files:

- `client/src/components/message-input.tsx`
- `client/src/components/message-input.test.tsx`
- `client/src/pages/channel.tsx`
- `client/src/components/thread-panel.tsx`
- `client/src/components/channel-messages.tsx`
- tests that manipulate the editor in `channel-messages.test.tsx` and `pages/channel.integration.test.tsx`.

Remove production DOM monkey-patching that defines synthetic `value`, `selectionStart`, `selectionEnd`, and `setSelectionRange` properties on a `div`.

Define a narrow native contract:

- The value remains controlled through props.
- The DOM ref is `HTMLDivElement | null`.
- If parents need more than `.focus()`, expose an explicit `MessageInputHandle` with `focus()` and typed selection methods using `forwardRef`/`useImperativeHandle` (or React 19's ref prop pattern).
- Callback refs receive `null` on detachment.
- Tests set contenteditable text with user events or `textContent` plus an input event and set caret position with DOM `Range`/`Selection` helpers.

Refactor every-render correction effects:

- Key autocomplete selection state by token/session where possible.
- Derive clamped indices rather than effect-correcting them after every render.
- Give async mention search complete dependencies and a request generation/abort path.
- Preserve composition handling, autocomplete precedence, controlled value rendering, chip serialization, and focus/selection restoration.
- Remove cleanup-only disposed flags that remain true after Strict Mode replay.

### 7.2 Finish secondary React cleanup

- Intentionally fix text-channel mention navigation by using React Router `Link`/navigation instead of document-reloading anchors. This is a scoped bug fix, not behavior-neutral cleanup: add a router harness to MessageText tests and assert navigation changes the client route without a document reload or tearing down active providers.
- Keep mention click events typed as React events long enough to use the button `currentTarget`.
- Ensure all context provider values are memoized and all public actions stable.
- Remove remaining zero-argument getter wrappers used only to emulate Solid accessors.
- Replace ordinary controlled form `onInput` handlers with `onChange`; retain contenteditable `onInput`.
- Resolve every remaining React hooks/dependency/key lint override rather than broadening it.

### 7.3 Normalize React Testing Library usage

Migrate every remaining test away from static signals:

- `render(() => <Component />)` -> `render(<Component />)` or a named harness component.
- Hooks execute only inside components/custom hooks.
- Module-level voice/control mocks become explicit stores, harness-owned state, or `rerender` props.
- Externally invoked updates use `act`.
- Router helpers pass elements/components without eagerly invoking a callback during parent render.
- Prefer direct `@testing-library/react` imports or keep one thin project helper that only provides Strict Mode/provider/router composition.

Then:

- remove `useStaticSignalRerender` from production and tests;
- delete the global static listener/`flushSync` bridge;
- delete invalid-hook warning suppression;
- remove the global `act()` warning filter from `test/setup.ts`;
- run the entire suite with React diagnostics visible.

### 7.4 Delete compatibility and dead files

After a zero-reference search, delete:

- `client/src/hooks/react-state.tsx`
- `client/src/test/testing-library.tsx` if the helper has no non-compatibility value; otherwise replace it with a normal thin re-export in a separate commit before deletion.
- `client/src/routes.ts` (unused duplicate route definition).
- `client/src/contexts/index.ts` if it remains unused.
- `client/oxlint.config.ts` (unused by the package script).

Keep the Phase 4 `client/src/hooks/use-resource.ts`; it is now the tested native resource hook.

### 7.5 Make React/type rules blocking

Final `client/oxlint.config.json` must have no migration quarantines and enforce at least:

- `react/rules-of-hooks`
- `react/exhaustive-deps`
- `react/jsx-key`
- `react/no-array-index-key` where the rule can be enabled without false-positive positional UI IDs
- `react/jsx-no-constructed-context-values`
- `typescript/no-explicit-any`
- `typescript/no-non-null-assertion`

Final `client/tsconfig.json` keeps `strict: true` and `noImplicitAny: true` without contradictory weakening.

Do not add blanket `eslint-disable`/Oxlint disable comments. Any narrow exception must explain the invariant the analyzer cannot see.

### 7.6 Add a permanent native-React audit

Add `client/scripts/check-native-react.mjs` and a `pnpm run check:native-react` command. Wire it into `scripts/check.sh` client checks and relevant client CI/deployment jobs.

The script should fail on:

- existence/import of `src/hooks/react-state`;
- compatibility identifiers such as `useSignalState`, `useCallableResource`, `registerCleanup`, `useStaticSignalRerender`, `CallableResource`, `<If>`, `<List>`, `<Choose>`, or `<Case>` in active source/tests;
- Solid runtime/tool dependencies or plugins in `package.json`/lockfile;
- active documentation describing the current client as Solid.

Allow historical use of the word "Solid" in the rewrite PRD when clearly marked as historical; do not use a repository-wide naive word ban.

### 7.7 Update active documentation

Update:

- `AGENTS.md` / symlinked `CLAUDE.md`
- `server/CLAUDE.md`
- `scripts/check.sh` usage text
- `docs/hosting-static-client-and-server.md`
- `client/README.md`
- `docs/solid-to-react-client-rewrite-prd.md`

Changes:

- Describe the active renderer as React/Vite.
- Document direct-value context conventions, reducer ownership, and the `VoiceSession`/VoicePreferences boundary in `client/README.md`.
- Mark the rewrite PRD completed or make its old-state language explicitly historical.
- Keep command names and runtime architecture accurate.

## Final zero-reference audit

Run the repository searches from the repository root; they must return no active compatibility results:

```bash
rg -n "hooks/react-state|useSignalState|useComputedValue|useCallableResource|useAfterRenderEffect|useMountEffect|registerCleanup|useStaticSignalRerender|useStoreState|preserveIdentity|CallableResource" client/src
rg -n "<If|<List|<Choose|<Case" client/src --glob '*.tsx'
rg -n "from ['\"]solid|solid-js|vite-plugin-solid|@solidjs" client client/pnpm-lock.yaml
(cd client && pnpm run check:native-react)
```

## Final validation gates

From `client/`:

```bash
pnpm run fmt
pnpm run lint
pnpm run check:native-react
pnpm run typecheck
pnpm run test
pnpm run storybook:build
pnpm run build:web
pnpm run size
pnpm run test:e2e:renderer
pnpm run test:e2e:electron
pnpm run test:e2e:voice:browser
```

From repository root:

```bash
scripts/check.sh client --e2e
```

Run `pnpm run package:smoke` when the final changes affect packaged renderer behavior or Electron media flows.

## Acceptance criteria

- `react-state.tsx` and static test signal machinery are deleted.
- No callable signal/resource/context contracts remain.
- No hooks run outside components/custom hooks.
- Tests use normal React render/harness semantics.
- No React or `act()` warning is globally suppressed.
- Strict Mode, hook dependencies, stable keys, and explicit event types are enforced.
- Active documentation consistently describes React.
- Full unit, integration, renderer E2E, Electron E2E, voice E2E, Storybook build, web build, and size budget pass.

## Suggested commits

- `refactor(client): remove contenteditable compatibility properties`
- `test(client): use native React Testing Library harnesses`
- `refactor(client): delete React compatibility layer`
- `chore(client): enforce native React audit and hooks lint`
- `docs: mark React renderer migration complete`

---

# Recommended execution waves

This is the dependency-safe assignment order for multiple agents.

## Wave A — Serial foundation

1. Phase 1 guardrails.
2. Phase 2 mechanical rendering/types.

Phase 2 may use parallel workers on disjoint component families after Phase 1 lands.

## Wave B — Parallel leaf ownership

- Worker 1: modal/cropper/object URL lifecycle.
- Worker 2: typing/emoji picker/message preview lifecycle.
- Worker 3: local state in forms/settings/sidebar/reactions/attachments.
- Integrator: `useComposerPhotoSelection` and shared ChannelView/ThreadPanel call sites.

Land all of Phase 3 before changing context contracts.

## Wave C — Serial context boundary

1. Native resource hook.
2. Auth.
3. Events lifecycle characterization.
4. Channels.
5. Custom emojis.
6. Read states.

Channels and custom emojis must not independently edit MessageInput/MessageText in parallel without an additive context API prepared first.

## Wave D — Parallel domains

After Phase 4:

- Message lane: Phase 5 reducers and integration.
- Voice lane: Phase 6 session/preferences/media.

Coordinate ownership for App, ChannelSidebar, SettingsModal, and test helpers. Pure reducer/session modules and their tests can be developed independently before integration commits.

## Wave E — Serial deletion/enforcement

Phase 7 should be one coordinated cleanup lane. Do not delete compatibility files until all domain branches have landed and the zero-reference searches are clean.

# Completion definition

The project is not complete merely because `react-state.tsx` has no production imports. Completion requires:

- tests no longer depending on static signal semantics;
- explicit latest-value handling for SSE/LiveKit/timers;
- deterministic snapshot/realtime reducer behavior;
- Strict Mode-safe resource cleanup;
- direct context values and stable actions;
- full runtime, E2E, build, size, lint, type, and documentation gates passing.
