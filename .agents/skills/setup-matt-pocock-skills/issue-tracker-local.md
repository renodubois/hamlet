# Issue tracker: Local Markdown

Optional local-markdown tracker template. This repo currently uses GitHub Issues, configured in repository-root `llm-docs/agents/issue-tracker.md`. Use the conventions below only if the tracker is changed to local markdown.

Store local issues and specs under repository-root `llm-docs/`. Paths containing angle-bracket placeholders below are output templates, not references to existing files; create their directories when publishing.

## Conventions

- One feature per directory: `llm-docs/<feature-slug>/`
- The spec is `llm-docs/<feature-slug>/spec.md`
- Implementation issues are one file per ticket at `llm-docs/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01`, never a single combined tickets file
- Triage state is recorded as a `Status:` line near the top of each issue file (see repository-root `llm-docs/agents/triage-labels.md` for the role strings)
- Comments and conversation history append to the bottom of the file under a `## Comments` heading

## When a skill says "publish to the issue tracker"

Create a new file under `llm-docs/<feature-slug>/` (creating the directory if needed).

## When a skill says "fetch the relevant ticket"

Read the file at the referenced path. The user will normally pass the path or the issue number directly.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a file with one **child** file per ticket.

- **Map**: `llm-docs/<effort>/map.md` (the Notes / Decisions-so-far / Fog body).
- **Child ticket**: `llm-docs/<effort>/issues/<NN>-<slug>.md`, numbered from `01`, with the question in the body. A `Type:` line records the ticket type (`research`/`prototype`/`grilling`/`task`); a `Status:` line records `claimed`/`resolved`.
- **Blocking**: a `Blocked by: NN, NN` line near the top. A ticket is unblocked when every file it lists is `resolved`.
- **Frontier**: scan `llm-docs/<effort>/issues/` for files that are open, unblocked, and unclaimed; first by number wins.
- **Claim**: set `Status: claimed` and save before any work.
- **Resolve**: append the answer under an `## Answer` heading, set `Status: resolved`, then append a context pointer (gist + link) to the map's Decisions-so-far in `llm-docs/<effort>/map.md`.
