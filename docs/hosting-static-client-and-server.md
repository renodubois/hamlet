# Hosting Hamlet with a static web client and public server

_Last reviewed: 2026-07-21_

This runbook covers the production hosting shape Hamlet now supports:

- **Web client**: the React/Vite renderer from `client/`, built as static HTML/CSS/JS and hosted on a static host such as GitHub Pages. Electron is not used.
- **API server**: the Rust/Actix server from `server/`, hosted on an Ubuntu Server homelab machine or cloud VPS, behind HTTPS on a custom domain.
- **Optional voice/video**: LiveKit, either self-hosted beside the server or provided by LiveKit Cloud.

Examples below use these placeholder domains:

| Purpose | Placeholder |
| --- | --- |
| Web client | `https://chat.example.com` |
| Hamlet API | `https://api.example.com` |
| LiveKit signaling | `wss://livekit.example.com` |

Use subdomains of the same site you own when possible. `chat.example.com` and `api.example.com` are cross-origin, but they are same-site in modern cookie rules. Avoid deploying the client at `https://<user>.github.io` while the API is on `https://api.example.com`; that becomes a third-party-cookie scenario and is more likely to break in browsers.

## Current project fit and production controls

The hosted web-client path is no longer blocked on localhost-only assumptions. The current deployment controls are:

- `HAMLET_ALLOWED_ORIGINS` is a comma-separated list of exact browser origins allowed to make credentialed CORS requests, for example `https://chat.example.com`. Values must be origins only: scheme + host + optional port, with no path, query, fragment, or credentials.
- `HAMLET_COOKIE_SECURE` controls the `Secure` attribute on the `session` and `hamlet_csrf` cookies. Set it to `true` for HTTPS deployments.
- `HAMLET_COOKIE_SAME_SITE` accepts `lax`, `strict`, or `none`. The default is `lax`; `none` is rejected unless `HAMLET_COOKIE_SECURE=true`.
- CSRF protection is enabled for browser-shaped unsafe authenticated writes. `POST /login`, `POST /register`, and `POST /logout` are exempt so a fresh browser can authenticate or clear a stale session. Authenticated clients can call `GET /csrf` to receive `{ "token": "..." }` and a non-HttpOnly `hamlet_csrf` cookie; unsafe `POST`, `PUT`, `PATCH`, and `DELETE` requests echo the token in `X-Hamlet-CSRF`.
- `server/docker-compose.yml` is the production API baseline. It publishes the API on loopback by default, stores SQLite/uploads under one `hamlet_data` volume, disables development seed data, and leaves LiveKit optional. `server/docker-compose.livekit.yml` adds self-hosted LiveKit for production. `server/docker-compose.override.yml` is the development hot-reload stack with local LiveKit defaults.
- The production Docker image builds and ships both `hamlet` and `hamlet-admin`, so operators can provision accounts inside the deployed image while public registration remains disabled.
- Static web output is first-class via `pnpm run build:web`, which runs the Vite renderer build and prepares GitHub Pages SPA fallback files.
- Renderer telemetry is opt-in via `VITE_HAMLET_SENTRY_DSN`; server telemetry is separate and opt-in via `HAMLET_SENTRY_DSN`.

## Additional tools/services needed

Minimum:

- Domain DNS control for your client/API/LiveKit subdomains.
- GitHub Pages or another static host.
- Node.js 24.13.0+ and pnpm 11.11.0+ for the client build. The project has `pnpm-lock.yaml`; use `pnpm install --frozen-lockfile` in automation.
- An Ubuntu Server host or VPS.
- Docker Engine with the Compose plugin, or a Rust toolchain plus systemd if not using Docker.
- A reverse proxy with trusted TLS certificates. Caddy is the simplest option because it manages HTTPS automatically; Nginx + Certbot is also fine.
- Firewall/router access to expose only the required ports.

Recommended:

