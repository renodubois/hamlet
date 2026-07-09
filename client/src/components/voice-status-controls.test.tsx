import { describe, expect, test, vi } from "vitest";
import { fireEvent, render, screen } from "../test/testing-library";
import { useSignalState } from "../hooks/react-state";
import VoiceStatusControls from "./voice-status-controls";

interface MockVoiceChatApi {
  activeChannelId: () => number | null;
  setActiveChannelId: (id: number | null) => void;
  isMuted: () => boolean;
  setIsMuted: (v: boolean) => void;
  isDeafened: () => boolean;
  setIsDeafened: (v: boolean) => void;
  toggleMuted: ReturnType<typeof vi.fn>;
  toggleDeafened: ReturnType<typeof vi.fn>;
  leave: ReturnType<typeof vi.fn>;
}

let mockVoice: MockVoiceChatApi;

vi.mock("../contexts/voice-chat", () => ({
  useVoiceChat: () => mockVoice,
}));

function setup() {
  const [activeChannelId, setActiveChannelId] = useSignalState<number | null>(null);
  const [isMuted, setIsMuted] = useSignalState(false);
  const [isDeafened, setIsDeafened] = useSignalState(false);
  mockVoice = {
    activeChannelId,
    setActiveChannelId,
    isMuted,
    setIsMuted,
    isDeafened,
    setIsDeafened,
    toggleMuted: vi.fn<() => Promise<void>>().mockImplementation(async () => {
      setIsMuted(!isMuted());
    }),
    toggleDeafened: vi.fn<() => Promise<void>>().mockImplementation(async () => {
      setIsDeafened(!isDeafened());
    }),
    leave: vi.fn<() => Promise<void>>().mockResolvedValue(),
  };
}

describe("<VoiceStatusControls>", () => {
  test("shows mute and deafen controls even without an active call", () => {
    setup();
    render(() => <VoiceStatusControls />);

    const mute = screen.getByRole("button", { name: "Mute microphone" });
    const deafen = screen.getByRole("button", { name: "Deafen" });
    expect(screen.queryByRole("button", { name: "Disconnect from voice" })).toBeNull();
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

  test("shows disconnect only while connected to voice", async () => {
    setup();
    render(() => <VoiceStatusControls />);

    expect(screen.queryByRole("button", { name: "Disconnect from voice" })).toBeNull();

    mockVoice.setActiveChannelId(42);
    const disconnect = await screen.findByRole("button", { name: "Disconnect from voice" });
    fireEvent.click(disconnect);

    expect(mockVoice.leave).toHaveBeenCalled();
  });
});
