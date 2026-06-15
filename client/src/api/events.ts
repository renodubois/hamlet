import { getServerUrl } from "./client";
import type { Channel } from "./channels";
import type { CustomEmoji } from "./emojis";
import type {
  Message,
  MessageDeleted,
  MessageEmbedsUpdated,
  MessageReactionsUpdated,
  ThreadReplyCreated,
  ThreadReplyDeleted,
} from "./messages";
import type {
  ScreenShareStopped,
  ScreenShareStream,
  VoiceParticipant,
  VoiceParticipantLeft,
  VoiceParticipantSpeaking,
} from "./voice";
import type { UserTyping } from "./typing";

export type SSEEvent =
  | { kind: "message"; data: Message }
  | { kind: "message_updated"; data: Message }
  | { kind: "message_deleted"; data: MessageDeleted }
  | { kind: "message_embeds_updated"; data: MessageEmbedsUpdated }
  | { kind: "message_reactions_updated"; data: MessageReactionsUpdated }
  | { kind: "channel_created"; data: Channel }
  | { kind: "channels_reordered"; data: Channel[] }
  | { kind: "emoji_created"; data: CustomEmoji }
  | { kind: "emoji_updated"; data: CustomEmoji }
  | { kind: "emoji_deleted"; data: CustomEmoji }
  | { kind: "voice_participant_joined"; data: VoiceParticipant }
  | { kind: "voice_participant_left"; data: VoiceParticipantLeft }
  | { kind: "voice_participant_speaking_changed"; data: VoiceParticipantSpeaking }
  | { kind: "screen_share_started"; data: ScreenShareStream }
  | { kind: "screen_share_stopped"; data: ScreenShareStopped }
  | { kind: "user_typing"; data: UserTyping }
  | { kind: "thread_reply_created"; data: ThreadReplyCreated }
  | { kind: "thread_reply_deleted"; data: ThreadReplyDeleted };

export function messagesEventSource(): EventSource {
  return new EventSource(`${getServerUrl()}/messages/subscribe`, {
    withCredentials: true,
  });
}