- Backup tooling for `/var/lib/hamlet` or the Docker named volume, such as `restic`, `borg`, or encrypted off-host `rsync`.
- Dynamic DNS if your homelab IP changes.
- LiveKit Cloud or a self-hosted LiveKit deployment for voice/video.
- TURN support for difficult networks. LiveKit can provide embedded TURN; `coturn` is another option.
- Monitoring/logging such as Sentry for the server, uptime checks, and disk-space alerts.

## Recommended domain and DNS layout

Use subdomains instead of path-prefix hosting:

```text
chat.example.com     -> GitHub Pages static client
api.example.com      -> Ubuntu/VPS public IP, reverse-proxied to Hamlet server
livekit.example.com  -> same Ubuntu/VPS public IP or LiveKit Cloud endpoint
```

For GitHub Pages, configure the custom domain in the repository's **Settings → Pages** before adding DNS records to reduce subdomain-takeover risk. For a subdomain, create a CNAME record like:

```text
chat.example.com.  CNAME  <your-github-user-or-org>.github.io.
```

For the server and self-hosted LiveKit:

```text
api.example.com.      A/AAAA  <public IP of Ubuntu/VPS>
livekit.example.com.  A/AAAA  <public IP of Ubuntu/VPS>
```

If using a homelab:

- Forward TCP `80` and `443` from your router to the Ubuntu host for Caddy/HTTPS.
- If self-hosting LiveKit, also forward the configured LiveKit media ports, usually TCP `7881` and UDP `50000-50100` or a wider UDP range.
- Verify your ISP does not put you behind CGNAT. If you cannot receive inbound connections, use a VPS or move LiveKit to LiveKit Cloud. HTTP tunnels can help the API, but they usually do not solve LiveKit UDP media hosting.

## Static client on GitHub Pages

### Local build smoke test

From the repo root:

```bash
cd client
pnpm install --frozen-lockfile
pnpm run fmt:check
pnpm run lint
pnpm run check:native-react
pnpm run typecheck
pnpm run test
VITE_HAMLET_DEFAULT_SERVER_URL=https://api.example.com \
VITE_HAMLET_SENTRY_DSN= \
pnpm run build:web
```

Notes:

- Use `pnpm run build:web` for static hosting. It runs the Vite renderer build, copies `dist/index.html` to `dist/404.html` for GitHub Pages deep-link fallback, and writes `dist/.nojekyll`.
- `VITE_HAMLET_DEFAULT_SERVER_URL` is baked into the static bundle as the default server URL shown by the login screen. Users can still override the server URL in localStorage from the login UI.
- `VITE_HAMLET_SENTRY_DSN` is optional. Leave it unset or empty to disable renderer-side Sentry. This is separate from server-side `HAMLET_SENTRY_DSN`.
- `VITE_HAMLET_BASE_PATH` is the static site's URL path. Use `/hamlet/` (or the repository's actual name) when locally reproducing a GitHub Pages project site at `https://<owner>.github.io/hamlet/`; omit it or use `/` for a custom domain or user/organization site. The value is normalized to leading and trailing slashes.
- The API URL must be HTTPS. An HTTPS static page cannot use an insecure `http://` API because of browser mixed-content rules.

Preview the output with any static server, then log in against the public/staging API.

### GitHub Pages deployment workflow

Set the repository's Pages source to **GitHub Actions**. The checked-in [`.github/workflows/deploy-web-client.yml`](../.github/workflows/deploy-web-client.yml) automatically checks, builds, and deploys the web client on pushes to `main`; it can also be run manually with **Run workflow**.

For a manual run, the `api_url` field is baked into the login screen and remains editable there by users. For push-triggered runs, which have no dispatch inputs, the workflow uses the `HAMLET_DEFAULT_SERVER_URL` repository Actions variable when set and otherwise safely falls back to `https://api.hamlet.chat`. Set that variable under **Settings → Secrets and variables → Actions → Variables** before pushing if your deployment uses another API origin.

