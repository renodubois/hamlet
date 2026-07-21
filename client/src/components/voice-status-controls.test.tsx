import { useState } from "react";
import { describe, expect, test, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { renderNative } from "../test/render";
import VoiceStatusControls from "./voice-status-controls";

let mockVoice: {
  activeChannelId: number | null;
  isMuted: boolean;
  isDeafened: boolean;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
  leave: ReturnType<typeof vi.fn>;
};

vi.mock("../contexts/voice-chat", () => ({ useVoiceChat: () => mockVoice }));

function renderControls(initialChannelId: number | null = null) {
  function Harness() {
    const [activeChannelId, setActiveChannelId] = useState(initialChannelId);
    const [isMuted, setIsMuted] = useState(false);
    const [isDeafened, setIsDeafened] = useState(false);
    mockVoice = {
      activeChannelId,
      isMuted,
      isDeafened,
      toggleMuted: vi.fn(async () => setIsMuted((value) => !value)),
      toggleDeafened: vi.fn(async () => setIsDeafened((value) => !value)),
      leave: vi.fn(async () => setActiveChannelId(null)),
    };
    return <VoiceStatusControls />;
  }
  return renderNative(<Harness />);
}

describe("<VoiceStatusControls>", () => {
  test("shows direct-value mute and deafen controls even without an active call", () => {
    renderControls();
    const mute = screen.getByRole("button", { name: "Mute microphone" });
    const deafen = screen.getByRole("button", { name: "Deafen" });
    expect(screen.queryByRole("button", { name: "Disconnect from voice" })).toBeNull();
    fireEvent.click(mute);
    fireEvent.click(deafen);
    expect(screen.getByRole("button", { name: "Unmute microphone" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Undeafen" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  test.each([
    ["Mute microphone", "toggleMuted", "mute microphone"],
    ["Deafen", "toggleDeafened", "deafen"],
  ] as const)(
    "surfaces a rejected %s command in an accessible alert",
    async (label, command, action) => {
      renderControls();
      mockVoice[command].mockRejectedValueOnce(new Error("device unavailable"));

      fireEvent.click(screen.getByRole("button", { name: label }));

      await waitFor(() =>
        expect(screen.getByRole("alert")).toHaveTextContent(
          `Could not ${action}: device unavailable`,
        ),
      );
    },
  );

  test("shows disconnect only while connected to voice", () => {
    renderControls(42);
    const disconnect = screen.getByRole("button", { name: "Disconnect from voice" });
    const leave = mockVoice.leave;
    fireEvent.click(disconnect);
    expect(leave).toHaveBeenCalled();
  });
});
