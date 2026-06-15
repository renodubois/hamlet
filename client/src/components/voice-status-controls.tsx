import { Show } from "solid-js";
import { useVoiceChat } from "../contexts/voice-chat";
import { HeadphoneOffIcon, HeadphonesIcon, MicIcon, MicOffIcon, PhoneOffIcon } from "./icons";

export default function VoiceStatusControls() {
  const voice = useVoiceChat();

  return (
    <div class="flex items-center gap-1" role="group" aria-label="Voice status controls">
      <button
        type="button"
        class="p-1 rounded hover:bg-gray-700"
        classList={{
          "text-red-400 bg-gray-700": voice.isMuted(),
          "text-gray-400 hover:text-gray-100": !voice.isMuted(),
        }}
        aria-pressed={voice.isMuted()}
        aria-label={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        title={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        onClick={() => void voice.toggleMuted()}
      >
        <Show when={voice.isMuted()} fallback={<MicIcon size={16} aria-hidden="true" />}>
          <MicOffIcon size={16} aria-hidden="true" />
        </Show>
      </button>
      <button
        type="button"
        class="p-1 rounded hover:bg-gray-700"
        classList={{
          "text-red-400 bg-gray-700": voice.isDeafened(),
          "text-gray-400 hover:text-gray-100": !voice.isDeafened(),
        }}
        aria-pressed={voice.isDeafened()}
        aria-label={voice.isDeafened() ? "Undeafen" : "Deafen"}
        title={voice.isDeafened() ? "Undeafen" : "Deafen"}
        onClick={() => void voice.toggleDeafened()}
      >
        <Show when={voice.isDeafened()} fallback={<HeadphonesIcon size={16} aria-hidden="true" />}>
          <HeadphoneOffIcon size={16} aria-hidden="true" />
        </Show>
      </button>
      <Show when={voice.activeChannelId() != null}>
        <button
          type="button"
          class="p-1 rounded hover:bg-gray-700 text-red-400 hover:text-red-300"
          aria-label="Disconnect from voice"
          title="Disconnect from voice"
          onClick={() => void voice.leave()}
        >
          <PhoneOffIcon size={16} aria-hidden="true" />
        </button>
      </Show>
    </div>
  );
}
