import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  type ReactNode,
} from "react";
import {
  loadVoicePreferences,
  saveVoicePreferences,
  type VoicePreferencesSnapshot,
  type VoicePreferencesStorage,
} from "../voice/settings";

export interface VoicePreferencesContextValue extends VoicePreferencesSnapshot {
  /** Stable immutable object for VoiceSession.applyPreferences. */
  readonly snapshot: VoicePreferencesSnapshot;
  readonly setInputDeviceId: (deviceId: string) => void;
  readonly setOutputDeviceId: (deviceId: string) => void;
  readonly setCameraDeviceId: (deviceId: string) => void;
  readonly setNoiseSuppression: (enabled: boolean) => void;
  readonly setInputGain: (gain: number) => void;
  readonly setShowSpeakingEverywhere: (enabled: boolean) => void;
}

type PreferenceAction =
  | { readonly type: "inputDeviceId"; readonly value: string }
  | { readonly type: "outputDeviceId"; readonly value: string }
  | { readonly type: "cameraDeviceId"; readonly value: string }
  | { readonly type: "noiseSuppression"; readonly value: boolean }
  | { readonly type: "inputGain"; readonly value: number }
  | { readonly type: "showSpeakingEverywhere"; readonly value: boolean };

function preferencesReducer(
  state: VoicePreferencesSnapshot,
  action: PreferenceAction,
): VoicePreferencesSnapshot {
  if (action.type === "inputGain") {
    const value = Number.isFinite(action.value) ? Math.min(2, Math.max(0, action.value)) : 1;
    return value === state.inputGain ? state : { ...state, inputGain: value };
  }
  if (state[action.type] === action.value) return state;
  return { ...state, [action.type]: action.value };
}

const VoicePreferencesContext = createContext<VoicePreferencesContextValue | undefined>(undefined);

export function VoicePreferencesProvider(props: {
  children: ReactNode;
  /** Injectable for tests and non-browser hosts. Normal application code omits this. */
  storage?: VoicePreferencesStorage;
}) {
  const [snapshot, dispatch] = useReducer(preferencesReducer, props.storage, loadVoicePreferences);

  useEffect(() => {
    saveVoicePreferences(snapshot, props.storage);
  }, [props.storage, snapshot]);

  const setInputDeviceId = useCallback((value: string) => {
    dispatch({ type: "inputDeviceId", value });
  }, []);
  const setOutputDeviceId = useCallback((value: string) => {
    dispatch({ type: "outputDeviceId", value });
  }, []);
  const setCameraDeviceId = useCallback((value: string) => {
    dispatch({ type: "cameraDeviceId", value });
  }, []);
  const setNoiseSuppression = useCallback((value: boolean) => {
    dispatch({ type: "noiseSuppression", value });
  }, []);
  const setInputGain = useCallback((value: number) => {
    dispatch({ type: "inputGain", value });
  }, []);
  const setShowSpeakingEverywhere = useCallback((value: boolean) => {
    dispatch({ type: "showSpeakingEverywhere", value });
  }, []);

  const contextValue = useMemo<VoicePreferencesContextValue>(
    () => ({
      ...snapshot,
      snapshot,
      setInputDeviceId,
      setOutputDeviceId,
      setCameraDeviceId,
      setNoiseSuppression,
      setInputGain,
      setShowSpeakingEverywhere,
    }),
    [
      setCameraDeviceId,
      setInputDeviceId,
      setInputGain,
      setNoiseSuppression,
      setOutputDeviceId,
      setShowSpeakingEverywhere,
      snapshot,
    ],
  );

  return (
    <VoicePreferencesContext.Provider value={contextValue}>
      {props.children}
    </VoicePreferencesContext.Provider>
  );
}

export function useVoicePreferences(): VoicePreferencesContextValue {
  const context = useContext(VoicePreferencesContext);
  if (!context) {
    throw new Error("useVoicePreferences must be used inside VoicePreferencesProvider");
  }
  return context;
}
