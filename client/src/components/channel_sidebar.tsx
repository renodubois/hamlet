import { A } from "@solidjs/router";
import { For, Match, Resource, Show, Switch } from "solid-js";
import { type Channel, type User } from "../api";

export default function ChannelSidebar(props: {
  channels: Resource<Channel[]>;
  user: User;
  onLogout: () => Promise<void>;
}) {
  return (
    <div class="flex flex-col h-full">
      <div class="p-4 font-bold text-lg border-b border-gray-700">Hamlet</div>

      <Show when={props.channels.loading}>
        <p class="px-3 py-2 text-gray-400 text-sm">Loading...</p>
      </Show>
      <Switch>
        <Match when={props.channels.error}>
          <p class="px-3 py-2 text-red-400 text-sm">Error loading channels</p>
        </Match>
        <Match when={props.channels()}>
          <nav class="flex-1 overflow-y-auto py-2">
            <For each={props.channels()}>
              {(channel) => (
                <A
                  href={`/channel/${channel.id}`}
                  activeClass="bg-gray-700 text-white font-medium"
                  inactiveClass="text-gray-400 hover:bg-gray-700 hover:text-gray-200"
                  class="block px-3 py-1.5 mx-2 rounded text-sm cursor-pointer"
                >
                  # {channel.name}
                </A>
              )}
            </For>
          </nav>
        </Match>
      </Switch>

      <div class="p-3 border-t border-gray-700 flex items-center justify-between">
        <span class="text-gray-300 text-sm truncate">{props.user.username}</span>
        <button
          class="text-gray-400 hover:text-gray-100 text-sm ml-2 flex-shrink-0"
          onClick={props.onLogout}
        >
          Log out
        </button>
      </div>
    </div>
  );
}
