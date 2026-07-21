export function normalizeRouterBasename(baseUrl: string): string {
  const path = baseUrl.trim().replace(/^\/+|\/+$/g, "");
  return path === "" ? "/" : `/${path}`;
}
