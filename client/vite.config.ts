import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

const defaultRendererHost = "127.0.0.1";
const defaultRendererPort = 1422;
const defaultServerUrl = "http://127.0.0.1:3030";

function envOr(env: Record<string, string>, key: string, fallback: string): string {
  const value = env[key]?.trim();
  return value === undefined || value === "" ? fallback : value;
}

function rendererPort(env: Record<string, string>): number {
  const rawPort = envOr(env, "HAMLET_RENDERER_PORT", String(defaultRendererPort));
  const port = Number(rawPort);
  if (!Number.isInteger(port) || port < 0 || port > 65535) {
    throw new Error(`Invalid HAMLET_RENDERER_PORT "${rawPort}". Expected 0-65535.`);
  }
  return port;
}

function envFlag(env: Record<string, string>, key: string): boolean {
  const rawValue = env[key]?.trim().toLowerCase();
  if (rawValue === undefined || rawValue === "" || rawValue === "0" || rawValue === "false") {
    return false;
  }
  if (rawValue === "1" || rawValue === "true") {
    return true;
  }
  throw new Error(`Invalid ${key} "${env[key]}". Expected true/false or 1/0.`);
}

// https://vitejs.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), "");
  const rendererHost = envOr(env, "HAMLET_RENDERER_HOST", defaultRendererHost);
  const port = rendererPort(env);
  const hamletServerUrl = envOr(
    env,
    "VITE_HAMLET_DEFAULT_SERVER_URL",
    envOr(env, "HAMLET_SERVER_URL", defaultServerUrl),
  );
  const emitSourceMaps = envFlag(env, "HAMLET_BUILD_SOURCE_MAPS");

  return {
    plugins: [react()],

    // Worktrees can override HAMLET_RENDERER_PORT to run side by side.
    clearScreen: false,
    server: {
      host: rendererHost,
      port,
      strictPort: true,
    },
    preview: {
      host: rendererHost,
      port,
      strictPort: true,
    },
    build: {
      sourcemap: emitSourceMaps,
    },
    define: {
      "import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL": JSON.stringify(hamletServerUrl),
    },
  };
});
