## Documentation ownership

- Every `README.md`, in every directory except for .agents, is human-authored documentation for humans. Do not create, edit, regenerate, or overwrite README files.
- Keep all LLM-generated project documentation under `llm-docs/` at the repository root. Use `OVERVIEW.md`, not `README.md`, for LLM-facing overviews. Component documentation lives in `llm-docs/client/` and `llm-docs/server/`.
- Human-authored documentation outside `llm-docs/` is read-only to agents. If it needs a change, describe the proposed change to the user rather than editing it.
- `AGENTS.md` files and skill definitions/support files under `.agents/skills/` are agent configuration, not human documentation; keep them in their discoverable locations and maintain them as needed. This exception does not include any `README.md`.
- Skills use repository-root `llm-docs/CONTEXT.md` for the glossary, `llm-docs/adr/` for ADRs, `llm-docs/agents/` for workflow guidance, and `llm-docs/out-of-scope/` for rejection records. Write new research, plans, specs, and other generated documentation under `llm-docs/`. Treat paths with placeholders as templates for future outputs, not links to existing files.
- Commands and source paths in LLM documentation are relative to the repository root or the explicitly named component directory, not the documentation directory.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues. See `llm-docs/agents/issue-tracker.md`.

### Triage labels

Use the five default triage labels. See `llm-docs/agents/triage-labels.md`.

### Domain docs

Use a single-context layout. See `llm-docs/agents/domain.md`.
