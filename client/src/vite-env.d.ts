/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_HAMLET_DEFAULT_SERVER_URL?: string;
  readonly VITE_HAMLET_SENTRY_DSN?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
