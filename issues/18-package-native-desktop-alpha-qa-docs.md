# Issue 18: Package native desktop alpha with native QA checklist and boundary docs

**Type**: AFK

**User stories covered**: 75, 77, 79, 80, 81, 82

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Prepare the native client as a distributable desktop alpha while keeping the existing Tauri/Solid client available. Packaging should include application identity assets and platform metadata, tests and fixtures should be easy to run, and maintainers should have documentation for native module boundaries plus manual QA coverage for behavior that browser/DOM tests no longer cover.

## Acceptance criteria

- [ ] Native desktop packaging includes app icons, application metadata, and platform-specific release settings needed by implemented features.
- [ ] The existing Tauri/Solid client remains available during the alpha and is not removed as part of packaging.
- [ ] A native-client check command or documented sequence runs formatting, linting, type/compile checks, and tests.
- [ ] Deterministic fixtures cover auth, channels, messages, SSE events, voice presence, and voice worker behavior.
- [ ] A manual QA checklist covers windowing, focus, modal/popover dismissal, channel reorder interactions, file dialogs, image previews, high-DPI/window resizing, app shutdown cleanup, and microphone behavior.
- [ ] Boundary documentation explains the app/feature/deep-module split and records browser-specific compromises around iframes, WebAudio, drag-and-drop, and accessibility tooling.

## Blocked by

- Issue 9
- Issue 11
- Issue 13
- Issue 14
- Issue 15
- Issue 17
