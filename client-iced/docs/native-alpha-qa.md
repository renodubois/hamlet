# Native desktop alpha manual QA checklist

Run this checklist against a packaged native build and a development server before tagging an alpha. The existing Tauri/Solid client in `../client/` remains available and should not be removed while this native checklist is still required.

## Setup

- [ ] Start the Hamlet server and LiveKit stack: `cd ../server && docker compose up --build`.
- [ ] Build/run the native client from `client-iced/` or install the packaged artifact for the target platform.
- [ ] Log in with seeded credentials (`baipas` / `password`) and confirm the saved server URL is used.

## Windowing, focus, and resizing

- [ ] Launch shows the Hamlet app name/icon in the window switcher or dock/taskbar.
- [ ] The app opens at a usable desktop size and cannot be resized below the minimum content width/height.
- [ ] Resize the window repeatedly, including narrow and tall layouts; channel list, message list, composer, settings, image previews, and voice controls remain usable.
- [ ] Move the window between normal-DPI and high-DPI displays; text, icons, avatars, image previews, and pointer hit targets remain crisp and correctly scaled.
- [ ] Keyboard focus starts in a sensible login field when signed out; after login, Tab/Shift+Tab traverse interactive controls without trapping focus.
- [ ] Opening and closing settings returns focus to the main app instead of losing keyboard input.

## Modal/popover dismissal

- [ ] Open the settings panel and close it with the visible close control.
- [ ] Open the emoji picker, navigate with ArrowUp/ArrowDown/Home/End, insert with Enter, and dismiss with Escape.
- [ ] Dismissing the emoji picker returns focus to the message composer with the draft preserved.
- [ ] Popovers/panels do not remain open after logging out or after session expiration.

## Channel reorder interactions

- [ ] Use channel move up/down controls to reorder text and voice channels.
- [ ] Confirm the local order changes immediately and the server-confirmed order remains after reload/relogin.
- [ ] Simulate or trigger a failed reorder and confirm the old order is restored or a clear error is shown.
- [ ] Confirm keyboard users can reach and activate the reorder controls without drag-and-drop.

## File dialogs and image previews

- [ ] Open the avatar file dialog from settings and cancel it; no state changes and no error is shown.
- [ ] Select a valid image file; the avatar upload completes and the updated avatar appears in the sidebar and existing visible messages.
- [ ] Select an unsupported/non-image file; a clear upload failure is shown and the previous avatar remains visible.
- [ ] Delete the avatar and confirm the deterministic fallback avatar returns.
- [ ] Send image/link URLs that produce embed previews; images load, scale within the message column, and failures degrade to text/external-open controls.

## Shutdown and cleanup

- [ ] Close the app while signed out; it exits without background processes.
- [ ] Close the app while signed in but not in voice; no stale realtime connection remains visible to another client.
- [ ] Join a voice channel, then close the app; another client no longer shows the user in voice after the server/LiveKit cleanup interval.
- [ ] Reopen the app; saved server URL, session token, and voice device preferences restore correctly, and no stale voice connection is shown.

## Microphone and voice behavior

- [ ] On macOS, the first voice join prompts for microphone permission with the Hamlet-specific usage text.
- [ ] Deny microphone permission; the app shows a recoverable permission message with instructions to allow access in OS settings.
- [ ] Allow microphone permission and join voice; connection status reaches connected and participants update.
- [ ] Mute/unmute changes the local microphone state and posts speaking changes only when appropriate.
- [ ] Deafen/undeafen toggles remote audio playback state without disconnecting from the room.
- [ ] Switch between two voice channels; the old room disconnects before the new one is marked connected.
- [ ] Leave voice; participants and speaking indicators clear locally and in another client.
