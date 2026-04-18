import type { Channel, Message, SSEEvent } from "../../api";

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

  pushChannelCreated(channel: Channel) {
    this.push({ kind: "channel_created", data: channel });
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