Renderer Sentry selection is deliberately event-specific: a manual run uses the `renderer_sentry_dsn` field exactly, and its blank default disables telemetry rather than falling through to a repository value. A push build uses the optional `HAMLET_RENDERER_SENTRY_DSN` repository variable. The workflow obtains GitHub's configured Pages `base_path` before building and passes it to Vite, so repository project sites receive `/repository-name/` asset URLs while custom domains and user/organization sites build at `/`. Its build job has read access to Pages metadata, while only the deploy job receives Pages and OIDC write permissions.

[`docs/examples/github-pages-deploy-web-client.yml`](examples/github-pages-deploy-web-client.yml) remains a manual-only generic template for downstream repositories that do not want Hamlet's checked-in automatic deployment. Its third-party actions are full-commit SHA pinned; when updating them, review the upstream release and replace both the SHA and version comment.

After the first deploy:

1. Open the repository's **Settings → Pages**.
2. Set custom domain to `chat.example.com`.
3. Wait for DNS verification.
4. Re-run the deployment workflow so the client is rebuilt after GitHub reports the custom domain. This is required: the Pages base path is baked into asset URLs and the client router at build time, so an artifact built for a `/repository-name/` project site must not be reused at the custom-domain root.
5. Enable **Enforce HTTPS** when GitHub makes it available.
6. Test `https://chat.example.com/login` and a deep route refresh such as `https://chat.example.com/channel/<id>`.

If you choose a host such as Netlify or Cloudflare Pages instead of GitHub Pages, configure a proper SPA rewrite (`/* -> /index.html`) instead of relying on `404.html`.

## Ubuntu/VPS server deployment with Docker and Caddy

The safest production shape is:

```text
Internet HTTPS -> Caddy on host -> Hamlet server on 127.0.0.1:3030
Internet WSS   -> Caddy on host -> LiveKit signaling on 127.0.0.1:7880
Internet UDP/TCP media ports -> LiveKit directly
```

### 1. Prepare the host

Install system packages, Docker Engine/Compose, Caddy, and Git using your normal host-management approach. For firewalling with `ufw`, a minimal non-voice setup exposes only SSH, HTTP, and HTTPS:

```bash
sudo ufw allow OpenSSH
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw enable
```

If self-hosting LiveKit, also allow your configured media ports, for example:

```bash
sudo ufw allow 7881/tcp
sudo ufw allow 50000:50100/udp
```

Be aware that Docker's port publishing can interact with host firewalls. The production Compose file binds the Hamlet server to `127.0.0.1`; Caddy should be the only public HTTP(S) listener.

Clone the repo:

```bash
sudo mkdir -p /opt/hamlet
sudo chown "$USER":"$USER" /opt/hamlet
git clone https://github.com/<you>/<hamlet-repo>.git /opt/hamlet
cd /opt/hamlet/server
```

### 2. Create production environment values

Generate strong LiveKit credentials if using self-hosted or cloud LiveKit:

```bash
openssl rand -hex 16   # API key candidate
openssl rand -hex 32   # API secret candidate
```

Create `server/.env` on the host. Do not commit it.

```dotenv
HAMLET_SERVER_PORT=3030
RUST_LOG=info

# Exact static-client origins allowed to make credentialed browser requests.
HAMLET_ALLOWED_ORIGINS=https://chat.example.com

# HTTPS cookie policy. For same-site subdomains (chat.example.com -> api.example.com),
# Lax is usually sufficient and more restrictive than None.
HAMLET_COOKIE_SECURE=true
HAMLET_COOKIE_SAME_SITE=lax

# Keep public production registration disabled and provision users with hamlet-admin.
HAMLET_ACCOUNT_REGISTRATION_ENABLED=false

# Optional server-side Sentry. Leave blank to disable.
HAMLET_SENTRY_DSN=

# Leave LiveKit URL/credentials empty if voice/video is disabled.
# LIVEKIT_URL is the server-side LiveKit URL. LIVEKIT_CLIENT_URL is the
# browser-reachable signaling URL returned by /voice/token; leave it empty
# when clients can use LIVEKIT_URL directly.
LIVEKIT_URL=
LIVEKIT_CLIENT_URL=
LIVEKIT_API_KEY=
LIVEKIT_API_SECRET=
# Set when using the optional self-hosted LiveKit Compose file.
HAMLET_LIVEKIT_CONFIG=./livekit.prod.yaml
```

