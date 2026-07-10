import type { Preview } from "@storybook/react-vite";
import { isCommonAssetRequest, type UnhandledRequestCallback } from "msw";
import { initialize, mswLoader } from "msw-storybook-addon";
import { MemoryRouter } from "react-router-dom";

import "../src/index.css";

const storybookServerUrl =
  import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL ?? "http://127.0.0.1:3030";

const onUnhandledRequest: UnhandledRequestCallback = (request, print) => {
  if (isCommonAssetRequest(request)) return;

  const url = new URL(request.url);
  if (url.pathname.startsWith("/sb-") || url.pathname.includes("hot-update")) return;

  if (url.hostname === "127.0.0.1" || url.hostname === "localhost") {
    print.warning();
  }
};

initialize({ onUnhandledRequest });

const preview: Preview = {
  loaders: [mswLoader],
  decorators: [
    (Story, context) => {
      const initialPath = context.parameters.router?.initialPath ?? "/";
      localStorage.setItem("hamlet.serverUrl", storybookServerUrl);

      return (
        <MemoryRouter initialEntries={[initialPath]}>
          <div className="min-h-screen bg-white p-6 text-gray-900">
            <Story />
          </div>
        </MemoryRouter>
      );
    },
  ],
  parameters: {
    a11y: { test: "todo" },
    controls: { expanded: true },
  },
};

export default preview;
