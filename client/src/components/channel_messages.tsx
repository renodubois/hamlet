import { Component, For, Match, Show, Switch } from "solid-js";

const ChannelMessages: Component<{ messages: any }> = (props) => {
  let messages = props.messages;
  return (
    <section class="p-8">
      <Show when={messages.loading}>
        <p>Loading...</p>
      </Show>
      <Switch>
        <Match when={messages.error}>
          <span>Error getting messages: {messages.error}</span>
        </Match>
        <Match when={messages()}>
          <For each={messages()}>{(message) => <div>{message.text}</div>}</For>
        </Match>
      </Switch>
    </section>
  );
};

export default ChannelMessages;
