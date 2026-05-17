export const STATIC_RENDERER_HOST = "127.0.0.1" as const;
export const STATIC_RENDERER_PORT = 1422 as const;
export const STATIC_RENDERER_ORIGIN =
  `http://${STATIC_RENDERER_HOST}:${STATIC_RENDERER_PORT}` as const;

export const DEFAULT_RENDERER_DEV_ORIGIN = STATIC_RENDERER_ORIGIN;

export const ELECTRON_WINDOW_TITLE = "Hamlet Electron Alpha" as const;
