import { A } from "@solidjs/router";
import { createSignal, For, Match, Show, Switch } from "solid-js";
import { type Channel, type User } from "../api";
import { useChannels } from "../contexts/channels";
import AddChannelModal from "./add-channel-modal";
import Avatar from "./avatar";
import { SettingsIcon } from "./icons";
import SettingsModal from "./settings-modal";
import VoiceChannel from "./voice-channel";

export default function ChannelSidebar(props: {
  user: User;
  onLogout: () => Promise<void>;
  onAvatarChange?: () => void;
}) {
  const { channels, reorder } = useChannels();
  const [addChannelOpen, setAddChannelOpen] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [draggedId, setDraggedId] = createSignal<number | null>(null);
  const [dropTargetId, setDropTargetId] = createSignal<number | null>(null);

  function clearDragState() {
    setDraggedId(null);
    setDropTargetId(null);
  }

  function handleDragStart(e: DragEvent, id: number) {
    setDraggedId(id);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Some browsers refuse to start a drag unless data is set.
      e.dataTransfer.setData("text/plain", String(id));
    }
  }

  function handleDragOver(e: DragEvent, id: number) {
    if (draggedId() === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    if (dropTargetId() !== id) setDropTargetId(id);
  }

  function handleDrop(e: DragEvent, targetId: number, list: Channel[]) {
    e.preventDefault();
    const sourceId = draggedId();
    clearDragState();
    if (sourceId == null || sourceId === targetId) return;
    const ids = list.map((c) => c.id);
    const from = ids.indexOf(sourceId);
    const to = ids.indexOf(targetId);
    if (from < 0 || to < 0) return;
    const next = [...ids];
    next.splice(from, 1);
    next.splice(to, 0, sourceId);
    void reorder(next).catch((err) => {
      console.error("failed to reorder channels", err);
    });
  }

  return (
    <div class="flex flex-col h-full">
      <div class="p-4 font-bold text-lg border-b border-gray-700">Hamlet</div>

      <Show when={channels.loading}>
        <p class="px-3 py-2 text-gray-400 text-sm">Loading...</p>
      </Show>
      <Switch>
        <Match when={channels.error}>
          <p class="px-3 py-2 text-red-400 text-sm">Error loading channels</p>
        </Match>
        <Match when={channels()}>
          <nav class="flex-1 overflow-y-auto py-2" aria-label="Channels">
            <For each={channels()}>
              {(channel) => (
                <div
                  class="mx-2"
                  classList={{
                    "opacity-50": draggedId() === channel.id,
                    "ring-2 ring-blue-400 rounded":
                      dropTargetId() === channel.id && draggedId() !== channel.id,
                  }}
                  draggable={true}
                  data-channel-id={channel.id}
                  data-channel-type={channel.type}
                  onDragStart={(e) => handleDragStart(e, channel.id)}
                  onDragOver={(e) => handleDragOver(e, channel.id)}
                  onDragLeave={() => {
                    if (dropTargetId() === channel.id) setDropTargetId(null);
                  }}
                  onDrop={(e) => handleDrop(e, channel.id, channels() ?? [])}
                  onDragEnd={clearDragState}
                >
                  <Show
                    when={channel.type === "voice"}
                    fallback={
                      <A
                        href={`/channel/${channel.id}`}
                        activeClass="bg-gray-700 text-white font-medium"
                        inactiveClass="text-gray-400 hover:bg-gray-700 hover:text-gray-200"
                        class="block px-3 py-1.5 rounded text-sm cursor-pointer"
                        // Anchors are draggable by default as URLs, which hijacks
                        // our custom drag-and-drop — suppress that here and let
                        // the wrapping div own the drag.
                        draggable={false}
                      >
                        # {channel.name}
                      </A>
                    }
                  >
                    <VoiceChannel channel={channel} />
                  </Show>
                </div>
              )}
            </For>
          </nav>
        </Match>
      </Switch>

      <div class="p-2 border-t border-gray-700">
        <button
          class="w-full text-left px-3 py-1.5 text-sm text-gray-400 hover:bg-gray-700 hover:text-gray-200 rounded"
          onClick={() => setAddChannelOpen(true)}
        >
          + Add Channel
        </button>
      </div>

      <div class="p-3 border-t border-gray-700 flex items-center gap-2">
        <Avatar
          url={props.user.avatar_url}
          username={props.user.display_name ?? props.user.username}
          size={24}
        />
        <span class="text-gray-300 text-sm truncate flex-1 min-w-0">
          {props.user.display_name ?? props.user.username}
        </span>
        <button
          type="button"
          aria-label="Settings"
          class="text-gray-400 hover:text-gray-100 flex-shrink-0 p-1 rounded hover:bg-gray-700"
          onClick={() => setSettingsOpen(true)}
        >
          <SettingsIcon size={18} aria-hidden="true" />
        </button>
      </div>

      <AddChannelModal open={addChannelOpen()} onClose={() => setAddChannelOpen(false)} />
      <SettingsModal
        open={settingsOpen()}
        onClose={() => setSettingsOpen(false)}
        onLogout={props.onLogout}
        user={props.user}
        onAvatarChange={props.onAvatarChange}
      />
    </div>
  );
}
