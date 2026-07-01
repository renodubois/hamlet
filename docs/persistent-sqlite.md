# Persistent SQLite operations

Hamlet's server uses SQLite as durable application state by default. Treat the
SQLite file, its WAL sidecars, uploads, sessions, channels, messages, custom
emoji, and attachments as persistent data unless you intentionally run against an
in-memory database or remove the data directory.

## Defaults and environment controls

When `DATABASE_URL` is unset or empty, `Config::from_env` builds a file-backed
SQLite URL for `hamlet.db` under the local application data directory:

- `HAMLET_DATA_DIR`, when set, is used directly.
- Linux/BSD: `$XDG_DATA_HOME/hamlet` or `~/.local/share/hamlet`.
- macOS: `~/Library/Application Support/Hamlet`.
- Windows: `%LOCALAPPDATA%\Hamlet` or `%USERPROFILE%\AppData\Local\Hamlet`.
- If no platform location is available, Hamlet falls back to `.hamlet-data` in
  the current working directory.

Set `DATABASE_URL` to override the database completely. Only `sqlite:` URLs are
supported. File-backed URLs should include `?mode=rwc`, for example
`sqlite://data/hamlet.db?mode=rwc` or
`sqlite:///var/lib/hamlet/hamlet.db?mode=rwc`. Explicit in-memory URLs are still
supported for tests and clean-room runs; prefer a named shared-memory URL such as
`sqlite:file:hamlet_clean_room?mode=memory&cache=shared`.

Useful server environment flags:

| Variable | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | derived from the data dir | Full SQLite URL override. |
| `HAMLET_DATA_DIR` | platform app-data dir | Root for the default `hamlet.db` path. |
| `HAMLET_BOOTSTRAP_DEFAULT_CHANNELS` | `true` | Create `general` text and `voice` voice channels when the channel table is empty. |
| `HAMLET_SEED_DEV_DATA` | `true` in debug builds, `false` in release builds | Seed local dev users and a fixed dev session token. |
| `HAMLET_UPLOADS_DIR` | `./uploads` | Public upload storage. Compose points this at `/var/lib/hamlet/uploads`. |
| `HAMLET_MESSAGE_ATTACHMENTS_DIR` | `./private-uploads/message-attachments` | Private message attachment storage. Compose points this under `/var/lib/hamlet`. |

`server/.env.example` shows local values. Docker Compose sets a release-shaped
server environment in `server/docker-compose.yml` and the development override
explicitly opts back into dev seed data.

## Startup, bootstrap, and seed behavior

Every server startup connects to SQLite and initializes the baseline schema
before HTTP routes are served. After schema initialization:

1. Default channel bootstrap runs when `HAMLET_BOOTSTRAP_DEFAULT_CHANNELS` is
   enabled. It inserts `general` and `voice` only if the channel table is empty;
   existing channel lists are left authoritative and are not repaired or
   duplicated.
2. Development seed data runs only when `HAMLET_SEED_DEV_DATA` is enabled. It
   ensures `baipas` / `password` and `teo` / `password`, a placeholder avatar for
   `baipas`, and a long-lived fixed `baipas` session token that is printed in the
   logs for quick local login.

Release-shaped defaults disable development users. Operators who want a clean
workspace with no seeded accounts should run with `HAMLET_SEED_DEV_DATA=false`
and create real users through registration/login flows or the
[admin account creation CLI](admin-account-creation.md).

## Admin account provisioning

Server operators can create a password-backed user without exposing a remote
admin endpoint by running the `hamlet-admin` CLI against the same `DATABASE_URL`
or `HAMLET_DATA_DIR` used by the server:

```bash
cd server
cargo run --bin hamlet-admin -- create-user \
  --username alice \
  --password 'temporary-password'
```

See [Admin account creation](admin-account-creation.md) for usage, Docker notes,
and error behavior.

## Schema changes and migrations

Persistent SQLite means schema changes must preserve existing databases. The
current startup path uses SeaORM's schema registry for the baseline schema and
then applies explicit, idempotent SQLite migration steps in
`server/src/database.rs` for integrity fixes and indexes.

For future schema changes:

- Add a migration-shaped step (or a dedicated migration framework) instead of
  relying on operators to delete their database.
- Make the step idempotent, transactional where possible, and safe for rows that
  already exist in durable local and Docker volumes.
- Validate invariants before adding constraints; return a clear startup error
  when existing data cannot be migrated automatically.
- Add tests that exercise both fresh databases and an already-initialized
  database containing representative persisted rows.
- Do not use ad hoc schema sync as the only plan for data transforms, backfills,
  uniqueness changes, or destructive column/table changes.

## Reset workflows

Stop the server before deleting SQLite files.

### Local file-backed reset

If you set an explicit local data directory, remove that directory and any upload
roots you want to clear:

```bash
cd server
rm -rf ./data ./uploads ./private-uploads
```

If you used the platform default, remove the `hamlet`/`Hamlet` app-data directory
listed above. For a database-only reset, delete `hamlet.db` and all SQLite
sidecars beside it (`hamlet.db-wal`, `hamlet.db-shm`, and any `hamlet.db-*`
files). For a full app-state reset, remove the data directory plus upload roots.

### Explicit in-memory clean-room run

Use an in-memory URL when you want state to disappear on process exit without
touching local files:

```bash
cd server
DATABASE_URL='sqlite:file:hamlet_clean_room?mode=memory&cache=shared' \
HAMLET_SEED_DEV_DATA=false \
cargo run
```

Leave `HAMLET_BOOTSTRAP_DEFAULT_CHANNELS=true` if the clean-room server should
still start with the built-in `general` and `voice` channels.

### Docker Compose reset

The Compose server stores SQLite, uploads, and private uploads together in the
`hamlet_data` named volume mounted at `/var/lib/hamlet`.

```bash
cd server
docker compose down      # stop containers; keep the hamlet_data volume
docker compose down -v   # stop containers and remove named volumes, including app data
```

`down -v` also removes the development cargo-cache volumes declared by the
override file. To wipe only application data, stop Compose and remove the
project's `hamlet_data` volume shown by `docker volume ls` (commonly
`server_hamlet_data` or `<compose-project>_hamlet_data`).

## Repository hygiene

Local SQLite databases and sidecars (`*.db`, `*.db-*`, `*.sqlite*`), local data
directories (`data/`, `.hamlet-data/`, `hamlet-data/`), upload roots, `.env`
files, and generated `livekit.local*.yaml` files are ignored by git. The same
local database/data artifacts are excluded from the server Docker build context
by `server/.dockerignore`, so a local database cannot be baked into an image by
accident.
