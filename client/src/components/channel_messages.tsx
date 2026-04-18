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
import { deleteMessage, editMessage, type Message } from "../api";
import Avatar from "./avatar";
import Modal from "./modal";

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
  const [pendingDeleteId, setPendingDeleteId] = createSignal<number | null>(null);

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

  const requestDelete = (id: number) => {
    setPendingDeleteId(id);
    closeMenu();
  };

  const cancelDelete = () => {
    setPendingDeleteId(null);
  };

  const confirmDelete = async () => {
    const id = pendingDeleteId();
    if (id === null) return;
    try {
      await deleteMessage(id);
    } catch (e) {
      console.error("failed to delete message", e);
    }
    setPendingDeleteId(null);
    if (editingId() === id) cancelEditing();
  };

  const saveEdit = async (msg: Message) => {
    const next = draft();
    if (next.length === 0) {
      requestDelete(msg.id);
      return;
    }
    if (next === msg.text) {
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
            <li>
              <button
                role="menuitem"
                type="button"
                class="w-full text-left px-4 py-1 text-red-600 hover:bg-red-50"
                onClick={() => requestDelete(menu().messageId)}
              >
                Delete message
              </button>
            </li>
          </ul>
        )}
      </Show>
      <Modal open={pendingDeleteId() !== null} onClose={cancelDelete} title="Delete message?">
        <p class="text-sm text-gray-200 mb-4">
          This will permanently delete the message. This cannot be undone.
        </p>
        <div class="flex gap-2 justify-end">
          <button
            type="button"
            class="text-gray-300 hover:text-gray-100 text-sm px-3 py-2"
            onClick={cancelDelete}
          >
            Cancel
          </button>
          <button
            type="button"
            class="bg-red-600 hover:bg-red-700 text-white rounded-md px-4 py-2 text-sm font-medium transition-colors"
            onClick={() => void confirmDelete()}
          >
            Delete
          </button>
        </div>
      </Modal>
    </section>
  );
};

export default ChannelMessages;
