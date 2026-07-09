import { If, useStaticSignalRerender } from "../hooks/react-state";
import { useVoiceChat } from "../contexts/voice-chat";
import { HeadphoneOffIcon, HeadphonesIcon, MicIcon, MicOffIcon, PhoneOffIcon } from "./icons";

export default function VoiceStatusControls() {
  useStaticSignalRerender();
  const voice = useVoiceChat();

  return (
    <div className="flex items-center gap-1" role="group" aria-label="Voice status controls">
      <button
        type="button"
        className={`p-1 rounded hover:bg-gray-700 ${
          voice.isMuted() ? "text-red-400 bg-gray-700" : "text-gray-400 hover:text-gray-100"
        }`}
        aria-pressed={voice.isMuted()}
        aria-label={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        title={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        onClick={() => void voice.toggleMuted()}
      >
        <If when={voice.isMuted()} fallback={<MicIcon size={16} aria-hidden="true" />}>
          <MicOffIcon size={16} aria-hidden="true" />
        </If>
      </button>
      <button
        type="button"
        className={`p-1 rounded hover:bg-gray-700 ${
          voice.isDeafened() ? "text-red-400 bg-gray-700" : "text-gray-400 hover:text-gray-100"
        }`}
        aria-pressed={voice.isDeafened()}
        aria-label={voice.isDeafened() ? "Undeafen" : "Deafen"}
        title={voice.isDeafened() ? "Undeafen" : "Deafen"}
        onClick={() => void voice.toggleDeafened()}
      >
        <If when={voice.isDeafened()} fallback={<HeadphonesIcon size={16} aria-hidden="true" />}>
          <HeadphoneOffIcon size={16} aria-hidden="true" />
        </If>
      </button>
      <If when={voice.activeChannelId() != null}>
        <button
          type="button"
          className="p-1 rounded hover:bg-gray-700 text-red-400 hover:text-red-300"
          aria-label="Disconnect from voice"
          title="Disconnect from voice"
          onClick={() => void voice.leave()}
        >
          <PhoneOffIcon size={16} aria-hidden="true" />
        </button>
      </If>
    </div>
  );
}
