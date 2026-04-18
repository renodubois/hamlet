import { describe, expect, test, vi } from "vitest";
import { fireEvent, screen } from "@solidjs/testing-library";
import { createResource, Show } from "solid-js";
import type { Channel, User } from "../api";
import { renderWithRouter } from "../test/render";

const channelsResource = vi.hoisted(() =>
  vi.fn<() => { channels: () => Channel[] | undefined; refetch: () => void }>(),
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

describe("<ChannelSidebar>", () => {
  test("renders each channel as a link", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([
        { id: 10, name: "general" },
        { id: 20, name: "random" },
      ]),
      refetch: () => {},
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
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.getByText("alice")).toBeInTheDocument();
  });

  test("opens the settings modal when the Settings button is clicked", () => {
    channelsResource.mockReturnValue({
      channels: fakeChannels([]),
      refetch: () => {},
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
    });
    renderWithRouter(() => <ChannelSidebar user={USER} onLogout={async () => {}} />);
    expect(screen.queryByRole("button", { name: /^log out$/i })).toBeNull();
  });
});
