import {
  Component,
  For,
  Match,
  Resource,
  Show,
  Switch,
  createSignal,
  onCleanup,
  onMount,
} from "solid-js";
import { editMessage, type Message } from "../api";
import Avatar from "./avatar";

interface ContextMenuState {
  messageId: number;
  x: number;
  y: number;
}

const ChannelMessages: Component<{
  messages: Resource<Message[]>;
  currentUserId: number | null;
}> = (props) => {
  let messages = props.messages;
  const [contextMenu, setContextMenu] = createSignal<ContextMenuState | null>(null);
  const [editingId, setEditingId] = createSignal<number | null>(null);
  const [draft, setDraft] = createSignal("");

  const closeMenu = () => setContextMenu(null);

  onMount(() => {
    const onDocClick = () => closeMenu();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeMenu();
    };
    document.addEventListener("click", onDocClick);
    document.addEventListener("keydown", onKey);
    onCleanup(() => {
      document.removeEventListener("click", onDocClick);
      document.removeEventListener("keydown", onKey);
    });
  });

  const startEditing = (msg: Message) => {
    setEditingId(msg.id);
    setDraft(msg.text);
    closeMenu();
  };

  const cancelEditing = () => {
    setEditingId(null);
    setDraft("");
  };

  const saveEdit = async (msg: Message) => {
    const next = draft();
    if (next === msg.text || next.length === 0) {
      cancelEditing();
      return;
    }
    try {
      await editMessage(msg.id, next);
    } catch (e) {
      console.error("failed to edit message", e);
    }
    cancelEditing();
  };

  const handleContextMenu = (e: MouseEvent, msg: Message) => {
    if (props.currentUserId === null || msg.user_id !== props.currentUserId) return;
    e.preventDefault();
    setContextMenu({ messageId: msg.id, x: e.clientX, y: e.clientY });
  };

  return (
    <section class="p-8 min-h-full flex flex-col justify-end">
      <Show when={messages.loading}>
        <p>Loading...</p>
      </Show>
      <Switch>
        <Match when={messages.error}>
          <span>Error getting messages: {messages.error}</span>
        </Match>
        <Match when={messages()}>
          <For each={messages()}>
            {(message) => (
              <div
                class="flex items-start gap-3 mb-2"
                onContextMenu={(e) => handleContextMenu(e, message)}
              >
                <Avatar url={message.avatar_url} username={message.username} size={32} />
                <div class="min-w-0 flex-1">
                  <span class="font-bold mr-2">{message.username}</span>
                  <Show when={editingId() === message.id} fallback={<span>{message.text}</span>}>
                    <form
                      class="inline-flex gap-2 items-center"
                      onSubmit={(e) => {
                        e.preventDefault();
                        void saveEdit(message);
                      }}
                    >
                      <input
                        aria-label="Edit message"
                        class="bg-gray-100 rounded-md px-2 py-1"
                        value={draft()}
                        onInput={(e) => setDraft(e.currentTarget.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Escape") {
                            e.preventDefault();
                            cancelEditing();
                          }
                        }}
                        ref={(el) => queueMicrotask(() => el.focus())}
                      />
                      <button type="submit" class="text-sm text-blue-600">
                        Save
                      </button>
                      <button type="button" class="text-sm text-gray-500" onClick={cancelEditing}>
                        Cancel
                      </button>
                    </form>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </Match>
      </Switch>
      <Show when={contextMenu()}>
        {(menu) => (
          <ul
            role="menu"
            class="fixed z-50 bg-white border border-gray-200 rounded-md shadow-md py-1"
            style={{ top: `${menu().y}px`, left: `${menu().x}px` }}
            onClick={(e) => e.stopPropagation()}
          >
            <li>
              <button
                role="menuitem"
                type="button"
                class="w-full text-left px-4 py-1 hover:bg-gray-100"
                onClick={() => {
                  const id = menu().messageId;
                  const msg = messages()?.find((m) => m.id === id);
                  if (msg) startEditing(msg);
                }}
              >
                Edit message
              </button>
            </li>
          </ul>
        )}
      </Show>
    </section>
  );
};

export default ChannelMessages;
