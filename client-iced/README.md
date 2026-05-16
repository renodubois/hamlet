# Hamlet Iced Client

This is the native Rust/Iced client being built alongside the existing Tauri/Solid client in `../client/`.

The crate is intentionally self-contained for the native desktop alpha. It talks to the existing Hamlet server API over HTTP/SSE, uses native storage for preferences, and keeps deterministic fakes/fixtures for auth, channels, messages, realtime, voice presence, and the voice worker.

## Development

```bash
cd client-iced
cargo run
./scripts/check.sh
```

`./scripts/check.sh` runs the native-client gate in order: `cargo fmt -- --check`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. Use `./scripts/check.sh --fix` to run `cargo fmt` before the remaining checks.

Headless end-to-end smoke tests live in `tests/e2e.rs`. They use `iced_test::Simulator` to drive the rendered Iced UI against deterministic native-client fakes, covering the same golden-path categories as the web client's Playwright tests: auth, channel landing, message send, emoji send, and channel reorder persistence.

The default server URL is `http://localhost:3030`. The URL can be edited from the signed-out login shell and is persisted to the native config directory.

## Native alpha packaging

Packaging metadata lives in `Cargo.toml` under `[package.metadata.bundle]`. The alpha bundle identity is:

- app name: `Hamlet`
- bundle/application id: `com.renodubois.hamlet`
- icon assets: `packaging/icons/hamlet-*.png` and `packaging/icons/hamlet-icon.svg`
- macOS microphone and high-DPI Info.plist additions: `packaging/macos/microphone-permission.plist`
- Linux desktop/metainfo templates: `packaging/linux/`
- Windows DPI manifest reference: `packaging/windows/hamlet.exe.manifest`

The runtime Iced window also embeds the 256px Hamlet icon and sets a minimum desktop size for the alpha. Release builds on Windows use the GUI subsystem so they do not open a console window.

A cargo-bundle-compatible alpha build can be produced with:

```bash
cd client-iced
cargo install cargo-bundle # once per machine, if not already installed
cargo bundle --release
```

The existing Tauri/Solid client remains in `../client/` and can still be built/run independently during the native alpha.

## QA and architecture docs

- Manual native alpha checklist: [`docs/native-alpha-qa.md`](docs/native-alpha-qa.md)
- App/feature/deep-module boundaries and browser compromises: [`docs/module-boundaries.md`](docs/module-boundaries.md)

## Manual LiveKit voice validation

The native voice worker uses the server's `/voice/token/{channel_id}` flow and the LiveKit Rust SDK. To validate real media behavior:

1. Start the development server stack with LiveKit: `cd ../server && docker compose up --build`.
2. In another terminal, run the native client: `cd client-iced && cargo run`.
3. Log in with `baipas` / `password`, select the seeded `voice` channel, and click **Join voice**.
4. Verify the UI reaches `Connected to voice room channel-…`, then click **Leave voice** and confirm it returns to idle.
5. Create or use a second voice channel, join the first, then click **Switch voice** on the second and confirm stale connected state is not shown for the first channel.
6. Optional multi-client check: run a second client as `teo` / `password` and confirm participants update through the server/LiveKit webhook path.

If voice token fetch fails with a LiveKit configuration error, keep the client open, start the Compose stack above, and retry the join button.
