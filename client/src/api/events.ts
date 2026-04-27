import { getServerUrl } from "./client";
import type { Channel } from "./channels";
import type { Message, MessageDeleted, MessageEmbedsUpdated } from "./messages";
import type { VoiceParticipant, VoiceParticipantLeft, VoiceParticipantSpeaking } from "./voice";
import type { UserTyping } from "./typing";

export type SSEEvent =
  | { kind: "message"; data: Message }
  | { kind: "message_updated"; data: Message }
  | { kind: "message_deleted"; data: MessageDeleted }
  | { kind: "message_embeds_updated"; data: MessageEmbedsUpdated }
  | { kind: "channel_created"; data: Channel }
  | { kind: "channels_reordered"; data: Channel[] }
  | { kind: "voice_participant_joined"; data: VoiceParticipant }
  | { kind: "voice_participant_left"; data: VoiceParticipantLeft }
  | { kind: "voice_participant_speaking_changed"; data: VoiceParticipantSpeaking }
  | { kind: "user_typing"; data: UserTyping };

export function messagesEventSource(): EventSource {
  return new EventSource(`${getServerUrl()}/messages/subscribe`, {
    withCredentials: true,
  });
}
