import { describe, expect, test, vi } from "vitest";
import { fireEvent, screen } from "@solidjs/testing-library";
import { createResource, Show } from "solid-js";
import type { Channel, User } from "../api";
import { renderWithRouter } from "../test/render";

const channelsResource = vi.hoisted(() =>
  vi.fn<
    () => {
      channels: () => Channel[] | undefined;
      refetch: () => void;
      reorder: (ids: number[]) => Promise<void>;
    }
  >(),
);

vi.mock("../contexts/channels", () => ({
  useChannels: () => channelsResource(),
}));

// AddChannelModal pulls in the real api module transitively; stub it to keep
// this test tight on the sidebar itself.
vi.mock("./add-channel-modal", () => ({
  default: () => null,
}));

vi.mock("./settings-modal", () => ({
  default: (props: { open: boolean }) => (
    <Show when={props.open}>
      <div data-testid="settings-modal-stub">settings-open</div>
    </Show>
  ),
}));

// VoiceChannel pulls in livekit-client transitively and needs the
// VoiceChatProvider. Stub it out so the sidebar's own behavior stays testable
// without standing up the whole voice stack — it has dedicated coverage in
// voice-channel.test.tsx.
vi.mock("./voice-channel", () => ({
  default: (props: { channel: Channel }) => (
    <button type="button" data-testid={`voice-channel-${props.channel.id}`}>
      {props.channel.name}
    </button>
  ),
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
  // createResource gives us something shaped like a Resource (loading/error/etc).
  const [resource] = createResource(async () => data ?? []);
  const wrapped = (() => (data === undefined ? undefined : data)) as unknown as () => Channel[];
  Object.assign(wrapped, {
    loading: resource.loading,
    error: undefined,
    state: data ? "ready" : "pending",
  });
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
  test("renders each channel as a link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 20, name: "random", position: 1, type: "text" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });

    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);

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
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);

    expect(screen.getByRole("link", { name: /^threads$/i })).toHaveAttribute("href", "/threads");
  });

  test("shows the current user's name", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  test("opens the settings modal when the Settings button is clicked", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
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
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
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
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
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

    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);

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

  test("voice channels render the VoiceChannel component instead of a link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0, type: "text" },
        { id: 40, name: "lobby", position: 1, type: "voice" },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);

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
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    const row = screen.getByText(/general/).closest("[data-channel-id]") as HTMLElement;
    const dt = makeDataTransfer();
    fireEvent.dragStart(row, { dataTransfer: dt });
    fireEvent.dragOver(row, { dataTransfer: dt });
    fireEvent.drop(row, { dataTransfer: dt });
    expect(reorder).not.toHaveBeenCalled();
  });
});
