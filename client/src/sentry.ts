import * as Sentry from "@sentry/solid";

type SentryInitOptions = NonNullable<Parameters<typeof Sentry.init>[0]>;

export interface RendererSentryClient {
  init(options: SentryInitOptions): unknown;
}

export const RENDERER_SENTRY_PRIVACY_OPTIONS = {
  dataCollection: {
    userInfo: false,
    httpBodies: [],
  },
} satisfies Pick<SentryInitOptions, "dataCollection">;

export function normalizeSentryDsn(dsn: string | undefined): string | undefined {
  const trimmedDsn = dsn?.trim();
  return trimmedDsn === undefined || trimmedDsn === "" ? undefined : trimmedDsn;
}

export function initializeRendererSentry({
  dsn = import.meta.env.VITE_HAMLET_SENTRY_DSN,
  sentry = Sentry,
}: {
  dsn?: string;
  sentry?: RendererSentryClient;
} = {}): boolean {
  const normalizedDsn = normalizeSentryDsn(dsn);

  if (normalizedDsn === undefined) {
    return false;
  }

  sentry.init({
    dsn: normalizedDsn,
    ...RENDERER_SENTRY_PRIVACY_OPTIONS,
  });

  return true;
}
