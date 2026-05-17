---
id: 32
name: Build the local unpacked Electron alpha package
status: in-progress
tags: [project-electron-client-port, client-electron, afk, packaging, release]
blocked_by: [31]
---

# Build the local unpacked Electron alpha package

**Type**: AFK

**User stories covered**: 1-3, 73, 75-80.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Add local unpacked packaging scaffolding for an Electron alpha that can be installed or launched side by side with the existing clients. The package should use distinct alpha identity, include practical platform metadata and icons, launch the fixed-loopback production renderer path, and deliberately defer signing, notarization, installers, auto-update, and public distribution.

## Acceptance criteria

- [ ] A local unpacked Electron package target builds from renderer, main, and preload outputs and launches the packaged app.
- [ ] The alpha product name, application identifier, and install metadata are distinct from the Tauri/Solid and native clients.
- [ ] Reused or adapted Hamlet icons and platform permission metadata, including macOS microphone/camera usage strings where needed for voice, are included.
- [ ] Package scripts for build, unpacked packaging, full packaging placeholders, formatting, linting, typechecking, tests, E2E, and size checks remain stable and documented.
- [ ] Signing, notarization, installers, auto-update, and public distribution are explicitly deferred rather than half-configured.

## Blocked by

- Issue 31
