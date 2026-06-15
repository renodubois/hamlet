import type {
  Channel,
  Message,
  MessageDeleted,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  ScreenShareStopped,
  ScreenShareStream,
  SSEEvent,
  ThreadReplyCreated,
  ThreadReplyDeleted,
  UserTyping,
} from "../../api";

type Listener = (ev: MessageEvent<string>) => void;

export class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onmessage: Listener | null = null;
  onerror: ((ev: Event) => void) | null = null;
  closed = false;
  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  push(event: SSEEvent) {
    if (this.closed || !this.onmessage) return;
    this.onmessage(new MessageEvent("message", { data: JSON.stringify(event) }));
  }

  pushMessage(msg: Message) {
    this.push({ kind: "message", data: msg });
  }

  pushMessageUpdated(msg: Message) {
    this.push({ kind: "message_updated", data: msg });
  }

  pushMessageDeleted(data: MessageDeleted) {
    this.push({ kind: "message_deleted", data });
  }

  pushMessageEmbedsUpdated(data: MessageEmbedsUpdated) {
    this.push({ kind: "message_embeds_updated", data });
  }

  pushMessageReactionsUpdated(data: MessageReactionsUpdated) {
    this.push({ kind: "message_reactions_updated", data });
  }

  pushChannelCreated(channel: Channel) {
    this.push({ kind: "channel_created", data: channel });
  }

  pushScreenShareStarted(data: ScreenShareStream) {
    this.push({ kind: "screen_share_started", data });
  }

  pushScreenShareStopped(data: ScreenShareStopped) {
    this.push({ kind: "screen_share_stopped", data });
  }

  pushUserTyping(data: UserTyping) {
    this.push({ kind: "user_typing", data });
  }

  pushThreadReplyCreated(data: ThreadReplyCreated) {
    this.push({ kind: "thread_reply_created", data });
  }

  pushThreadReplyDeleted(data: ThreadReplyDeleted) {
    this.push({ kind: "thread_reply_deleted", data });
  }

  close() {
    this.closed = true;
  }
}

export function latestFakeEventSource(): FakeEventSource | undefined {
  return FakeEventSource.instances[FakeEventSource.instances.length - 1];
}

export function resetFakeEventSources() {
  FakeEventSource.instances = [];
}
