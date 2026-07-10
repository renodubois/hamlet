import type { StorybookConfig } from "@storybook/react-vite";
import { mergeConfig } from "vite";

const defaultServerUrl =
  process.env.VITE_HAMLET_DEFAULT_SERVER_URL ??
  process.env.HAMLET_SERVER_URL ??
  "http://127.0.0.1:3030";

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx|mdx)"],
  addons: ["@storybook/addon-a11y", "msw-storybook-addon"],
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  staticDirs: ["./public"],
  viteFinal: (viteConfig) =>
    mergeConfig(viteConfig, {
      define: {
        "import.meta.env.VITE_HAMLET_DEFAULT_SERVER_URL": JSON.stringify(defaultServerUrl),
      },
      server: {
        strictPort: false,
      },
      preview: {
        strictPort: false,
      },
    }),
};

export default config;
