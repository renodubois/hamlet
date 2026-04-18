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

vi.mock("../channels_context", () => ({
  useChannels: () => channelsResource(),
}));

// AddChannelModal pulls in the real api module transitively; stub it to keep
// this test tight on the sidebar itself.
vi.mock("./add_channel_modal", () => ({
  default: () => null,
}));

vi.mock("./settings_modal", () => ({
  default: (props: { open: boolean }) => (
    <Show when={props.open}>
      <div data-testid="settings-modal-stub">settings-open</div>
    </Show>
  ),
}));

import ChannelSidebar from "./channel_sidebar";

const USER: User = {
  id: 1,
  username: "alice",
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
        { id: 10, name: "general", position: 0 },
        { id: 20, name: "random", position: 1 },
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

  test("channel links are marked draggable", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0 },
        { id: 20, name: "random", position: 1 },
      ]),
      refetch: () => {},
      reorder: async () => {},
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    const general = screen.getByText(/general/).closest("a");
    expect(general).toHaveAttribute("draggable", "true");
  });

  test("dragging a channel onto another calls reorder with the new order", async () => {
    const reorder = vi.fn<(ids: number[]) => Promise<void>>().mockResolvedValue();
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0 },
        { id: 20, name: "random", position: 1 },
        { id: 30, name: "dev", position: 2 },
      ]),
      refetch: () => {},
      reorder,
    });

    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);

    const dev = screen.getByText(/dev/).closest("a") as HTMLElement;
    const general = screen.getByText(/general/).closest("a") as HTMLElement;
    const dt = makeDataTransfer();

    // Drag "dev" onto "general" — expect dev to move to index 0.
    fireEvent.dragStart(dev, { dataTransfer: dt });
    fireEvent.dragOver(general, { dataTransfer: dt });
    fireEvent.drop(general, { dataTransfer: dt });

    expect(reorder).toHaveBeenCalledTimes(1);
    expect(reorder).toHaveBeenCalledWith([30, 10, 20]);
  });

  test("dropping a channel on itself is a no-op", () => {
    const reorder = vi.fn<(ids: number[]) => Promise<void>>().mockResolvedValue();
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general", position: 0 },
        { id: 20, name: "random", position: 1 },
      ]),
      refetch: () => {},
      reorder,
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    const general = screen.getByText(/general/).closest("a") as HTMLElement;
    const dt = makeDataTransfer();
    fireEvent.dragStart(general, { dataTransfer: dt });
    fireEvent.dragOver(general, { dataTransfer: dt });
    fireEvent.drop(general, { dataTransfer: dt });
    expect(reorder).not.toHaveBeenCalled();
  });
});
