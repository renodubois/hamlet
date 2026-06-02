export const DEFAULT_STATIC_RENDERER_HOST = "127.0.0.1" as const;
export const DEFAULT_STATIC_RENDERER_PORT = 1422 as const;

// Backwards-compatible default constants. Runtime code should use the resolver
// helpers below so worktrees can opt into isolated renderer ports.
export const STATIC_RENDERER_HOST = DEFAULT_STATIC_RENDERER_HOST;
export const STATIC_RENDERER_PORT = DEFAULT_STATIC_RENDERER_PORT;
export const STATIC_RENDERER_ORIGIN = rendererOrigin(
  STATIC_RENDERER_HOST,
  STATIC_RENDERER_PORT,
) as `http://${typeof STATIC_RENDERER_HOST}:${typeof STATIC_RENDERER_PORT}`;

export const DEFAULT_RENDERER_DEV_ORIGIN = STATIC_RENDERER_ORIGIN;

export const ELECTRON_WINDOW_TITLE = "Hamlet Electron Alpha" as const;

export interface RendererPortEnvironment {
  HAMLET_RENDERER_HOST?: string;
  HAMLET_RENDERER_PORT?: string;
}

export function resolveConfiguredRendererHost(env: RendererPortEnvironment = process.env): string {
  const host = env.HAMLET_RENDERER_HOST?.trim();
  return host === undefined || host === "" ? DEFAULT_STATIC_RENDERER_HOST : host;
}

export function resolveConfiguredRendererPort(env: RendererPortEnvironment = process.env): number {
  const rawPort = env.HAMLET_RENDERER_PORT?.trim();
  if (rawPort === undefined || rawPort === "") return DEFAULT_STATIC_RENDERER_PORT;

  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`Invalid HAMLET_RENDERER_PORT "${rawPort}". Expected 0-65535.`);
  }
  return port;
}

export function resolveConfiguredRendererOrigin(
  env: RendererPortEnvironment = process.env,
): string {
  return rendererOrigin(resolveConfiguredRendererHost(env), resolveConfiguredRendererPort(env));
}

export function rendererOrigin(host: string, port: number): string {
  return `http://${host.includes(":") ? `[${host}]` : host}:${port}`;
}
