import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { listVoiceParticipants, type Channel, type VoiceParticipant } from "../api";
import { useEvents } from "../contexts/events";
import { useVoiceChat } from "../contexts/voice_chat";
import Avatar from "./avatar";
import { showSpeakingIndicatorsEverywhere } from "../voice/settings";
import {
  HeadphoneOffIcon,
  HeadphonesIcon,
  MicIcon,
  MicOffIcon,
  PhoneOffIcon,
  VoiceChannelIcon,
} from "./icons";

/**
 * Everything the sidebar needs to render a single voice channel: the channel
 * row itself, the live participant list beneath it, and (when this is the
 * channel we're connected to) mute/deafen/disconnect controls.
 *
 * Participant state is fetched once on mount and kept current by listening for
 * `voice_participant_joined`/`voice_participant_left` SSE events — the server
 * is the source of truth, driven by LiveKit webhooks.
 */
export default function VoiceChannel(props: { channel: Channel }) {
  const events = useEvents();
  const voice = useVoiceChat();
  const [participants, setParticipants] = createSignal<VoiceParticipant[]>([]);
  const [localError, setLocalError] = createSignal<string | null>(null);
  // Speakers known from SSE broadcasts — used to render the ring when we're
  // NOT connected to this channel. In-channel speakers come from LiveKit via
  // voice.speakingUserIds() instead.
  const [remoteSpeakers, setRemoteSpeakers] = createSignal<ReadonlySet<number>>(new Set());

  const isActive = () => voice.activeChannelId() === props.channel.id;
  const isBusy = () => voice.isConnecting();

  const speakingIds = createMemo<ReadonlySet<number>>(() => {
    if (isActive()) return voice.speakingUserIds();
    if (showSpeakingIndicatorsEverywhere()) return remoteSpeakers();
    return new Set();
  });

  async function handleToggleJoin() {
    setLocalError(null);
    try {
      if (isActive()) {
        await voice.leave();
      } else {
        await voice.join(props.channel.id);
      }
    } catch (e) {
      setLocalError(e instanceof Error ? e.message : "Could not connect");
    }
  }

  onMount(() => {
    void listVoiceParticipants(props.channel.id)
      .then(setParticipants)
      .catch(() => {
        // Server may not have voice configured (503) — the sidebar still works.
      });

    const unsubJoined = events.onVoiceParticipantJoined((p) => {
      if (p.channel_id !== props.channel.id) return;
      setParticipants((prev) => {
        if (prev.some((x) => x.user_id === p.user_id)) return prev;
        return [...prev, p];
      });
    });
    const unsubLeft = events.onVoiceParticipantLeft((p) => {
      if (p.channel_id !== props.channel.id) return;
      setParticipants((prev) => prev.filter((x) => x.user_id !== p.user_id));
      setRemoteSpeakers((prev) => {
        if (!prev.has(p.user_id)) return prev;
        const next = new Set(prev);
        next.delete(p.user_id);
        return next;
      });
    });
    const unsubSpeaking = events.onVoiceParticipantSpeakingChanged((s) => {
      if (s.channel_id !== props.channel.id) return;
      setRemoteSpeakers((prev) => {
        const has = prev.has(s.user_id);
        if (s.speaking === has) return prev;
        const next = new Set(prev);
        if (s.speaking) next.add(s.user_id);
        else next.delete(s.user_id);
        return next;
      });
    });

    onCleanup(() => {
      unsubJoined();
      unsubLeft();
      unsubSpeaking();
    });
  });

  return (
    <div class="flex flex-col">
      <button
        type="button"
        class="w-full flex items-center gap-2 px-3 py-1.5 rounded text-sm text-left disabled:opacity-50"
        classList={{
          "bg-gray-700 text-white font-medium": isActive(),
          "text-gray-400 hover:bg-gray-700 hover:text-gray-200": !isActive(),
        }}
        onClick={() => void handleToggleJoin()}
        disabled={isBusy()}
        aria-pressed={isActive()}
        aria-label={
          isActive()
            ? `Leave voice channel ${props.channel.name}`
            : `Join voice channel ${props.channel.name}`
        }
        draggable={false}
      >
        <VoiceChannelIcon size={14} aria-hidden="true" />
        <span class="flex-1 truncate">{props.channel.name}</span>
        <Show when={isBusy() && voice.activeChannelId() !== props.channel.id}>
          <span class="text-xs text-gray-400" aria-hidden="true">
            …
          </span>
        </Show>
      </button>

      <Show when={participants().length > 0}>
        <ul
          class="ml-6 mt-1 flex flex-col gap-0.5"
          aria-label={`Participants in ${props.channel.name}`}
        >
          <For each={participants()}>
            {(p) => (
              <li class="flex items-center gap-2 px-2 py-1 text-xs text-gray-300">
                <Avatar
                  url={p.avatar_url}
                  username={p.username}
                  size={18}
                  isSpeaking={speakingIds().has(p.user_id)}
                />
                <span class="truncate">{p.username}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show when={isActive()}>
        <div
          class="ml-6 mt-1 mb-1 flex items-center gap-1"
          role="group"
          aria-label="Voice controls"
        >
          <button
            type="button"
            class="p-1.5 rounded hover:bg-gray-700"
            classList={{
              "text-red-400 bg-gray-700": voice.isMuted(),
              "text-gray-300": !voice.isMuted(),
            }}
            aria-pressed={voice.isMuted()}
            aria-label={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
            onClick={() => void voice.toggleMuted()}
          >
            <Show when={voice.isMuted()} fallback={<MicIcon size={14} aria-hidden="true" />}>
              <MicOffIcon size={14} aria-hidden="true" />
            </Show>
          </button>
          <button
            type="button"
            class="p-1.5 rounded hover:bg-gray-700"
            classList={{
              "text-red-400 bg-gray-700": voice.isDeafened(),
              "text-gray-300": !voice.isDeafened(),
            }}
            aria-pressed={voice.isDeafened()}
            aria-label={voice.isDeafened() ? "Undeafen" : "Deafen"}
            onClick={voice.toggleDeafened}
          >
            <Show
              when={voice.isDeafened()}
              fallback={<HeadphonesIcon size={14} aria-hidden="true" />}
            >
              <HeadphoneOffIcon size={14} aria-hidden="true" />
            </Show>
          </button>
          <button
            type="button"
            class="p-1.5 rounded hover:bg-gray-700 text-red-400 ml-auto"
            aria-label="Disconnect from voice"
            onClick={() => void voice.leave()}
          >
            <PhoneOffIcon size={14} aria-hidden="true" />
          </button>
        </div>
      </Show>

      <Show when={localError() || voice.lastError()}>
        <p class="ml-6 mb-1 text-xs text-red-400" role="alert">
          {localError() ?? voice.lastError()}
        </p>
      </Show>
    </div>
  );
}
