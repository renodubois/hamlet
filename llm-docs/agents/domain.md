# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`llm-docs/CONTEXT.md`** at the repo root.
- **`llm-docs/adr/OVERVIEW.md`**: the ADR directory convention. Read any numbered ADRs in `llm-docs/adr/` that touch the area you're about to work in.

If there are no relevant ADRs, proceed silently. The `/domain-modeling` skill creates decisions only when they actually get resolved.

## File structure

Single-context repo:

```
llm-docs/
├── CONTEXT.md
└── adr/
    └── OVERVIEW.md
```

Add numbered ADRs only as decisions are recorded; do not create component glossaries or context maps.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `llm-docs/CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders), but worth reopening because…_
