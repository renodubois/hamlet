# Native client module boundaries and browser compromises

The native client is intentionally separate from the Tauri/Solid client. `client-iced/` owns the Rust/Iced alpha, while `../client/` remains the existing WebView client until native parity is proven.

## Boundary model

Hamlet uses three layers of modules in the native client.

### `app`: orchestration layer

`src/app/` is the only layer that should know the whole application shape. It owns the top-level route, boot/session state, signed-in state, selected channel, cross-feature effects, subscriptions, and the Iced application builder.

Responsibilities:

- map runtime events into `AppMessage` values;
- route reducer output into `AppEffect` values;
- start/stop signed-in-only subscriptions such as SSE and the voice worker;
- coordinate cross-feature cleanup on logout or auth expiration;
- configure native window metadata such as title, icon, and minimum size.

Do not put HTTP request construction, SSE parsing, filesystem paths, LiveKit room state, or platform dialogs directly in view code. Those belong in deep modules or explicit effects.

### Feature layer

Feature-owned state and UI behavior should stay local to the feature whenever possible. The alpha still has some feature view composition in `src/app/view.rs`, but state and behavior are already split by domain:

- auth/session/profile/channel/message/voice UI state is modeled in `src/auth/mod.rs` and consumed by the app reducer;
- emoji picker state and keyboard navigation live in `src/emoji.rs`;
- embed rendering decisions live in `src/embeds.rs`;
- external-link validation/opening status lives in `src/external_open.rs`;
- voice command/event UI state is driven by `src/voice/mod.rs` and signed-in state.

When extracting future `feature/*` modules, expose a narrow surface: `State`, feature `Message`, optional `Action`, reducer/update helpers, and view helpers. Feature reducers should return state changes plus actions/effects instead of reaching into unrelated modules.

### Deep modules

Deep modules hide implementation details behind typed capabilities and deterministic tests:

- `src/protocol/` contains serde DTOs matching the server wire format and broadcast event taxonomy.
- `src/api/` owns HTTP transport, base URL normalization, cookies, JSON/multipart payloads, and error mapping.
- `src/realtime/` owns SSE framing/parsing, connection events, auth-expiration mapping, and reconnect timing decisions.
- `src/storage/` owns native config paths and typed persisted preferences.
- `src/avatar.rs` and `src/embeds.rs` own image URL resolution, fallback avatars, fetch/cache status, and native preview decisions.
- `src/external_open.rs` owns safe external URL validation and OS-specific opener commands.
- `src/voice/` owns the LiveKit/native audio worker contract. The app sends `VoiceCommand` values and receives `VoiceEvent` values; widgets never own LiveKit room state.
- `src/test_support/fixtures.rs` provides deterministic auth, channel, message, SSE, voice-presence, and voice-worker fixtures for reducer and deep-module tests.

## Browser-specific compromises

### Iframes and rich embeds

The native client does not render arbitrary provider iframes. Iced has no browser sandbox or DOM frame equivalent, and embedding third-party web content would create a separate security and focus model. Rich/video embeds degrade to native cards with title/site/description/image metadata when available plus an external-open action for the original URL.

### WebAudio and media devices

The Tauri/Solid client can rely on browser WebAudio/WebRTC device behavior. The native client uses the LiveKit Rust SDK through a long-lived voice worker. Device selection, microphone permission errors, mute/deafen, remote playback, and disconnect cleanup are surfaced as typed worker events and must be manually QAed on each desktop OS. Native device lists and output routing may not match browser labels or behavior exactly.

### Drag-and-drop

Browser drag-and-drop is not ported directly. The alpha uses explicit move up/down channel reorder controls so the interaction is deterministic, testable in reducers, and reachable by keyboard users. Native pointer-driven drag reorder can be added later, but it must preserve the same optimistic update/rollback contract.

### Accessibility tooling

The existing web client can use DOM-oriented tooling such as axe and Testing Library. Those tools do not inspect Iced widgets. Native accessibility acceptance therefore relies on reducer tests for state changes, keyboard/focus manual QA, and platform inspection until a stronger Iced-native accessibility test stack is selected. Do not claim DOM accessibility parity without native evidence.

### File dialogs and local storage

Browser file inputs and localStorage are replaced with native file dialogs (`rfd`) and typed config persisted through `directories`. Tests should target the effect boundaries and storage module; manual QA should cover actual OS file dialogs and cancellation paths.
