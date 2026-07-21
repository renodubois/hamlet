import { useCallback, useEffect, useRef, useState } from "react";

import { useVoicePreferences } from "../contexts/voice-preferences";
import { Button } from "./ui/button";
import { Label } from "./ui/label";

// "" means "let the browser pick the system default".
const DEFAULT_DEVICE_ID = "";

type NormalizedMediaDevice = {
  device: MediaDeviceInfo;
  key: string;
};

type OutputTestResources = {
  context: AudioContext;
  audio: HTMLAudioElement | null;
  destinationStream: MediaStream | null;
  timer: ReturnType<typeof setTimeout> | null;
  finishDelay: (() => void) | null;
  disposed: boolean;
};

function disposeOutputTestResources(resources: OutputTestResources) {
  if (resources.disposed) return;
  resources.disposed = true;
  if (resources.timer != null) clearTimeout(resources.timer);
  resources.timer = null;
  resources.finishDelay?.();
  resources.finishDelay = null;
  resources.audio?.pause();
  if (resources.audio) resources.audio.srcObject = null;
  resources.destinationStream?.getTracks().forEach((track) => track.stop());
  void resources.context.close();
}

function sinkIdSupported(): boolean {
  return typeof HTMLAudioElement !== "undefined" && "setSinkId" in HTMLAudioElement.prototype;
}

function mediaDevicesSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices &&
    typeof navigator.mediaDevices.enumerateDevices === "function" &&
    typeof navigator.mediaDevices.getUserMedia === "function"
  );
}