Cookie notes:

- `HAMLET_COOKIE_SECURE=true` is expected for HTTPS. It is safe even though Caddy talks to the container over plain HTTP; the browser sees the public HTTPS origin.
- `HAMLET_COOKIE_SAME_SITE=lax` works for same-site subdomains such as `chat.example.com` and `api.example.com`.
- Use `HAMLET_COOKIE_SAME_SITE=none` only for truly cross-site client/API deployments, and always with `HAMLET_COOKIE_SECURE=true`. Some browsers block third-party cookies even with `SameSite=None`, so same-site subdomains are preferred.

### 3. Use the checked-in production Compose files

The checked-in `server/docker-compose.yml` is the production API baseline: it publishes the API on `127.0.0.1:${HAMLET_SERVER_PORT:-3030}`, stores SQLite and uploads in the `hamlet_data` volume, disables development seed data, and leaves LiveKit disabled unless its env vars are set. Use `-f docker-compose.yml` explicitly on production hosts so the development override is not loaded.

Validate and start the API:

```bash
docker compose -f docker-compose.yml --env-file .env config
docker compose -f docker-compose.yml --env-file .env up -d --build
```

For self-hosted LiveKit, add the optional companion file:

```bash
docker compose -f docker-compose.yml -f docker-compose.livekit.yml --env-file .env config
docker compose -f docker-compose.yml -f docker-compose.livekit.yml --env-file .env up -d --build
```

This keeps SQLite, public uploads, private message attachments, and server state in the `hamlet_data` Docker volume mounted at `/var/lib/hamlet`.

### 4. Configure LiveKit, if voice/video is in scope

If voice/video is not needed yet, do not include `docker-compose.livekit.yml` and leave `LIVEKIT_URL`, `LIVEKIT_API_KEY`, and `LIVEKIT_API_SECRET` empty. Hamlet will still boot; voice endpoints return 503.

If the Hamlet server reaches LiveKit through an internal URL but browsers must use a public domain, set both URLs. For example, with API traffic on `https://api.example.com` and LiveKit signaling on `wss://livekit.example.com`, use `LIVEKIT_CLIENT_URL=wss://livekit.example.com` so `/voice/token/*` returns the client-reachable address instead of the server-side `LIVEKIT_URL`.

For self-hosted LiveKit, create `server/livekit.prod.yaml` on the host. API key and secret must match `server/.env`.

```yaml
port: 7880
bind_addresses:
  - ""
rtc:
  tcp_port: 7881
  port_range_start: 50000
  port_range_end: 50100
  # Good default for a VPS/public cloud host. For a homelab behind NAT,
  # verify candidates carefully; you may need node_ip or TURN.
  use_external_ip: true
keys:
  REPLACE_WITH_LIVEKIT_API_KEY: REPLACE_WITH_LIVEKIT_API_SECRET
webhook:
  api_key: REPLACE_WITH_LIVEKIT_API_KEY
  urls:
    # Internal callback works when LiveKit and the server share the host.
    - http://127.0.0.1:3030/livekit/webhook
logging:
  level: info
```

For LiveKit Cloud:

1. Do not run the local `livekit` service.
2. Set `LIVEKIT_URL` to the Cloud project's `wss://...` URL.
3. Leave `LIVEKIT_CLIENT_URL` empty unless clients need a different public URL.
4. Set `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET` to that project's credentials.
5. In LiveKit Cloud, configure a webhook to `https://api.example.com/livekit/webhook` if you need participant/sidebar state to update from LiveKit webhooks.

### 5. Configure Caddy

Create a Caddyfile, usually `/etc/caddy/Caddyfile`:

