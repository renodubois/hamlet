import { useParams } from "@solidjs/router";
import { createResource, createSignal, onCleanup, onMount } from "solid-js";
import ChannelMessages from "../components/channel_messages";
import { listMessages, messagesEventSource, sendMessage } from "../api";

export default function ChannelView() {
  const params = useParams();
  const [message, setMessage] = createSignal("");
  const [eventSource, setEventSource] = createSignal<EventSource | null>(null);
  // TODO(reno): This needs to be typed
  const [messages, { mutate }] = createResource(() => params.id, listMessages);
  onMount(() => {
    const es = messagesEventSource();
    es.onmessage = (m) => {
      if (m.data !== "connected") {
        mutate((prev) => [...(prev ?? []), { text: m.data }]);
      }
    };
    setEventSource(es);
  });
  onCleanup(() => {
    eventSource()?.close();
  });

  // TODO(reno): Can the router ensure this parameter exists?
  if (!params.id) {
    return <div>Error: channel required</div>;
  }

  return (
    <div class="flex flex-col h-full">
      <section class="bg-gray-100 text-gray-700 p-4 flex-shrink-0">
        <h1 class="text-2xl font-bold">Channel {params.id}</h1>
      </section>

      <div class="flex-1 overflow-y-auto">
        <ChannelMessages messages={messages} />
      </div>

      <section class="flex-shrink-0 p-4 border-t border-gray-200">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            sendMessage(params.id!, message());
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
