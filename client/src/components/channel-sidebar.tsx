import { useRef, useState, type DragEvent } from "react";
import { NavLink } from "react-router-dom";

function classes(base: string, conditional: Record<string, boolean>): string {
  return [
    base,
    ...Object.entries(conditional)
      .filter(([, enabled]) => enabled)
      .map(([name]) => name),
  ].join(" ");
}
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
  const { channels, status, error, reorder } = useChannels();
  const readStates = useReadStates();
  const [addChannelOpen, setAddChannelOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [draggedId, setDraggedId] = useState<number | null>(null);
  const [dropTargetId, setDropTargetId] = useState<number | null>(null);
  const draggedIdRef = useRef<number | null>(null);
  const dropTargetIdRef = useRef<number | null>(null);
  const channelList = channels;

  function clearDragState() {
    draggedIdRef.current = null;
    dropTargetIdRef.current = null;
    setDraggedId(null);
    setDropTargetId(null);
  }

  function handleDragStart(e: DragEvent<HTMLDivElement>, id: number) {
    draggedIdRef.current = id;
    setDraggedId(id);
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "move";
      // Some browsers refuse to start a drag unless data is set.
      e.dataTransfer.setData("text/plain", String(id));
    }
  }

  function handleDragOver(e: DragEvent<HTMLDivElement>, id: number) {
    if (draggedIdRef.current === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
    if (dropTargetIdRef.current !== id) {
      dropTargetIdRef.current = id;
      setDropTargetId(id);
    }
  }

  function handleDrop(e: DragEvent<HTMLDivElement>, targetId: number, list: readonly Channel[]) {
    e.preventDefault();
    const sourceId = draggedIdRef.current;
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
      <div className="p-4 font-bold text-lg tracking-tight border-b border-sidebar-border">
        Hamlet
      </div>

      <nav className="px-2 py-2 border-b border-sidebar-border" aria-label="Primary">
        <NavLink
          to="/threads"
          className={({ isActive }) =>
            `block px-3 py-1.5 rounded-md text-sm cursor-pointer transition-colors ${
              isActive
                ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
                : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
            }`
          }
        >
          Threads
        </NavLink>
      </nav>

      {status === "loading" ? (
        <p className="px-3 py-2 text-sidebar-foreground/70 text-sm">Loading...</p>
      ) : null}
      {error ? (
        <p className="px-3 py-2 text-destructive text-sm">Error loading channels</p>
      ) : (
        <nav className="flex-1 overflow-y-auto py-2" aria-label="Channels">
          {channelList.map((channel) => (
            <div
              key={channel.id}
              className={classes("mx-2", {
                "opacity-50": draggedId === channel.id,
                "ring-2 ring-sidebar-ring rounded-md":
                  dropTargetId === channel.id && draggedId !== channel.id,
              })}
              draggable={true}
              data-channel-id={channel.id}
              data-channel-type={channel.type}
              onDragStart={(e) => handleDragStart(e, channel.id)}
              onDragOver={(e) => handleDragOver(e, channel.id)}
              onDragLeave={() => {
                if (dropTargetIdRef.current === channel.id) {
                  dropTargetIdRef.current = null;
                  setDropTargetId(null);
                }
              }}
              onDrop={(e) => handleDrop(e, channel.id, channelList)}
              onDragEnd={clearDragState}
            >
              {channel.type === "voice" ? (
                <VoiceChannel channel={channel} />
              ) : channel.type === "text" ? (
                <NavLink
                  to={`/channel/${channel.id}`}
                  className={({ isActive }) =>
                    classes(
                      `relative flex items-center gap-2 px-3 py-1.5 rounded-md text-sm cursor-pointer transition-colors ${
                        isActive
                          ? "bg-sidebar-accent text-sidebar-accent-foreground font-medium"
                          : readStates.hasUnread(channel.id) ||
                              readStates.mentionCount(channel.id) > 0
                            ? "text-sidebar-foreground font-semibold hover:bg-sidebar-accent"
                            : "text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground"
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
                  {readStates.hasUnread(channel.id) || readStates.mentionCount(channel.id) > 0 ? (
                    <span
                      aria-hidden="true"
                      className="absolute left-2 h-2 w-2 rounded-full bg-sidebar-primary"
                      data-testid={`channel-unread-dot-${channel.id}`}
                    />
                  ) : null}
                  <span aria-hidden="true">#</span>
                  <span className="min-w-0 flex-1 truncate">{channel.name}</span>
                  {readStates.mentionCount(channel.id) > 0 ? (
                    <span
                      className="ml-auto min-w-5 rounded-full bg-destructive px-1.5 py-0.5 text-center text-[11px] font-bold leading-none text-white"
                      role="img"
                      aria-label={`${readStates.mentionCount(channel.id)} unread mention${
                        readStates.mentionCount(channel.id) === 1 ? "" : "s"
                      } in ${channel.name}`}
                    >
                      {readStates.mentionCount(channel.id)}
                    </span>
                  ) : null}
                </NavLink>
              ) : (
                <p>unknown channel type: {channel.id}</p>
              )}
            </div>
          ))}
        </nav>
      )}

      <div className="p-2 border-t border-sidebar-border">
        <button
          className="w-full text-left px-3 py-1.5 text-sm text-sidebar-foreground/70 hover:bg-sidebar-accent hover:text-sidebar-foreground rounded-md transition-colors"
          onClick={() => setAddChannelOpen(true)}
        >
          + Add Channel
        </button>
      </div>

      <div className="p-3 border-t border-sidebar-border flex items-center gap-2">
        <Avatar
          url={props.user.avatar_url}
          username={props.user.display_name ?? props.user.username}
          size={24}
        />
        <span className="text-sidebar-foreground text-sm truncate flex-1 min-w-0">
          {props.user.display_name ?? props.user.username}
        </span>
        <VoiceStatusControls />
        <button
          type="button"
          aria-label="Settings"
          className="text-sidebar-foreground/70 hover:text-sidebar-foreground flex-shrink-0 p-1 rounded-md hover:bg-sidebar-accent transition-colors"
          onClick={() => setSettingsOpen(true)}
        >
          <SettingsIcon size={18} aria-hidden="true" />
        </button>
      </div>

      <AddChannelModal open={addChannelOpen} onClose={() => setAddChannelOpen(false)} />
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onLogout={props.onLogout}
        user={props.user}
        onAvatarChange={props.onAvatarChange}
      />
    </div>
  );
}