```caddyfile
api.example.com {
  encode zstd gzip
  reverse_proxy 127.0.0.1:3030 {
    # Low-latency streaming helps /messages/subscribe SSE.
    flush_interval -1
  }
}

# Only needed for self-hosted LiveKit signaling.
livekit.example.com {
  reverse_proxy 127.0.0.1:7880
}
```

Then reload Caddy:

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

Caddy will obtain and renew certificates automatically when DNS points at the host and ports 80/443 are reachable.

### 6. Start Hamlet

From `server/`:

```bash
docker compose -f docker-compose.yml --env-file .env config
docker compose -f docker-compose.yml --env-file .env up -d --build
docker compose -f docker-compose.yml --env-file .env logs -f server
```

Smoke test from your workstation:

```bash
curl -i https://api.example.com/config
```

Expected: HTTP 200 with JSON like:

```json
{"account_registration_enabled":false}
```

### 7. Create the first account

For a public server, keep registration disabled except during a controlled invite window. The normal production path is the shipped admin CLI inside the Compose image, so it uses the same `/var/lib/hamlet` volume as the server container:

```bash
docker compose -f docker-compose.yml --env-file .env run --rm --no-deps server \
  hamlet-admin create-user --username alice --password 'replace-with-a-strong-password'
```

Alternative controlled-registration path:

1. Set `HAMLET_ACCOUNT_REGISTRATION_ENABLED=true`.
2. Restart the server.
3. Create the first account through `https://chat.example.com/login`.
4. Set `HAMLET_ACCOUNT_REGISTRATION_ENABLED=false` and restart again.

## CSRF behavior for hosted clients

The web client handles CSRF automatically through `client/src/api/client.ts`, including multipart upload requests.

Operational details:

- `POST /login` and `POST /register` set both the HttpOnly `session` cookie and a non-HttpOnly `hamlet_csrf` cookie on the API host.
- `GET /csrf` requires a valid session cookie and returns `{ "token": "..." }`. It also sets/refreshes the `hamlet_csrf` cookie. This endpoint is useful when the static client cannot read the API-host cookie directly because the client and API are on different subdomains.
- Authenticated browser unsafe methods (`POST`, `PUT`, `PATCH`, `DELETE`) must send `X-Hamlet-CSRF` with the token value. The browser also sends the matching `hamlet_csrf` cookie back to the API host.
- Missing, mismatched, or invalid CSRF tokens on browser-shaped unsafe authenticated writes return 403.
- Non-browser local tooling that omits `Origin`, the CSRF header, and the CSRF cookie is not challenged by the browser-write CSRF gate.

## Source/systemd alternative

If you do not want Docker, build the server on the Ubuntu host and run it under systemd.

High-level steps:

```bash
# install Rust and build deps first
cd /opt/hamlet/server
cargo build --release --locked --bin hamlet --bin hamlet-admin
sudo install -m 0755 target/release/hamlet /usr/local/bin/hamlet
sudo install -m 0755 target/release/hamlet-admin /usr/local/bin/hamlet-admin
sudo useradd --system --home /var/lib/hamlet --create-home --shell /usr/sbin/nologin hamlet
sudo mkdir -p /var/lib/hamlet/uploads /var/lib/hamlet/private-uploads/message-attachments
sudo chown -R hamlet:hamlet /var/lib/hamlet
```

Example `/etc/systemd/system/hamlet.service`:

```ini
[Unit]
Description=Hamlet API server
After=network-online.target
Wants=network-online.target

[Service]
User=hamlet
Group=hamlet
Environment=HAMLET_BIND_ADDR=127.0.0.1:3030
Environment=HAMLET_DATA_DIR=/var/lib/hamlet
Environment=HAMLET_UPLOADS_DIR=/var/lib/hamlet/uploads
Environment=HAMLET_MESSAGE_ATTACHMENTS_DIR=/var/lib/hamlet/private-uploads/message-attachments
Environment=HAMLET_BOOTSTRAP_DEFAULT_CHANNELS=true
Environment=HAMLET_SEED_DEV_DATA=false
Environment=HAMLET_ALLOWED_ORIGINS=https://chat.example.com
Environment=HAMLET_COOKIE_SECURE=true
Environment=HAMLET_COOKIE_SAME_SITE=lax
Environment=HAMLET_ACCOUNT_REGISTRATION_ENABLED=false
Environment=HAMLET_SENTRY_DSN=
Environment=RUST_LOG=info
# Environment=LIVEKIT_URL=wss://livekit.example.com
# Environment=LIVEKIT_CLIENT_URL=wss://livekit.example.com
# Environment=LIVEKIT_API_KEY=...
# Environment=LIVEKIT_API_SECRET=...
ExecStart=/usr/local/bin/hamlet
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now hamlet
journalctl -u hamlet -f
```

