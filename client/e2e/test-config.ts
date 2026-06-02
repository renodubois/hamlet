import { hamletEnv } from "../playwright.env";

export const serverUrl =
  hamletEnv.HAMLET_SERVER_URL ??
  hamletEnv.VITE_HAMLET_DEFAULT_SERVER_URL ??
  "http://127.0.0.1:3030";
export const livekitUrl = hamletEnv.LIVEKIT_URL ?? "ws://127.0.0.1:7880";

export function rendererOriginPattern(): RegExp {
  const rendererUrl =
    hamletEnv.HAMLET_RENDERER_URL ??
    `http://${hamletEnv.HAMLET_RENDERER_HOST ?? "127.0.0.1"}:${hamletEnv.HAMLET_RENDERER_PORT ?? "1422"}`;
  const origin = new URL(rendererUrl).origin.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${origin}(?:/|$)`);
}
