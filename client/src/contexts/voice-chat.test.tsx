/* oxlint-disable typescript/unbound-method -- VoiceSession test doubles are arrow-function spies. */
import { act, fireEvent, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";

import { renderNative } from "../test/render";
import { VoicePreferencesProvider, useVoicePreferences } from "./voice-preferences";
import { createVoiceSnapshot, updateVoiceSnapshot, type VoiceSnapshot } from "../voice/voice-state";
import type { VoiceSession } from "../voice/voice-session";
import { VoiceChatProvider, useVoiceChat, type VoiceChatContextValue } from "./voice-chat";

function makeSession() {
  let snapshot: VoiceSnapshot = createVoiceSnapshot();
  const listeners = new Set<() => void>();
  const session: VoiceSession = {
    subscribe: vi.fn((listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    }),
    getSnapshot: vi.fn(() => snapshot),
    activate: vi.fn(),
    deactivate: vi.fn(async () => {}),
    join: vi.fn(async () => {}),
    leave: vi.fn(async () => {}),
    toggleMuted: vi.fn(async () => {}),
    toggleDeafened: vi.fn(async () => {}),
    startScreenShare: vi.fn(async () => {}),
    stopScreenShare: vi.fn(async () => {}),
    startCamera: vi.fn(async () => {}),
    stopCamera: vi.fn(async () => {}),
    syncRemoteCameraStreams: vi.fn(),
    applyPreferences: vi.fn(),
    watchScreenShare: vi.fn(async () => {}),
    stopWatchingScreenShare: vi.fn(async () => {}),
  };
  return {
    session,
    publish(update: Parameters<typeof updateVoiceSnapshot>[1]) {
      snapshot = updateVoiceSnapshot(snapshot, update);
      act(() => listeners.forEach((listener) => listener()));
    },
  };
}

function Harness() {
  const voice = useVoiceChat();
  return (
    <>
      <output data-testid="projection">
        {voice.activeChannelId ?? "none"}:{voice.isConnecting ? "connecting" : "idle"}:
        {voice.isMuted ? "muted" : "open"}:{voice.isCameraEnabled ? "camera" : "no-camera"}
      </output>
      <button type="button" onClick={() => void voice.join(42)}>
        Join
      </button>
      <button type="button" onClick={() => void voice.toggleMuted()}>
        Mute
      </button>
      <button type="button" onClick={() => voice.syncRemoteCameraStreams(42, [])}>
        Sync
      </button>
    </>
  );
}

function PreferenceControl() {
  const preferences = useVoicePreferences();
  return (
    <button type="button" onClick={() => preferences.setOutputDeviceId("speakers-b")}>
      Output B
    </button>
  );
}

function renderProvider(session: VoiceSession) {
  return renderNative(
    <VoicePreferencesProvider>
      <VoiceChatProvider createSession={() => session}>
        <Harness />
        <PreferenceControl />
      </VoiceChatProvider>
    </VoicePreferencesProvider>,
  );
}

describe("VoiceChatProvider", () => {
  test("projects immutable session snapshots as direct values", () => {
    const fake = makeSession();
    renderProvider(fake.session);
    expect(screen.getByTestId("projection")).toHaveTextContent("none:idle:open:no-camera");

    fake.publish({
      activeChannelId: 42,
      connectionStatus: "connecting",
      muted: true,
      cameraStatus: "on",
    });

    expect(screen.getByTestId("projection")).toHaveTextContent("42:connecting:muted:camera");
  });

  test("keeps every command referentially stable across snapshot updates", () => {
    const fake = makeSession();
    const values: VoiceChatContextValue[] = [];
    function Observer() {
      values.push(useVoiceChat());
      return null;
    }
    renderNative(
      <VoicePreferencesProvider>
        <VoiceChatProvider createSession={() => fake.session}>
          <Observer />
        </VoiceChatProvider>
      </VoicePreferencesProvider>,
    );
    const before = values.at(-1);
    fake.publish({
      connectionStatus: "connecting",
      cameraStatus: "stopping",
      screenShareStatus: "stopping",
    });
    const after = values.at(-1);
    expect(before).toBeDefined();
    expect(after).toBeDefined();
    for (const command of [
      "join",
      "leave",
      "toggleMuted",
      "toggleDeafened",
      "startScreenShare",
      "stopScreenShare",
      "startCamera",
      "stopCamera",
      "syncRemoteCameraStreams",
      "watchScreenShare",
      "stopWatchingScreenShare",
    ] as const) {
      expect(after?.[command]).toBe(before?.[command]);
    }
    expect(after?.cameraStatus).toBe("stopping");
    expect(after?.screenShareStatus).toBe("stopping");
    expect(after?.isCameraBusy).toBe(true);
    expect(after?.isScreenShareBusy).toBe(true);
  });

  test("keeps a stopping screen share visibly active until its publication disappears", () => {
    const fake = makeSession();
    const values: VoiceChatContextValue[] = [];
    function Observer() {
      values.push(useVoiceChat());
      return null;
    }
    renderNative(
      <VoicePreferencesProvider>
        <VoiceChatProvider createSession={() => fake.session}>
          <Observer />
        </VoiceChatProvider>
      </VoicePreferencesProvider>,
    );

    fake.publish({ screenShareStatus: "stopping", screenSharePublicationVisible: true });
    expect(values.at(-1)).toMatchObject({
      screenShareStatus: "stopping",
      isScreenSharing: true,
      isScreenShareBusy: true,
    });
  });

  test("forwards stable commands to the session", () => {
    const fake = makeSession();
    renderProvider(fake.session);
    fireEvent.click(screen.getByRole("button", { name: "Join" }));
    fireEvent.click(screen.getByRole("button", { name: "Mute" }));
    fireEvent.click(screen.getByRole("button", { name: "Sync" }));
    expect(fake.session.join).toHaveBeenCalledWith(42);
    expect(fake.session.toggleMuted).toHaveBeenCalledOnce();
    expect(fake.session.syncRemoteCameraStreams).toHaveBeenCalledWith(42, []);
  });

  test("bridges preference snapshots into the active session", () => {
    const fake = makeSession();
    renderProvider(fake.session);
    fireEvent.click(screen.getByRole("button", { name: "Output B" }));
    expect(fake.session.applyPreferences).toHaveBeenLastCalledWith(
      expect.objectContaining({ outputDeviceId: "speakers-b" }),
    );
  });

  test("pairs activation and deactivation during Strict Mode replay and unmount", () => {
    const fake = makeSession();
    const view = renderProvider(fake.session);
    expect(fake.session.activate).toHaveBeenCalledTimes(2);
    expect(fake.session.deactivate).toHaveBeenCalledTimes(1);
    view.unmount();
    expect(fake.session.deactivate).toHaveBeenCalledTimes(2);
  });

  test("throws a clear error when the provider is missing", () => {
    function MissingProviderHarness() {
      useVoiceChat();
      return null;
    }
    expect(() => renderNative(<MissingProviderHarness />)).toThrow(
      "useVoiceChat must be used inside VoiceChatProvider",
    );
  });
});
