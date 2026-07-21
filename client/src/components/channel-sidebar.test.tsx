import { StrictMode, useRef } from "react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, test, vi } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import type { Channel, User } from "../api";
import { expectNoA11yViolations } from "../test/a11y";
import { renderNative, renderWithRouterNative } from "../test/render";

const channelsResource = vi.hoisted(() =>
  vi.fn<
    () => {
      channels: () => Channel[] | undefined;
      refetch: () => void;
      reorder: (ids: number[]) => Promise<void>;
    }
  >(),
);

const readStatesContext = vi.hoisted(() =>
  vi.fn<
    () => {
      hasUnread: (channelId: number) => boolean;
      mentionCount: (channelId: number) => number;
    }
  >(),
);

vi.mock("../contexts/channels", () => ({
  useChannels: () => channelsResource(),
}));

vi.mock("../contexts/read-states", () => ({
  useReadStates: () => readStatesContext(),
}));

// AddChannelModal pulls in the real api module transitively; stub it to keep
// this test tight on the sidebar itself.
vi.mock("./add-channel-modal", () => ({
  default: () => null,
}));

vi.mock("./settings-modal", () => ({
  default: (props: { open: boolean }) =>
    props.open ? <div data-testid="settings-modal-stub">settings-open</div> : null,
}));

vi.mock("./voice-status-controls", () => ({
  default: () => <div data-testid="voice-status-controls-stub" />,
}));

// VoiceChannel pulls in livekit-client transitively and needs the
// VoiceChatProvider. Stub it out so the sidebar's own behavior stays testable
// without standing up the whole voice stack — it has dedicated coverage in
// voice-channel.test.tsx.
vi.mock("./voice-channel", () => ({
  default: function VoiceChannelStub(props: { channel: Channel }) {
    const initialChannelId = useRef(props.channel.id);
    return (
      <button
        type="button"
        data-testid={`voice-channel-${props.channel.id}`}
        data-initial-channel-id={initialChannelId.current}
      >
        {props.channel.name}
      </button>
    );
  },
}));

import ChannelSidebar from "./channel-sidebar";

const USER: User = {
  id: 1,
  username: "alice",
  display_name: null,
  email: null,
  email_verified: false,
  avatar_url: null,
};

function fakeChannels(data: Channel[] | undefined) {
  const wrapped = (() => data) as (() => Channel[] | undefined) & {
    loading: boolean;
    error: unknown;
    state: "ready" | "pending";
  };
  wrapped.loading = data === undefined;
  wrapped.error = undefined;
  wrapped.state = data === undefined ? "pending" : "ready";
  return wrapped;
}

function makeDataTransfer(): DataTransfer {
  const store = new Map<string, string>();
  return {
    data: store,
    effectAllowed: "none",
    dropEffect: "none",
    setData(type: string, value: string) {
      store.set(type, value);
    },
    getData(type: string) {
      return store.get(type) ?? "";
    },
  } as unknown as DataTransfer;
}