export default function VoiceSettings() {
  const supported = mediaDevicesSupported();
  const canPickOutput = sinkIdSupported();
  const preferences = useVoicePreferences();

  const [inputDevices, setInputDevices] = useState<NormalizedMediaDevice[]>([]);
  const [outputDevices, setOutputDevices] = useState<NormalizedMediaDevice[]>([]);
  const [cameraDevices, setCameraDevices] = useState<NormalizedMediaDevice[]>([]);
  const [isLoading, setIsLoading] = useState(supported);
  const [micTesting, setMicTesting] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [micError, setMicError] = useState<string | null>(null);
  const [outputError, setOutputError] = useState<string | null>(null);
  const [playingTestSound, setPlayingTestSound] = useState(false);
  const [cameraPreviewStream, setCameraPreviewStream] = useState<MediaStream | null>(null);
  const [cameraPreviewStarting, setCameraPreviewStarting] = useState(false);
  const [cameraError, setCameraError] = useState<string | null>(null);

  const micStreamRef = useRef<MediaStream | null>(null);
  const micContextRef = useRef<AudioContext | null>(null);
  const micRafRef = useRef<number | null>(null);
  const micRequestGenerationRef = useRef(0);
  const cameraStreamRef = useRef<MediaStream | null>(null);
  const cameraRequestGenerationRef = useRef(0);
  const outputResourcesRef = useRef<OutputTestResources | null>(null);
  const outputRequestGenerationRef = useRef(0);
  const lifecycleGenerationRef = useRef(0);
  const cameraVideoRef = useRef<HTMLVideoElement | null>(null);
  const primePromiseRef = useRef<Promise<void> | null>(null);
  const syntheticDeviceKeysByObjectRef = useRef(new WeakMap<MediaDeviceInfo, string>());
  const syntheticDeviceKeysBySignatureRef = useRef(new Map<string, string[]>());
  const nextSyntheticDeviceKeyRef = useRef(0);

  const normalizeDevices = useCallback((devices: readonly MediaDeviceInfo[]) => {
    const signatureOccurrences = new Map<string, number>();
    return devices.map((device): NormalizedMediaDevice => {
      if (device.deviceId) return { device, key: `${device.kind}:${device.deviceId}` };
      const signature = `${device.kind}:${device.groupId}:${device.label}`;
      const occurrence = signatureOccurrences.get(signature) ?? 0;
      signatureOccurrences.set(signature, occurrence + 1);
      let key = syntheticDeviceKeysByObjectRef.current.get(device);
      if (!key) {
        const keys = syntheticDeviceKeysBySignatureRef.current.get(signature) ?? [];
        key = keys[occurrence];
        if (!key) {
          key = `${device.kind}:synthetic-${nextSyntheticDeviceKeyRef.current++}`;
          keys.push(key);
          syntheticDeviceKeysBySignatureRef.current.set(signature, keys);
        }
        syntheticDeviceKeysByObjectRef.current.set(device, key);
      }
      return { device, key };
    });
  }, []);

  const refreshDevices = useCallback(async () => {
    if (!supported) return;
    try {
      const devices = normalizeDevices(await navigator.mediaDevices.enumerateDevices());
      setInputDevices(devices.filter(({ device }) => device.kind === "audioinput"));
      setOutputDevices(devices.filter(({ device }) => device.kind === "audiooutput"));
      setCameraDevices(devices.filter(({ device }) => device.kind === "videoinput"));
    } catch {
      // Restricted contexts may not permit enumeration. Keep system-default options.
    }
  }, [normalizeDevices, supported]);

  const stopMicTest = useCallback((updateState = true) => {
    micRequestGenerationRef.current += 1;
    if (micRafRef.current != null) {
      cancelAnimationFrame(micRafRef.current);
      micRafRef.current = null;
    }
    micStreamRef.current?.getTracks().forEach((track) => track.stop());
    micStreamRef.current = null;
    const context = micContextRef.current;
    micContextRef.current = null;
    if (context) void context.close();
    if (updateState) {
      setMicLevel(0);
      setMicTesting(false);
    }
  }, []);

  const stopCameraPreview = useCallback((updateState = true) => {
    cameraRequestGenerationRef.current += 1;
    const stream = cameraStreamRef.current;
    cameraStreamRef.current = null;
    stream?.getTracks().forEach((track) => track.stop());
    if (cameraVideoRef.current?.srcObject === stream) cameraVideoRef.current.srcObject = null;
    if (updateState) {
      setCameraPreviewStream(null);
      setCameraPreviewStarting(false);
    }
  }, []);

  const stopOutputTest = useCallback((updateState = true) => {
    outputRequestGenerationRef.current += 1;
    const resources = outputResourcesRef.current;
    outputResourcesRef.current = null;
    if (resources) disposeOutputTestResources(resources);
    if (updateState) setPlayingTestSound(false);
  }, []);

  useEffect(() => {
    if (!supported) return;
    const lifecycleGeneration = ++lifecycleGenerationRef.current;
    let active = true;
    void refreshDevices();
    navigator.mediaDevices.addEventListener("devicechange", refreshDevices);

    // Reuse this promise across Strict Mode's setup/cleanup/setup probe. The stream
    // is stopped by the promise itself, so no effect instance can orphan it.
    primePromiseRef.current ??= navigator.mediaDevices
      .getUserMedia({ audio: true })
      .then((stream) => {
        stream.getTracks().forEach((track) => track.stop());
      })
      .catch(() => {});
    void primePromiseRef.current.then(async () => {
      if (!active || lifecycleGeneration !== lifecycleGenerationRef.current) return;
      await refreshDevices();
      if (active && lifecycleGeneration === lifecycleGenerationRef.current) setIsLoading(false);
    });

    return () => {
      active = false;
      lifecycleGenerationRef.current += 1;
      navigator.mediaDevices.removeEventListener("devicechange", refreshDevices);
      stopMicTest(false);
      stopCameraPreview(false);
      stopOutputTest(false);
    };
  }, [refreshDevices, stopCameraPreview, stopMicTest, stopOutputTest, supported]);

  useEffect(() => {
    const video = cameraVideoRef.current;
    if (!video || !cameraPreviewStream) return;
    try {
      video.srcObject = cameraPreviewStream;
      void video.play()?.catch(() => {});
    } catch {
      // srcObject/play failures do not replace actionable capture errors.
    }
    return () => {
      if (video.srcObject === cameraPreviewStream) video.srcObject = null;
    };
  }, [cameraPreviewStream]);

  const startMicTest = async (selectedInputId = preferences.inputDeviceId) => {
    stopMicTest();
    const requestGeneration = micRequestGenerationRef.current;
    const lifecycleGeneration = lifecycleGenerationRef.current;
    setMicError(null);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: selectedInputId ? { deviceId: { exact: selectedInputId } } : true,
      });
      if (
        requestGeneration !== micRequestGenerationRef.current ||
        lifecycleGeneration !== lifecycleGenerationRef.current
      ) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      micStreamRef.current = stream;
      const context = new AudioContext();
      micContextRef.current = context;
      const source = context.createMediaStreamSource(stream);
      const analyser = context.createAnalyser();
      analyser.fftSize = 1024;
      source.connect(analyser);
      const buffer = new Uint8Array(analyser.fftSize);
      const tick = () => {
        if (requestGeneration !== micRequestGenerationRef.current) return;
        analyser.getByteTimeDomainData(buffer);
        let sumSquares = 0;
        for (const sample of buffer) {
          const value = (sample - 128) / 128;
          sumSquares += value * value;
        }
        setMicLevel(Math.min(1, Math.sqrt(sumSquares / buffer.length) * 2.5));
        micRafRef.current = requestAnimationFrame(tick);
      };
      tick();
      setMicTesting(true);
      void refreshDevices();
    } catch (error) {
      if (
        requestGeneration === micRequestGenerationRef.current &&
        lifecycleGeneration === lifecycleGenerationRef.current
      ) {
        stopMicTest();
        setMicError(error instanceof Error ? error.message : "Could not access microphone");
      }
    }
  };

  const startCameraPreview = async (selectedCameraId = preferences.cameraDeviceId) => {
    stopCameraPreview();
    const requestGeneration = cameraRequestGenerationRef.current;
    const lifecycleGeneration = lifecycleGenerationRef.current;
    setCameraError(null);
    setCameraPreviewStarting(true);
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: false,
        video: selectedCameraId ? { deviceId: { exact: selectedCameraId } } : true,
      });
      if (
        requestGeneration !== cameraRequestGenerationRef.current ||
        lifecycleGeneration !== lifecycleGenerationRef.current
      ) {
        stream.getTracks().forEach((track) => track.stop());
        return;
      }
      cameraStreamRef.current = stream;
      setCameraPreviewStream(stream);
      setCameraPreviewStarting(false);
      void refreshDevices();
    } catch (error) {
      if (
        requestGeneration === cameraRequestGenerationRef.current &&
        lifecycleGeneration === lifecycleGenerationRef.current
      ) {
        setCameraPreviewStarting(false);
        setCameraError(error instanceof Error ? error.message : "Could not access camera");
      }
    }
  };

  const playTestSound = async () => {
    stopOutputTest();
    const requestGeneration = outputRequestGenerationRef.current;
    const lifecycleGeneration = lifecycleGenerationRef.current;
    setOutputError(null);
    setPlayingTestSound(true);
    const context = new AudioContext();
    const resources: OutputTestResources = {
      context,
      audio: null,
      destinationStream: null,
      timer: null,
      finishDelay: null,
      disposed: false,
    };
    outputResourcesRef.current = resources;
    try {
      const oscillator = context.createOscillator();
      const gain = context.createGain();
      oscillator.type = "sine";
      oscillator.frequency.value = 440;
      const now = context.currentTime;
      gain.gain.setValueAtTime(0, now);
      gain.gain.linearRampToValueAtTime(0.2, now + 0.05);
      gain.gain.linearRampToValueAtTime(0, now + 0.75);
      if (preferences.outputDeviceId && canPickOutput) {
        const destination = context.createMediaStreamDestination();
        resources.destinationStream = destination.stream;
        oscillator.connect(gain).connect(destination);
        const audio = new Audio();
        resources.audio = audio;
        audio.srcObject = destination.stream;
        await (
          audio as HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> }
        ).setSinkId?.(preferences.outputDeviceId);
        if (
          requestGeneration !== outputRequestGenerationRef.current ||
          lifecycleGeneration !== lifecycleGenerationRef.current ||
          outputResourcesRef.current !== resources
        )
          return;
        oscillator.start(now);
        oscillator.stop(now + 0.8);
        await audio.play();
        if (
          requestGeneration !== outputRequestGenerationRef.current ||
          lifecycleGeneration !== lifecycleGenerationRef.current ||
          outputResourcesRef.current !== resources
        )
          return;
      } else {
        oscillator.connect(gain).connect(context.destination);
        oscillator.start(now);
        oscillator.stop(now + 0.8);
      }
      await new Promise<void>((resolve) => {
        resources.finishDelay = resolve;
        resources.timer = setTimeout(resolve, 850);
      });
    } catch (error) {
      if (
        requestGeneration === outputRequestGenerationRef.current &&
        lifecycleGeneration === lifecycleGenerationRef.current
      ) {
        setOutputError(error instanceof Error ? error.message : "Could not play test sound");
      }
    } finally {
      if (outputResourcesRef.current === resources) {
        outputResourcesRef.current = null;
        disposeOutputTestResources(resources);
        if (
          requestGeneration === outputRequestGenerationRef.current &&
          lifecycleGeneration === lifecycleGenerationRef.current
        ) {
          setPlayingTestSound(false);
        }
      }
    }
  };

  const inputLabel = (device: MediaDeviceInfo, index: number) =>
    device.label || `Microphone ${index + 1}`;
  const outputLabel = (device: MediaDeviceInfo, index: number) =>
    device.label || `Speaker ${index + 1}`;
  const cameraLabel = (device: MediaDeviceInfo, index: number) =>
    device.label || `Camera ${index + 1}`;

  return (
    <div className="relative flex flex-col gap-6">
      {isLoading ? (
        <div
          role="status"
          aria-live="polite"
          className="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-popover/60 backdrop-blur-sm"
        >
          <div className="flex items-center gap-3 text-sm text-foreground">
            <span
              aria-hidden="true"
              className="inline-block h-4 w-4 animate-spin rounded-full border-2 border-border border-t-primary"
            />
            <span>Loading media devices…</span>
          </div>
        </div>
      ) : null}
      {!supported ? (
        <p className="text-destructive" role="alert">
          This platform does not expose media device APIs, so voice and video chat are unavailable
          here.
        </p>
      ) : null}

      <div className="flex flex-col gap-2">
        <Label htmlFor="voice-input-select">Input device</Label>
        <select
          id="voice-input-select"
          className="bg-background text-foreground rounded-md px-3 py-2 text-sm border border-border focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          value={preferences.inputDeviceId}
          disabled={!supported}
          onChange={(event) => {
            const value = event.currentTarget.value;
            preferences.setInputDeviceId(value);
            if (micTesting) void startMicTest(value);
          }}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          {inputDevices.map(({ device, key }, index) => (
            <option key={key} value={device.deviceId}>
              {inputLabel(device, index)}
            </option>
          ))}
        </select>
        <div className="flex items-center gap-3 mt-1">
          <Button
            type="button"
            size="lg"
            onClick={() => (micTesting ? stopMicTest() : void startMicTest())}
            disabled={!supported}
          >
            {micTesting ? "Stop test" : "Test microphone"}
          </Button>
          <div
            className="flex-1 h-2 bg-muted rounded-md overflow-hidden"
            role="meter"
            aria-label="Microphone input level"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(micLevel * 100)}
          >
            <div
              className="h-full bg-green-500 transition-[width] duration-75"
              style={{ width: `${Math.round(micLevel * 100)}%` }}
            />
          </div>
        </div>
        {micError ? (
          <p className="text-destructive text-sm" role="alert">
            {micError}
          </p>
        ) : null}
        {micTesting ? (
          <p className="text-xs text-muted-foreground">
            Speak into your mic — the bar should move.
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="voice-camera-select">Camera</Label>
        <select
          id="voice-camera-select"
          className="bg-background text-foreground rounded-md px-3 py-2 text-sm border border-border focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          value={preferences.cameraDeviceId}
          disabled={!supported}
          onChange={(event) => {
            const value = event.currentTarget.value;
            preferences.setCameraDeviceId(value);
            if (cameraPreviewStream || cameraPreviewStarting) void startCameraPreview(value);
          }}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          {cameraDevices.map(({ device, key }, index) => (
            <option key={key} value={device.deviceId}>
              {cameraLabel(device, index)}
            </option>
          ))}
        </select>
        <div className="mt-1 flex items-center gap-3">
          <Button
            type="button"
            size="lg"
            onClick={() =>
              cameraPreviewStream || cameraPreviewStarting
                ? stopCameraPreview()
                : void startCameraPreview()
            }
            disabled={!supported}
            aria-busy={cameraPreviewStarting}
          >
            {cameraPreviewStarting
              ? "Starting preview…"
              : cameraPreviewStream
                ? "Stop preview"
                : "Preview camera"}
          </Button>
          {cameraPreviewStream ? (
            <p className="text-xs text-muted-foreground">
              Your camera preview stays local to this device.
            </p>
          ) : null}
        </div>
        {cameraPreviewStream ? (
          <video
            ref={cameraVideoRef}
            aria-label="Camera preview"
            autoPlay
            muted
            playsInline
            className="mt-2 aspect-video w-full max-w-sm rounded-md bg-black object-cover"
          />
        ) : null}
        {cameraError ? (
          <p className="text-destructive text-sm" role="alert">
            {cameraError}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor="voice-output-select">Output device</Label>
        <select
          id="voice-output-select"
          className="bg-background text-foreground rounded-md px-3 py-2 text-sm border border-border focus:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          value={preferences.outputDeviceId}
          disabled={!supported || !canPickOutput}
          onChange={(event) => {
            stopOutputTest();
            preferences.setOutputDeviceId(event.currentTarget.value);
          }}
        >
          <option value={DEFAULT_DEVICE_ID}>System default</option>
          {outputDevices.map(({ device, key }, index) => (
            <option key={key} value={device.deviceId}>
              {outputLabel(device, index)}
            </option>
          ))}
        </select>
        {supported && !canPickOutput ? (
          <p className="text-xs text-muted-foreground">
            This platform plays audio through the system default device; per-app output selection is
            not supported here.
          </p>
        ) : null}
        <div className="mt-1">
          <Button
            type="button"
            size="lg"
            onClick={() => void playTestSound()}
            disabled={!supported || playingTestSound}
          >
            {playingTestSound ? "Playing..." : "Play test sound"}
          </Button>
        </div>
        {outputError ? (
          <p className="text-destructive text-sm" role="alert">
            {outputError}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-3 border-t border-border pt-4">
        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={preferences.noiseSuppression}
            disabled={!supported}
            onChange={(event) => preferences.setNoiseSuppression(event.currentTarget.checked)}
          />
          <span className="flex flex-col gap-0.5">
            <span className="font-medium text-foreground">Noise suppression</span>
            <span className="text-xs text-muted-foreground">
              Filters steady background noise (fans, typing). Uses the browser's built-in processor.
            </span>
          </span>
        </label>
        <label className="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={preferences.showSpeakingEverywhere}
            onChange={(event) => preferences.setShowSpeakingEverywhere(event.currentTarget.checked)}
          />
          <span className="flex flex-col gap-0.5">
            <span className="font-medium text-foreground">
              If speaking indicators for voice channels I'm not in
            </span>
            <span className="text-xs text-muted-foreground">
              Highlights speakers in the sidebar even when you haven't joined the channel.
            </span>
          </span>
        </label>
        <div className="flex flex-col gap-1">
          <label htmlFor="voice-input-gain" className="flex items-center justify-between">
            <span className="font-medium text-foreground">Input volume</span>
            <span className="text-xs text-muted-foreground" aria-hidden="true">
              {Math.round(preferences.inputGain * 100)}%
            </span>
          </label>
          <input
            id="voice-input-gain"
            type="range"
            min="0"
            max="2"
            step="0.05"
            value={preferences.inputGain}
            disabled={!supported}
            className="w-full"
            onChange={(event) =>
              preferences.setInputGain(Number.parseFloat(event.currentTarget.value))
            }
          />
          <p className="text-xs text-muted-foreground">
            Adjusts how loud your voice is to other participants.
          </p>
        </div>
      </div>
    </div>
  );
}
