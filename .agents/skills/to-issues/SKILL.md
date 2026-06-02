---
name: to-issues
description: Break a plan, spec, or PRD into a project-tagged set of independently-grabbable tracer-bullet issues and create them with the `it` issue tracker. Use when user wants to convert a plan into issues, create implementation tickets, or break down work into issues.
---

# To Issues

Break a plan into a project-tagged set of independently-grabbable issues using vertical slices (tracer bullets), then create the approved issues with Hamlet's central `it` issue tracker.

`it` is a tiny markdown-backed tracker. Use it as the source of truth for issue files; do not hand-write, rename, move, or archive issue Markdown files unless the user explicitly asks.

Issue tracking is intentionally outside every git worktree. The single source of truth for Hamlet tracker issues is `$HOME/.issues/hamlet`. At the start of the workflow, set `TRACKER_DIR="$HOME/.issues/hamlet"` and confirm it exists. Never create or use a repo-local `issues/` directory for this workflow.

A "project" is represented by one shared tag on every issue in the set. Use a short kebab-case project tag prefixed with `project-`, for example `project-native-notifications`. All issues created from the same plan must include this shared project tag.

## `it` commands

Use these commands with the central tracker directory:

```bash
TRACKER_DIR="$HOME/.issues/hamlet"
it list --dir "$TRACKER_DIR"
it list --dir "$TRACKER_DIR" --tag project-example
it show <id> --dir "$TRACKER_DIR"
it create "<name>" --dir "$TRACKER_DIR" --tags "project-example,tag-a,tag-b" --blocked-by "1,2" --body "$body"
it status <id> completed --dir "$TRACKER_DIR"
```

Notes:

- `it create` writes `$HOME/.issues/hamlet/<id>-<slug>.md` and owns the frontmatter (`id`, `name`, `status`, `tags`, `blocked_by`). Do not duplicate that frontmatter in the body.
- Omit `--blocked-by` when there are no blockers.
- Tags are comma-separated. Every issue in a generated set must include the same `project-<slug>` tag. Also include short kebab-case area tags such as `server` or `client`, and slice type tags (`afk` or `hitl`). Follow existing tag conventions when they are clear.
- For multiline bodies, build a shell variable or temporary file, then pass it to `--body`; do not try to manually create the issue file.
- If `it` is unavailable or errors in a way you cannot resolve, stop and ask the user how to proceed instead of falling back to manual issue files.

## Process

### 1. Gather context

Work from whatever is already in the conversation context. If the user passes a source reference (issue ID, URL, or path) as an argument, read its full body and comments when accessible.

For existing tracked issues, use `it show <id> --dir "$TRACKER_DIR"`. For the issue queue and tag conventions, use `it list --dir "$TRACKER_DIR"` before drafting. If the work belongs to an existing project, identify its `project-<slug>` tag and inspect it with `it list --dir "$TRACKER_DIR" --tag <project-tag>`. If the source cannot be fetched directly, ask the user to provide the missing content.

### 2. Explore the codebase (optional)

If you have not already explored the codebase, do so to understand the current state of the code. Issue titles and descriptions should use the project's domain glossary vocabulary and respect ADRs in the area you're touching.

### 3. Choose the project tag

Derive a concise project tag from the plan/source title, unless the user provides one. Use `project-<short-kebab-case-topic>`; avoid generic tags such as `project-feature` or `project-refactor`.

Check whether that tag already exists with `it list --dir "$TRACKER_DIR" --tag <project-tag>`. If it exists, decide whether the new issues should join that project or use a more specific tag. Include the proposed project tag when asking the user to approve the breakdown.

### 4. Draft vertical slices

Break the plan into **tracer bullet** issues. Each issue is a thin vertical slice that cuts through ALL integration layers end-to-end, NOT a horizontal slice of one layer.

Slices may be **HITL** or **AFK**. HITL slices require human interaction, such as an architectural decision or a design review. AFK slices can be implemented and merged without human interaction. Prefer AFK over HITL where possible.