describe("<ChannelSidebar>", () => {
  beforeEach(() => {
    readStatesContext.mockReturnValue({
      hasUnread: () => false,
      mentionCount: () => 0,
    });
  });

  test("renders each channel as a link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });

    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);

    expect(screen.getByText(/general/)).toBeInTheDocument();
    expect(screen.getByText(/random/)).toBeInTheDocument();
    const general = screen.getByText(/general/).closest("a");
    expect(general).toHaveAttribute("href", "/channel/10");
  });

  test("renders a persistent Threads navigation link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);

    expect(screen.getByRole("link", { name: /^threads$/i })).toHaveAttribute("href", "/threads");
  });

  test("shows the current user's name", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  test("opens the settings modal when the Settings button is clicked", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.queryByTestId("settings-modal-stub")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByTestId("settings-modal-stub")).toBeInTheDocument();
  });

  test("does not render a Log out button (logout lives inside settings)", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.queryByRole("button", { name: /^log out$/i })).toBeNull();
  });

  test("channel rows are marked draggable", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);
    const row = screen.getByText(/general/).closest("[data-channel-id]");
    expect(row).toHaveAttribute("draggable", "true");
    // The inner anchor should opt out of the native URL-drag behavior so it
    // doesn't hijack the wrapping div's drag handlers.
    const link = screen.getByText(/general/).closest("a");
    expect(link).toHaveAttribute("draggable", "false");
  });

  test("dragging a channel onto another calls reorder with the new order", async () => {
    const reorder = vi.fn<(ids: number[]) => Promise<void>>().mockResolvedValue();
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
        { id: 30, name: "dev", position: 2, type: "text" },
      ]),
      refetch: () => {},
      reorder,
    });

    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);

    const devRow = screen.getByText(/dev/).closest("[data-channel-id]") as HTMLElement;
    const generalRow = screen.getByText(/general/).closest("[data-channel-id]") as HTMLElement;
    const dt = makeDataTransfer();

    // Drag "dev" onto "general" — expect dev to move to index 0.
    fireEvent.dragStart(devRow, { dataTransfer: dt });
    fireEvent.dragOver(generalRow, { dataTransfer: dt });
    fireEvent.drop(generalRow, { dataTransfer: dt });

    expect(reorder).toHaveBeenCalledTimes(1);
    expect(reorder).toHaveBeenCalledWith([30, 10, 20]);
  });

  test("renders ordinary unread treatment for unread text channels only", () => {
    readStatesContext.mockReturnValue({
      hasUnread: (channelId) => channelId === 20 || channelId === 40,
      mentionCount: () => 0,
    });
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
        { id: 40, name: "lobby", position: 2, type: "voice" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });

    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);

    expect(screen.queryByTestId("channel-unread-dot-10")).toBeNull();
    expect(screen.getByTestId("channel-unread-dot-20")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /random, unread messages/i })).toBeInTheDocument();
    expect(screen.queryByTestId("channel-unread-dot-40")).toBeNull();
    expect(screen.getByTestId("voice-channel-40")).toBeInTheDocument();
  });

  test("renders numeric mention badges with accessible names", async () => {
    readStatesContext.mockReturnValue({
      hasUnread: (channelId) => channelId === 20,
      mentionCount: (channelId) => (channelId === 20 ? 3 : 0),
    });
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 20, name: "random", position: 0, type: "text" },
        { id: 40, name: "lobby", position: 1, type: "voice" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });

    const { container } = renderWithRouterNative(
      <ChannelSidebar user={USER} onLogout={async () => {}} />,
    );

    expect(screen.getByRole("link", { name: /random, 3 unread mentions/i })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /3 unread mentions in random/i })).toHaveTextContent(
      "3",
    );
    expect(screen.queryByRole("img", { name: /lobby/i })).toBeNull();
    await expectNoA11yViolations(container, "sidebar mention unread badge");
  });

  test("channel reordering retains row and VoiceChannel identity by channel ID", () => {
    let channelData: Channel[] = [
      { id: 10, name: "general", position: 0, type: "text" },
      { id: 40, name: "lobby", position: 1, type: "voice" },
    ];
    channelsResource.mockImplementation(() => ({
      channels: fakeChannels(channelData),
      refetch: () => {},
      reorder: async () => {},
    }));

    const { rerender } = renderNative(
      <MemoryRouter>
        <ChannelSidebar user={USER} onLogout={async () => {}} />
      </MemoryRouter>,
    );
    const textRow = screen.getByText("general").closest("[data-channel-id]");
    const voiceRow = screen.getByTestId("voice-channel-40").closest("[data-channel-id]");

    channelData = [channelData[1], channelData[0]];
    rerender(
      <StrictMode>
        <MemoryRouter>
          <ChannelSidebar user={USER} onLogout={async () => {}} />
        </MemoryRouter>
      </StrictMode>,
    );

    expect(screen.getByText("general").closest("[data-channel-id]")).toBe(textRow);
    expect(screen.getByTestId("voice-channel-40").closest("[data-channel-id]")).toBe(voiceRow);
    expect(screen.getByTestId("voice-channel-40")).toHaveAttribute("data-initial-channel-id", "40");
  });

  test("voice channels render the VoiceChannel component instead of a link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 40, name: "lobby", position: 1, type: "voice" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);

    // Voice channels delegate to VoiceChannel (stubbed above) — they do not
    // render as navigation anchors.
    expect(screen.queryByRole("link", { name: /lobby/ })).toBeNull();
    expect(screen.getByTestId("voice-channel-40")).toBeInTheDocument();

    // Text channels still render as anchors.
    const general = screen.getByRole("link", { name: /general/ });
    expect(general).toHaveAttribute("href", "/channel/10");
  });

  test("dropping a channel on itself is a no-op", () => {
    const reorder = vi.fn<(ids: number[]) => Promise<void>>().mockResolvedValue();
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
      ]),
      refetch: () => {},
      reorder,
    });
    renderWithRouterNative(<ChannelSidebar user={USER} onLogout={async () => {}} />);
    const row = screen.getByText(/general/).closest("[data-channel-id]") as HTMLElement;
    const dt = makeDataTransfer();
    fireEvent.dragStart(row, { dataTransfer: dt });
    fireEvent.dragOver(row, { dataTransfer: dt });
    fireEvent.drop(row, { dataTransfer: dt });
    expect(reorder).not.toHaveBeenCalled();
  });
});
