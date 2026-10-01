# Working on the server

Read [the LLM-facing overview](../llm-docs/server/OVERVIEW.md) for the code layout and verification commands, and [architecture guide](../llm-docs/server/ARCHITECTURE.md) for extension seams. `README.md` is human-owned; do not edit it. Keep generated documentation under the repository-root `llm-docs/` directory.

Keep feature-specific HTTP handlers, request/response types, and operations together under the feature directory. Shared HTTP middleware and error responses belong in `src/http/`; reusable app wiring belongs in `src/lib.rs`, process startup in `src/main.rs`. When changing routes or DTO schemas, update the OpenAPI assembly and the route inventory test, then run `cargo test` from this directory.
