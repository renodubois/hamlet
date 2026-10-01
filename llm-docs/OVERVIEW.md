# Hamlet

Self-hosted chat for you and your friends.

## Repository layout

- [`server/`](server/OVERVIEW.md): Rust/Actix HTTP API with SQLite storage and a checked-in OpenAPI contract.
- [`client/`](client/OVERVIEW.md): Linux desktop client built with GPUI.
- [`CONTEXT.md`](CONTEXT.md): domain vocabulary.
- [`llm-docs/agents/`](agents/): contributor workflow guidance.

## Run locally

Install Rust and the [client's native Linux dependencies](client/OVERVIEW.md#linux-prerequisites), then use two terminals:

```sh
# Terminal 1
cd server
HAMLET_BIND=127.0.0.1:8081 cargo run --locked --bin hamlet

# Terminal 2, in a graphical Linux session
cd client
cargo run --locked
```

The client defaults to `http://127.0.0.1:8081`. Without `HAMLET_BIND`, the server binds to `127.0.0.1:3001`. Its database defaults to `server/data/hamlet.db` when run as above.

Signup is open and all authenticated users can access all channels. Review admission, TLS termination and rate limiting before exposing the server beyond local development. The client may access the configured Linux Secret Service to save sessions; use the [verification safety gate](client/VERIFY.md#native-safety-gate) for disposable desktop tests.

See the component LLM-facing overviews for verification commands and architecture guides. All command and source paths above are relative to the repository root.