Caddy can use the same `api.example.com -> 127.0.0.1:3030` reverse proxy.

Provision users with the installed CLI while registration remains disabled:

```bash
sudo -u hamlet HAMLET_DATA_DIR=/var/lib/hamlet \
  hamlet-admin create-user --username alice --password 'replace-with-a-strong-password'
```

## Production smoke checklist

Before inviting users, complete this checklist against the public domains:

1. **Static SPA and deep-link refresh**
   - `https://chat.example.com` loads over HTTPS.
   - Direct navigation and refresh work for `https://chat.example.com/login` and a deep route such as `https://chat.example.com/channel/<id>`.
2. **CORS preflight and credentialed API calls**
   - Preflight from the client origin succeeds:

     ```bash
     curl -i -X OPTIONS https://api.example.com/login \
       -H 'Origin: https://chat.example.com' \
       -H 'Access-Control-Request-Method: POST' \
       -H 'Access-Control-Request-Headers: content-type,x-hamlet-csrf'
     ```

   - The response includes `access-control-allow-origin: https://chat.example.com` and `access-control-allow-credentials: true`, and does not allow unrelated origins.
   - Browser devtools show `GET /config`, `POST /login`, `GET /me`, `GET /channels`, and `GET /messages/subscribe` succeeding without CORS errors.
3. **Login cookie attributes**
   - `POST /login` sets an HttpOnly `session` cookie for `api.example.com` with `Secure` and the configured `SameSite` value.
   - `POST /login` or `GET /csrf` sets a non-HttpOnly `hamlet_csrf` cookie with the same `Secure`/`SameSite` policy.
   - Refreshing the static client keeps the session signed in.
4. **CSRF success and failure**
   - While logged in, `GET /csrf` returns HTTP 200 and a non-empty JSON `token`.
   - Normal client writes such as send message, edit/delete message, reactions, avatar upload, custom emoji upload, and photo upload succeed.
   - Replaying an authenticated browser unsafe request with the `Origin` header but without `X-Hamlet-CSRF` returns HTTP 403.
   - Replaying with a mismatched CSRF cookie/header returns HTTP 403.
5. **Admin provisioning with registration disabled**
   - `GET /config` reports `account_registration_enabled: false`.
   - Creating a user with `hamlet-admin` inside the Compose image succeeds (use a unique smoke username if repeating the test):

     ```bash
     docker compose -f docker-compose.yml --env-file .env run --rm --no-deps server \
       hamlet-admin create-user --username smoke-admin --password 'temporary-strong-password'
     ```

   - The provisioned user can log in, and arbitrary browser registration remains disabled.
6. **SSE and uploads persist across restart**
   - Open two browsers/profiles and confirm SSE delivers new messages without reload.
   - Upload an avatar/photo or custom emoji and confirm the rendered asset URL works.
   - Restart the server container/process:

     ```bash
     docker compose -f docker-compose.yml --env-file .env restart server
     ```

   - Confirm channel history, uploaded assets, sessions, and SSE reconnect behavior survive the restart.
7. **Voice/video, if enabled**
   - Join a voice channel from two different networks and verify LiveKit signaling (`wss://livekit.example.com`), microphone, camera, screen share, participant sidebar, and leave/rejoin behavior.
