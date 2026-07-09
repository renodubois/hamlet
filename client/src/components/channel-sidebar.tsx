import { NavLink } from "react-router-dom";

function classes(base: string, conditional: Record<string, boolean>): string {
  return [
    base,
    ...Object.entries(conditional)
      .filter(([, enabled]) => enabled)
      .map(([name]) => name),
  ].join(" ");
}
import { useSignalState, List, Case, If, Choose } from "../hooks/react-state";
import { type Channel, type User } from "../api";
import { useChannels } from "../contexts/channels";
import { useReadStates } from "../contexts/read-states";
import AddChannelModal from "./add-channel-modal";
import Avatar from "./avatar";
import { SettingsIcon } from "./icons";
import SettingsModal from "./settings-modal";
import VoiceChannel from "./voice-channel";
import VoiceStatusControls from "./voice-status-controls";

export default function ChannelSidebar(props: {
  user: User;
  onLogout: () => Promise<void>;
  onAvatarChange?: () => void;
}) {
  const { channels, reorder } = useChannels();
  const readStates = useReadStates();
  const [addChannelOpen, setAddChannelOpen] = useSignalState(false);
  const [settingsOpen, setSettingsOpen] = useSignalState(false);
  const [draggedId, setDraggedId] = useSignalState<number | null>(null);
  const [dropTargetId, setDropTargetId] = useSignalState<number | null>(null);

  function clearDragState() {
    setDraggedId(null);
    setDropTargetId(null);
  }

  function handleDragStart(e: any, id: number) {
    setDraggedId(id);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Some browsers refuse to start a drag unless data is set.
      e.dataTransfer.setData("text/plain", String(id));
    }
  }

  function handleDragOver(e: any, id: number) {
    if (draggedId() === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    if (dropTargetId() !== id) setDropTargetId(id);
  }

  function handleDrop(e: any, targetId: number, list: Channel[]) {
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
    <div className="flex flex-col h-full">
      <div className="p-4 font-bold text-lg border-b border-gray-700">Hamlet</div>

      <nav className="px-2 py-2 border-b border-gray-700" aria-label="Primary">
        <NavLink
          to="/threads"
          className={({ isActive }) =>
            `block px-3 py-1.5 rounded text-sm cursor-pointer ${
              isActive
                ? "bg-gray-700 text-white font-medium"
                : "text-gray-400 hover:bg-gray-700 hover:text-gray-200"
            }`
          }
        >
          Threads
        </NavLink>
      </nav>

      <If when={channels.loading}>
        <p className="px-3 py-2 text-gray-400 text-sm">Loading...</p>
      </If>
      <Choose>
        <Case when={channels.error}>
          <p className="px-3 py-2 text-red-400 text-sm">Error loading channels</p>
        </Case>
        <Case when={channels()}>
          <nav className="flex-1 overflow-y-auto py-2" aria-label="Channels">
            <List each={channels()}>
              {(channel) => (
                <div
                  className={classes("mx-2", {
                    "opacity-50": draggedId() === channel.id,
                    "ring-2 ring-blue-400 rounded":
                      dropTargetId() === channel.id && draggedId() !== channel.id,
                  })}
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
                  <Choose fallback={<p>unknown channel type: {channel.id}</p>}>
                    <Case when={channel.type === "voice"}>
                      <VoiceChannel channel={channel} />
                    </Case>
                    <Case when={channel.type === "text"}>
                      <NavLink
                        to={`/channel/${channel.id}`}
                        className={({ isActive }) =>
                          classes(
                            `relative flex items-center gap-2 px-3 py-1.5 rounded text-sm cursor-pointer ${
                              isActive
                                ? "bg-gray-700 text-white font-medium"
                                : readStates.hasUnread(channel.id) ||
                                    readStates.mentionCount(channel.id) > 0
                                  ? "text-gray-100 font-semibold hover:bg-gray-700 hover:text-gray-100"
                                  : "text-gray-400 hover:bg-gray-700 hover:text-gray-200"
                            }`,
                            {
                              "pl-6":
                                readStates.hasUnread(channel.id) ||
                                readStates.mentionCount(channel.id) > 0,
                            },
                          )
                        }
                        aria-label={
                          readStates.mentionCount(channel.id) > 0
                            ? `${channel.name}, ${readStates.mentionCount(channel.id)} unread mention${
                                readStates.mentionCount(channel.id) === 1 ? "" : "s"
                              }`
                            : readStates.hasUnread(channel.id)
                              ? `${channel.name}, unread messages`
                              : channel.name
                        }
                        // Anchors are draggable by default as URLs, which hijacks
                        // our custom drag-and-drop — suppress that here and let
                        // the wrapping div own the drag.
                        draggable={false}
                      >
                        <If
                          when={
                            readStates.hasUnread(channel.id) ||
                            readStates.mentionCount(channel.id) > 0
                          }
                        >
                          <span
                            aria-hidden="true"
                            className="absolute left-2 h-2 w-2 rounded-full bg-blue-300"
                            data-testid={`channel-unread-dot-${channel.id}`}
                          />
                        </If>
                        <span aria-hidden="true">#</span>
                        <span className="min-w-0 flex-1 truncate">{channel.name}</span>
                        <If when={readStates.mentionCount(channel.id) > 0}>
                          <span
                            className="ml-auto min-w-5 rounded-full bg-red-500 px-1.5 py-0.5 text-center text-[11px] font-bold leading-none text-white"
                            role="img"
                            aria-label={`${readStates.mentionCount(channel.id)} unread mention${
                              readStates.mentionCount(channel.id) === 1 ? "" : "s"
                            } in ${channel.name}`}
                          >
                            {readStates.mentionCount(channel.id)}
                          </span>
                        </If>
                      </NavLink>
                    </Case>
                  </Choose>
                </div>
              )}
            </List>
          </nav>
        </Case>
      </Choose>

      <div className="p-2 border-t border-gray-700">
        <button
          className="w-full text-left px-3 py-1.5 text-sm text-gray-400 hover:bg-gray-700 hover:text-gray-200 rounded"
          onClick={() => setAddChannelOpen(true)}
        >
          + Add Channel
        </button>
      </div>

      <div className="p-3 border-t border-gray-700 flex items-center gap-2">
        <Avatar
          url={props.user.avatar_url}
          username={props.user.display_name ?? props.user.username}
          size={24}
        />
        <span className="text-gray-300 text-sm truncate flex-1 min-w-0">
          {props.user.display_name ?? props.user.username}
        </span>
        <VoiceStatusControls />
        <button
          type="button"
          aria-label="Settings"
          className="text-gray-400 hover:text-gray-100 flex-shrink-0 p-1 rounded hover:bg-gray-700"
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
