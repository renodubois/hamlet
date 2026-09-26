# Working on server-rewrite

Read [README.md](README.md) for the current code layout and verification commands. The tree in `docs/plans/2026-09-23-server-rewrite-prototype.md` is a historical, illustrative plan; use the implemented feature layout as the guide for new code.

Keep feature-specific HTTP handlers, request/response types, and operations together under the feature directory. Shared HTTP middleware and error responses belong in `src/http/`; reusable app wiring belongs in `src/lib.rs`, process startup in `src/main.rs`. When changing routes or DTO schemas, update the OpenAPI assembly and the route inventory test, then run `cargo test` from this directory.
