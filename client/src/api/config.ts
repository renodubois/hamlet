import { getServerUrl } from "./client";

export interface PublicServerConfig {
  account_registration_enabled: boolean;
}

export async function getPublicServerConfig(
  serverUrl: string = getServerUrl(),
  signal?: AbortSignal,
): Promise<PublicServerConfig> {
  const base = serverUrl.trim().replace(/\/+$/, "");
  const res = await fetch(`${base}/config`, { credentials: "include", signal });
  if (!res.ok) throw new Error(`Public server config request failed (${res.status})`);
  return res.json() as Promise<PublicServerConfig>;
}
