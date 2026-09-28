# GPUI client — Linux feasibility gate (#36)

This is still the **starter**, not the authenticated rewrite-server client. Its input and Submit button are functional; the headless tests also exercise the proposed conversation-history primitives. Do not use it to access a server yet.

## Linux setup

Verified on Linux x86_64, Arch kernel 7.2.7, Rust/Cargo 1.95.0, locked `gpui-kit` 0.6.1 (`gpui-pre` 0.3.4). Native build dependencies here include a C compiler, pkg-config, fontconfig 2.18.3, freetype 26.6.20, xkbcommon 1.13.2, Wayland client 1.26.0, X11 libraries and Vulkan loader/driver. This machine has Xvfb, ImageMagick `import`, and a working Vulkan implementation. Install equivalent development headers/libraries for your distribution; on a desktop, provide a functioning X11 or Wayland session and Vulkan driver. Other platforms have **not** been verified.

From `client-gpui/`:

```sh
cargo build --locked
cargo run --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked submitting_the_real_input_updates_the_greeting
cargo test --locked varied_height_history_preserves_reader_on_prepend
cargo test --locked selectable_history_copies_line_breaks
```

The headless tests use `gpui-kit`'s `test-support` feature in dev dependencies, run without DISPLAY/Xvfb, locate **real** Kit Input and Button controls by stable IDs, type, click, and assert the changed greeting. A real GPUI `ListState` with variable-height selectable text is scrolled with native wheel events. Its top visible item's viewport position remains unchanged when two older items are prepended. A drag-selection followed by Ctrl+C copies a message with its newline to the simulated clipboard. The `HistoryProbe` lives in `src/main.rs`'s test module; it is **not** yet the production conversation view. Measuring every row in the small probe makes its scrollbar accurate; a production history will need bounded measurement/height estimates for longer conversations.

## Actual gate results

On this machine `cargo build --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` succeeded (3/3 tests). With no desktop DISPLAY, `xvfb-run -a -s '-screen 0 1280x800x24' env LIBGL_ALWAYS_SOFTWARE=1 timeout 12s target/debug/client-gpui` kept the app running until timeout (exit 124, expected). A root-window capture at 5 seconds showed the 500×500 starter window, greeting, styled input, and button; no runtime errors were logged. The capture is not an assertion in the test suite.

**Not yet manually verified on a physical desktop:** actual mouse selection/clipboard, IME behavior, screen-reader behavior, and native variable-height scrolling. Headless GPUI events and Xvfb rendering do not prove these integrations. Run the app in a desktop session to perform those checks before claiming native UX verification. This gate establishes feasibility of the framework and headless seams, not the complete client or manual smoke test required at prototype exit (#46).
