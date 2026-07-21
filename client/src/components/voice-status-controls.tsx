import { useStaticSignalRerender } from "../hooks/react-state";
import { useVoiceChat } from "../contexts/voice-chat";
import { HeadphoneOffIcon, HeadphonesIcon, MicIcon, MicOffIcon, PhoneOffIcon } from "./icons";

export default function VoiceStatusControls() {
  useStaticSignalRerender();
  const voice = useVoiceChat();

  return (
    <div className="flex items-center gap-1" role="group" aria-label="Voice status controls">
      <button
        type="button"
        className={`p-1 rounded-md transition-colors hover:bg-sidebar-accent ${
          voice.isMuted()
            ? "text-destructive bg-sidebar-accent"
            : "text-sidebar-foreground/70 hover:text-sidebar-foreground"
        }`}
        aria-pressed={voice.isMuted()}
        aria-label={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        title={voice.isMuted() ? "Unmute microphone" : "Mute microphone"}
        onClick={() => void voice.toggleMuted()}
      >
        {voice.isMuted() ? (
          <MicOffIcon size={16} aria-hidden="true" />
        ) : (
          <MicIcon size={16} aria-hidden="true" />
        )}
      </button>
      <button
        type="button"
        className={`p-1 rounded-md transition-colors hover:bg-sidebar-accent ${
          voice.isDeafened()
            ? "text-destructive bg-sidebar-accent"
            : "text-sidebar-foreground/70 hover:text-sidebar-foreground"
        }`}
        aria-pressed={voice.isDeafened()}
        aria-label={voice.isDeafened() ? "Undeafen" : "Deafen"}
        title={voice.isDeafened() ? "Undeafen" : "Deafen"}
        onClick={() => void voice.toggleDeafened()}
      >
        {voice.isDeafened() ? (
          <HeadphoneOffIcon size={16} aria-hidden="true" />
        ) : (
          <HeadphonesIcon size={16} aria-hidden="true" />
        )}
      </button>
      {voice.activeChannelId() != null ? (
        <button
          type="button"
          className="p-1 rounded-md transition-colors hover:bg-sidebar-accent text-destructive hover:text-destructive/80"
          aria-label="Disconnect from voice"
          title="Disconnect from voice"
          onClick={() => void voice.leave()}
        >
          <PhoneOffIcon size={16} aria-hidden="true" />
        </button>
      ) : null}
    </div>
  );
}
