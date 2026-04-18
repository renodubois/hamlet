import { Component, For, Match, Resource, Show, Switch } from "solid-js";
import { type Message } from "../api";
import Avatar from "./avatar";

const ChannelMessages: Component<{ messages: Resource<Message[]> }> = (props) => {
  let messages = props.messages;
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
              <div class="flex items-start gap-3 mb-2">
                <Avatar url={message.avatar_url} username={message.username} size={32} />
                <div class="min-w-0 flex-1">
                  <span class="font-bold mr-2">{message.username}</span>
                  <span>{message.text}</span>
                </div>
              </div>
            )}
          </For>
        </Match>
      </Switch>
    </section>
  );
};

export default ChannelMessages;