8. **Telemetry, if enabled**
   - Server Sentry uses `HAMLET_SENTRY_DSN` and renderer Sentry uses `VITE_HAMLET_SENTRY_DSN`; leaving either blank disables that side.

## Operations runbook

### Logs

Docker API-only:

```bash
cd /opt/hamlet/server
docker compose -f docker-compose.yml --env-file .env logs -f server
```

Docker with self-hosted LiveKit:

```bash
cd /opt/hamlet/server
docker compose -f docker-compose.yml -f docker-compose.livekit.yml --env-file .env logs -f server livekit
```

systemd:

```bash
journalctl -u hamlet -f
journalctl -u caddy -f
```

### Backups

Back up all durable Hamlet data together:

- SQLite database: `/var/lib/hamlet/hamlet.db` and SQLite sidecars.
- Public uploads: `/var/lib/hamlet/uploads`.
- Private message attachments: `/var/lib/hamlet/private-uploads`.

With Docker named volumes, either back up the volume contents from a helper container or bind-mount `/var/lib/hamlet` directly in your production Compose. Always stop the server or use SQLite-safe backup tooling/snapshots to avoid inconsistent database copies.

### Upgrades

API-only:

```bash
cd /opt/hamlet
git pull
cd server
docker compose -f docker-compose.yml --env-file .env up -d --build
docker compose -f docker-compose.yml --env-file .env logs -f server
```

With self-hosted LiveKit:

```bash
cd /opt/hamlet
git pull
cd server
docker compose -f docker-compose.yml -f docker-compose.livekit.yml --env-file .env up -d --build
docker compose -f docker-compose.yml -f docker-compose.livekit.yml --env-file .env logs -f server livekit
```

After upgrade, test `/config`, login, channel history, message send, uploads, CSRF-gated writes, and voice if enabled.

### Security notes

- Use HTTPS everywhere.
- Keep `HAMLET_ALLOWED_ORIGINS` restricted to exact client origins you control.
- Keep `HAMLET_COOKIE_SECURE=true` in production.
- Prefer same-site subdomains and `HAMLET_COOKIE_SAME_SITE=lax`; use `none` only when truly necessary.
- Keep registration disabled by default and provision users with `hamlet-admin`.
- Replace dev LiveKit credentials and never commit production secrets. The production Compose baseline does not include dev LiveKit credentials.
- Expose the Hamlet server only on loopback; Caddy should be the public HTTP(S) entry point.
- Do not publish wildcard DNS records for GitHub Pages subdomains.
- Keep Ubuntu, Docker images, Caddy, and dependencies patched.
- Monitor disk usage; SQLite and uploads share the same persistent data root.

## Homelab vs VPS decision checklist

Choose **homelab** if:

- You have a public IPv4 or working IPv6, not CGNAT.
- You can forward ports 80/443 and LiveKit media ports.
- You are comfortable maintaining router/firewall/DNS and power/internet uptime.
- You can provide off-site backups.

Choose **VPS** if:

- Your ISP blocks inbound ports or uses CGNAT.
- You need simpler DNS/firewall/public-IP behavior.
- You want voice/video to work for users outside your LAN with fewer NAT surprises.
- You want better uptime than a home connection.

A hybrid is also reasonable: host the API on the homelab but use LiveKit Cloud, or host both API and LiveKit on a small VPS while keeping backups to the homelab.

## References

- GitHub Pages custom domains: https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site
- GitHub Pages publishing sources / Actions: https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site
- Docker Engine on Ubuntu: https://docs.docker.com/engine/install/ubuntu/
- Caddy automatic HTTPS: https://caddyserver.com/docs/automatic-https
- Caddy `reverse_proxy`: https://caddyserver.com/docs/caddyfile/directives/reverse_proxy
- LiveKit self-hosting deployment: https://docs.livekit.io/transport/self-hosting/deployment/
- LiveKit ports/firewall: https://docs.livekit.io/transport/self-hosting/ports-firewall/
- LiveKit sample config: https://raw.githubusercontent.com/livekit/livekit/master/config-sample.yaml