<vertical-slice-rules>
- Each slice delivers a narrow but COMPLETE path through every relevant layer (for example schema, API, UI, tests)
- A completed slice is demoable or verifiable on its own
- Prefer many thin slices over few thick ones
- Avoid duplicate issues by checking the current tracker state first
</vertical-slice-rules>

Use provisional labels while drafting, such as `Slice A` or `Draft 1`. Do not promise tracker IDs until after `it create` returns them.

### 5. Quiz the user

Present the proposed project tag and breakdown as a numbered list. For each slice, show:

- **Title**: short descriptive name
- **Type**: HITL / AFK
- **Project tag**: the shared `project-<slug>` tag for the issue set
- **Suggested tags**: comma-separated tags to pass to `it create`, including the project tag
- **Blocked by**: provisional labels for other new slices, or existing tracker IDs for existing blockers
- **User stories covered**: which user stories this addresses (if the source material has them)

Ask the user:

- Does the granularity feel right? (too coarse / too fine)
- Are the dependency relationships correct?
- Should any slices be merged or split further?
- Are the correct slices marked as HITL and AFK?
- Is the project tag right, and should these issues join an existing project tag instead?
- Are the suggested tags right?

Iterate until the user approves the breakdown. Do not create issues before approval.

### 6. Create approved issues with `it`

Create one tracker issue per approved slice using `it create`. Every created issue must include the approved shared project tag. Do not write the issue set as a single combined document. Do not place issue files in `docs/`. Do not manually write files into repo-local `issues/` directories.

Create issues in dependency order (blockers first). Let `it` assign real IDs, then maintain a mapping from provisional labels to created tracker IDs. When creating a later issue, translate its blockers to real IDs and pass them with `--blocked-by "1,2"`.

If a dependency cycle prevents topological creation, stop and ask the user to resolve the dependency graph.

Each issue body should use this Markdown template. The frontmatter is created by `it`; include only the body below.

<issue-body-template>
# <Short descriptive title>

**Type**: HITL / AFK

**User stories covered**: List the source user stories this issue addresses, or "Not specified".

## Parent

A reference to the parent source (if the source was an existing issue, PRD, or spec; otherwise omit this section).

## What to build

A concise description of this vertical slice. Describe the end-to-end behavior, not layer-by-layer implementation.

Avoid specific file paths or code snippets — they go stale fast. Exception: if a prototype produced a snippet that encodes a decision more precisely than prose can (state machine, reducer, schema, type shape), inline it here and note briefly that it came from a prototype. Trim to the decision-rich parts — not a working demo, just the important bits.

## Acceptance criteria

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3

## Blocked by

- Tracker issue ID(s), e.g. `#12`, that must complete first

Or "None - can start immediately" if no blockers.
</issue-body-template>

Example creation pattern:

```bash
TRACKER_DIR="$HOME/.issues/hamlet"

body=$(cat <<'EOF'
# Short descriptive title

**Type**: AFK

**User stories covered**: Not specified.

## What to build

Build the narrow end-to-end behavior.

## Acceptance criteria

- [ ] The behavior works from the user's perspective.
- [ ] Automated coverage verifies the behavior.

## Blocked by

None - can start immediately
EOF
)

it create "Short descriptive title" --dir "$TRACKER_DIR" --tags "project-short-topic,afk,client" --body "$body"
```

After creating all issues, run `it list --dir "$TRACKER_DIR"` or `it show <id> --dir "$TRACKER_DIR"` as needed to verify the IDs, tags, statuses, and blockers.

### 7. Report the created issues

Reply with the project tag and a concise summary of the created issues in dependency order:

Project: `project-<slug>`

- `#<id>` — title — tags — blocked by

If the user asks to mark an issue complete, use `it status <id> completed --dir "$TRACKER_DIR"`; do not move files between directories.
