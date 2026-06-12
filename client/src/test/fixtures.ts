import type { Message, MessageAttachment } from "../api";

/// Build a `Message` fixture with media/embed fields filled in with sensible
/// defaults. Every test message has `suppress_embeds: false`, no attachments,
/// and no embeds unless the caller overrides them, so tests that don't care
/// about media features stay compact.
export function makeMessage(
  partial: Partial<Message> & Pick<Message, "id" | "user_id" | "channel_id" | "text" | "username">,
): Message {
  return {
    display_name: null,
    avatar_url: null,
    parent_id: null,
    suppress_embeds: false,
    attachments: [],
    embeds: [],
    reactions: [],
    ...partial,
  };
}

export function makeAttachment(
  partial: Partial<MessageAttachment> & Pick<MessageAttachment, "id" | "message_id">,
): MessageAttachment {
  const { id, message_id, ...overrides } = partial;
  return {
    id,
    message_id,
    position: 0,
    content_type: "image/jpeg",
    byte_size: 1_024_000,
    width: 1600,
    height: 1200,
    url: `/attachments/${id}`,
    thumbnail_url: `/attachments/${id}/thumbnail`,
    thumbnail_content_type: "image/webp",
    thumbnail_byte_size: 24_000,
    thumbnail_width: 400,
    thumbnail_height: 300,
    ...overrides,
  };
}
