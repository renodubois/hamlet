## Documentation ownership

- Every `README.md`, in every directory except for .agents, is human-authored documentation for humans. Do not create, edit, regenerate, or overwrite README files.
- Keep all LLM-generated project documentation under `llm-docs/` at the repository root. Use `OVERVIEW.md`, not `README.md`, for LLM-facing overviews. Component documentation lives in `llm-docs/client/` and `llm-docs/server/`.
- Human-authored documentation outside `llm-docs/` is read-only to agents. If it needs a change, describe the proposed change to the user rather than editing it.
- `AGENTS.md` files and skill definitions/support files under `.agents/skills/` are agent configuration, not human documentation; keep them in their discoverable locations and maintain them as needed. This exception does not include any `README.md`.
- Skills use repository-root `llm-docs/CONTEXT.md` for the glossary, `llm-docs/adr/` for ADRs, `llm-docs/agents/` for workflow guidance, and `llm-docs/out-of-scope/` for rejection records. Write new research, plans, specs, and other generated documentation under `llm-docs/`. Treat paths with placeholders as templates for future outputs, not links to existing files.
- Commands and source paths in LLM documentation are relative to the repository root or the explicitly named component directory, not the documentation directory.

## Worktree development environments

When running or interacting with a development environment, use an isolated environment for the current worktree. Do not use the client's default shared configuration or another worktree's server/database.

- Choose a stable, unused loopback port for this worktree (for example, `8082`; other worktrees might use `8083`, `8084`, etc.). Coordinate with other active worktrees and reuse the chosen port across restarts. Do not assume the example port is available or stop another worktree's server to free it.
- Run the following commands from this worktree's repository root, in separate terminals. Replace `8082` with this worktree's chosen port:

```sh
# Server: running from server/ keeps the default database worktree-local.
(cd server && HAMLET_BIND=127.0.0.1:8082 \
  cargo run --locked --bin hamlet)

# Client: use worktree-local configuration, not ~/.config/hamlet.
(cd client && XDG_CONFIG_HOME="$PWD/.env.dev-config" \
  cargo run --locked)
```

- Set the client's Server URL to the matching endpoint, e.g. `http://127.0.0.1:8082`. When making API requests directly, use this endpoint too.
- The default database is this worktree's `server/data/hamlet.db`. Ensure an inherited `HAMLET_DATABASE_URL` does not redirect it to shared storage; unset it for the default local database or explicitly point it at a database owned by this worktree.
- Client configuration lives at `client/.env.dev-config/hamlet/session.json`; `.env.*` is already Git-ignored. Start with fresh configuration; never copy `session.json` or pending deletion records from another environment.
- Different server URLs, including ports, produce different Secret Service credential keys. The keyring itself remains shared: `XDG_CONFIG_HOME` alone does not isolate credentials. Keep each environment on its own endpoint, and do not point these clients at another environment's endpoint.
- These commands are isolation instructions, not permission to launch desktop automation or access a real keyring. Obtain separate consent and follow `llm-docs/client/VERIFY.md`'s native safety gate before doing either. Ordinary builds and automated tests do not require launching the client.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues. See `llm-docs/agents/issue-tracker.md`.

### Triage labels

Use the five default triage labels. See `llm-docs/agents/triage-labels.md`.

### Domain docs

Use a single-context layout. See `llm-docs/agents/domain.md`.
