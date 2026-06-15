import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import VoiceStatusControls from "./voice-status-controls";

interface MockVoiceChatApi {
  isMuted: () => boolean;
  setIsMuted: (v: boolean) => void;
  isDeafened: () => boolean;
  setIsDeafened: (v: boolean) => void;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
}

let mockVoice: MockVoiceChatApi;

vi.mock("../contexts/voice-chat", () => ({
  useVoiceChat: () => mockVoice,
}));

function setup() {
  const [isMuted, setIsMuted] = createSignal(false);
  const [isDeafened, setIsDeafened] = createSignal(false);
  mockVoice = {
    isMuted,
    setIsMuted,
    isDeafened,
    setIsDeafened,
    toggleMuted: vi.fn<() => Promise<void>>().mockImplementation(async () => {
      setIsMuted(!isMuted());
    }),
    toggleDeafened: vi.fn<() => void>().mockImplementation(() => {
      setIsDeafened(!isDeafened());
    }),
  };
}

describe("<VoiceStatusControls>", () => {
  test("shows mute and deafen controls even without an active call", () => {
    setup();
    render(() => <VoiceStatusControls />);

    const mute = screen.getByRole("button", { name: "Mute microphone" });
    const deafen = screen.getByRole("button", { name: "Deafen" });
    expect(mute).toHaveAttribute("aria-pressed", "false");
    expect(deafen).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(mute);
    fireEvent.click(deafen);

    expect(mockVoice.toggleMuted).toHaveBeenCalled();
    expect(mockVoice.toggleDeafened).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Unmute microphone" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Undeafen" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
