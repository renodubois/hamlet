---
name: implement-issues
description: Find all `it` tracker issues for a project tag, plan a dependency-safe implementation order with parallel waves, and launch worker subagents to implement ready issues. Use after `to-issues` when the user wants project-tagged issues implemented automatically.
---

# Implement Issues

Implement a project-tagged set of `it` tracker issues using dependency-aware parallel execution.

`it` is the source of truth for issue files. Do not hand-write, rename, move, archive, or directly edit issue Markdown files unless the user explicitly asks. The only tracker mutation this skill normally performs is marking finished issues completed with `it status <id> completed --dir issues`.

## Inputs

The user should provide a project name or tag, for example:

- `project-native-notifications`
- `native notifications` (normalize to `project-native-notifications`)
- any exact non-project tag if the repository uses one

If no project name/tag is provided, ask for one before continuing.

## `it` commands

Run these from the repository root unless the user specifies another issue directory:

```bash
it list --dir issues
it list --dir issues --tag <tag>
it list --dir issues --tag <tag> --status in-progress
it list --dir issues --tag <tag> --status completed
it show <id> --dir issues
it status <id> completed --dir issues
```

If `it` is unavailable or errors in a way you cannot resolve, stop and ask the user how to proceed. Do not fall back to manual issue file edits.

## Process

### 1. Resolve the project tag

1. Normalize the user's input into candidate tags:
   - exact input if it already looks like a tag
   - `project-<kebab-case-input>` for ordinary project names
   - exact kebab-case input as a fallback for older issue sets
2. Use `it list --dir issues --tag <candidate>` to find the matching issue set.
3. If exactly one candidate has issues, use it.
4. If none or multiple candidates match, ask the user to choose the intended tag.

### 2. Load issue context

For every issue in the selected tag, run `it show <id> --dir issues` and capture:

- ID, title, status, tags, and `blocked_by`
- `Type`: AFK / HITL, from tags and/or body
- acceptance criteria
- parent/source reference
- any explicit implementation notes

Skip issues already marked `completed`, but keep them in the dependency graph as satisfied blockers.

### 3. Build the dependency graph

Create a graph over all issues in the project tag.

- An uncompleted issue is ready only when every blocker is either:
  - a completed issue, or
  - an issue scheduled in an earlier wave.
- If a blocker is outside the selected project, inspect it with `it show`; it must already be completed before the dependent issue can run.
- If a blocker ID is missing or unreadable, stop and ask the user how to proceed.
- If uncompleted project issues contain a dependency cycle, stop and ask the user to resolve the cycle.
- Treat HITL issues as not delegateable by default. Ask the user for the required decision/input, complete that interaction in the main agent, and only mark the HITL issue complete when its acceptance criteria are actually satisfied.

Topologically sort the AFK issues into parallel waves. Issues in the same wave must not depend on each other.

### 4. Announce the implementation order

Before launching workers, print a concise plan:

```text
Project: <tag>
Wave 1: #1 Title, #2 Title (parallel)
Wave 2: #3 Title (after #1/#2)
Blocked/HITL: #4 Title — needs human decision
Already completed: #5 Title
```

If the user asked only for a plan, stop here. Otherwise proceed without an additional approval prompt unless there are ambiguities, HITL issues, destructive changes, dirty unrelated worktree changes, or dependency problems.

### 5. Launch all available worker subagents per wave

For each wave, maximize parallelism:

- Launch one `worker` subagent per ready AFK issue.
- Use the subagent tool's parallel mode.
- Set concurrency to the maximum available (currently 8). If a wave has more ready issues than the limit, batch them at that concurrency.
- Do not invent filler work when there are fewer ready issues than available subagent slots.
- Do not put dependent issues in the same wave.

Before launching a wave, run `git status --short`. If there are unrelated user changes you might overwrite, ask before continuing. Changes from earlier waves in this same run are expected.

Give each worker enough context to work independently. Use this task template and fill in the issue-specific details:

```text
Implement tracker issue #<id>: <title>

Project tag: <project-tag>
Issue context:
<paste the full `it show <id> --dir issues` output or a faithful summary including acceptance criteria and blockers>

Instructions:
- Work in the repository root.
- Read AGENTS.md and any relevant subdirectory CLAUDE.md before editing.
- Keep the change scoped to this issue's acceptance criteria.
- Preserve behavior outside this slice unless the issue requires it.
- Add or update tests for the behavior you implement.
- Run the relevant formatter/lint/typecheck/test commands for touched areas when practical.
- Do not commit, stash, reset, checkout branches, rebase, push, or edit issue tracker files.
- Do not mark the issue completed; the parent agent will validate and update status.
- If blocked by missing decisions, failing dependencies, merge conflicts, or unclear requirements, stop and report the blocker with what you tried.

Return:
- Summary
- Files changed
- Acceptance criteria satisfied
- Tests/checks run and results
- Any blockers or follow-up needed
```

### 6. Validate each wave before continuing

After a wave finishes:

1. Read every subagent report.
2. Inspect `git status --short` and relevant diffs.
3. Resolve integration conflicts or incomplete work in the main agent, or launch follow-up workers for clearly scoped fixes.
4. Run relevant checks for the areas touched. Follow repository instructions: if a check fails, fix it even if the failure predates the change.
5. Only after validation, mark completed issues with:

   ```bash
   it status <id> completed --dir issues
   ```

6. Do not schedule dependent issues until their blockers have been validated and marked completed.

If an issue fails or remains blocked, leave it `in-progress`, report why, and do not run dependent issues unless their blockers are otherwise satisfied.

### 7. Final verification and report

After all runnable waves finish:

1. Run the appropriate final project checks for all touched sides. Use the repo's aggregate check script if practical.
2. Re-run `it list --dir issues --tag <project-tag>` to verify statuses.
3. Report concisely:
   - project tag
   - completed issues
   - issues left blocked or HITL, with reasons
   - files/areas changed
   - checks/tests run

Do not claim an issue is complete unless its tracker status is `completed` and its acceptance criteria were verified.
