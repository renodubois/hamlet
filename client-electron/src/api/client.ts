const DEFAULT_SERVER = "http://127.0.0.1:3030";

export function getServerUrl(): string {
  return localStorage.getItem("hamlet.serverUrl") ?? DEFAULT_SERVER;
}

export function setServerUrl(url: string): void {
  localStorage.setItem("hamlet.serverUrl", url);
}

export async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  return fetch(`${getServerUrl()}${path}`, { credentials: "include", ...init });
}
