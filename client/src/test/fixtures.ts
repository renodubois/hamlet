import type { Message } from "../api";

/// Build a `Message` fixture with embed fields filled in with sensible
/// defaults. Every test message has `suppress_embeds: false` and no embeds
/// unless the caller overrides them, so tests that don't care about the
/// embed feature stay compact.
export function makeMessage(
  partial: Partial<Message> & Pick<Message, "id" | "user_id" | "channel_id" | "text" | "username">,
): Message {
  return {
    display_name: null,
    avatar_url: null,
    parent_id: null,
    suppress_embeds: false,
    embeds: [],
    ...partial,
  };
}
