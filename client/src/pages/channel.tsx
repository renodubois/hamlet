import { useParams } from "@solidjs/router";
import { createResource, createSignal, onCleanup, onMount } from "solid-js";
import ChannelMessages from "../components/channel_messages";
import { listMessages, sendMessage } from "../api";
import { useChannels } from "../channels_context";
import { useEvents } from "../events_context";
import { useAuth } from "../auth_context";

export default function ChannelView() {
  const params = useParams<{ id: string }>();
  const { channels } = useChannels();
  const events = useEvents();
  const { user } = useAuth();
  const channel = () => channels()?.find((c) => String(c.id) === params.id);
  const [message, setMessage] = createSignal("");
  const [messages, { mutate }] = createResource(() => params.id, listMessages);

  onMount(() => {
    const unsubCreated = events.onMessage((m) => {
      if (String(m.channel_id) !== params.id) return;
      mutate((prev) => [...(prev ?? []), m]);
    });
    const unsubUpdated = events.onMessageUpdated((m) => {
      if (String(m.channel_id) !== params.id) return;
      mutate((prev) => prev?.map((existing) => (existing.id === m.id ? m : existing)));
    });
    const unsubDeleted = events.onMessageDeleted((d) => {
      if (String(d.channel_id) !== params.id) return;
      mutate((prev) => prev?.filter((existing) => existing.id !== d.id));
    });
    onCleanup(() => {
      unsubCreated();
      unsubUpdated();
      unsubDeleted();
    });
  });

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  return (
    <div class="flex flex-col h-full">
      <section class="bg-gray-100 text-gray-700 p-4 flex-shrink-0">
        <h1 class="text-2xl font-bold"># {channel()?.name ?? params.id}</h1>
      </section>

      <div class="flex-1 overflow-y-auto">
        <ChannelMessages messages={messages} currentUserId={user()?.id ?? null} />
      </div>

      <section class="flex-shrink-0 p-4 border-t border-gray-200">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void sendMessage(params.id, message());
            setMessage("");
          }}
        >
          <div class="flex">
            <input
              class="bg-gray-100 rounded-md p-4 w-full"
              value={message()}
              onInput={(e) => setMessage(e.currentTarget.value)}
              placeholder="Send a new message..."
            />
            <input class="bg-blue-100 ml-2 p-4 rounded-md" type="button" value="Send" />
          </div>
        </form>
      </section>
    </div>
  );
}
