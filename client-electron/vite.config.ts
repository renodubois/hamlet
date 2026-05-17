import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import devtools from "solid-devtools/vite";

const rendererHost = "127.0.0.1";
const rendererPort = 1422;

// https://vitejs.dev/config/
export default defineConfig(() => ({
  plugins: [devtools(), solid()],

  // Keep the Electron renderer on a distinct, fixed localhost-style origin so it
  // can run side by side with the existing Solid client on port 1420.
  clearScreen: false,
  server: {
    host: rendererHost,
    port: rendererPort,
    strictPort: true,
  },
  preview: {
    host: rendererHost,
    port: rendererPort,
    strictPort: true,
  },
}));
