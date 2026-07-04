# Persistent SQLite durability smoke QA

Use this checklist when validating that Hamlet data survives a server restart against the same file-backed SQLite database and upload directories.

## Local `cargo run` restart smoke

1. From a worktree, load its ports first:
   ```bash
   source .hamlet-worktree.env
   ```
2. Start the server with an isolated file-backed data directory:
   ```bash
   export HAMLET_DATA_DIR="$(mktemp -d /tmp/hamlet-persistence-smoke.XXXXXX)"
   export HAMLET_DATABASE_URL="sqlite://$HAMLET_DATA_DIR/hamlet.db?mode=rwc"
   export HAMLET_UPLOADS_DIR="$HAMLET_DATA_DIR/uploads"
   export HAMLET_MESSAGE_ATTACHMENTS_DIR="$HAMLET_DATA_DIR/private-uploads/message-attachments"
   export HAMLET_SEED_DEV_DATA=false
   cd server
   cargo run
   ```
3. In the Electron client or via API calls, create durable user-visible data:
   - register/login a user and keep the session cookie;
   - upload an avatar;
   - create a custom emoji;
   - create a channel;
   - send a normal message, an inline reply or thread reply, and a photo attachment;
   - add a native reaction and a custom-emoji reaction.
4. Record the channel/message/reply/thread/attachment/emoji ids and the returned avatar, emoji, attachment, and thumbnail URLs.
5. Stop the server with `Ctrl-C`, then run `cargo run` again with the same environment.
6. Verify after restart:
   - the old session cookie still authorizes `GET /me`;
   - `GET /channels`, `GET /messages/{channel_id}`, and `GET /thread/{root_message_id}` show the same channel, messages, reply/thread metadata, embeds, reactions, and custom emoji metadata;
   - the avatar URL and emoji URL still return image bytes;
   - `GET /attachments/{attachment_id}` and `/thumbnail` still return the uploaded photo bytes while the files remain in `HAMLET_MESSAGE_ATTACHMENTS_DIR`.

## Docker Compose persistence and reset smoke

1. From the repo root/worktree, load ports and generate the worktree LiveKit config when using non-default ports:
   ```bash
   source .hamlet-worktree.env
   cd server
   ./scripts/write-livekit-config.sh
   ```
2. Start Compose:
   ```bash
   docker compose up --build
   ```
3. Repeat the local data-creation smoke through the client/API.
4. Restart without deleting volumes:
   ```bash
   docker compose restart server
   # or: docker compose down && docker compose up
   ```
5. Verify the same session, messages, uploads, and custom emoji remain available.
6. Reset the application data volume:
   ```bash
   docker compose down -v
   docker compose up --build
   ```
7. Verify the previous session/data/uploads are gone. A default dev Compose run should only recreate bootstrap channels and explicit dev seed data.

## Server check commands

Run before merging persistence changes when practical:

```bash
cd server
cargo fmt
cargo clippy -- -D warnings
cargo test
cd ..
./scripts/check.sh server
```
