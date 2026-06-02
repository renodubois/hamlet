---
name: implement-issues
description: Find all `it` tracker issues for a project tag, plan a dependency-safe implementation order with parallel waves, and launch worker subagents to implement ready issues. Use after `to-issues` when the user wants project-tagged issues implemented automatically.
---

# Implement Issues

Implement a project-tagged set of `it` tracker issues using dependency-aware parallel execution.

## Integration worktree and central tracker rule

The worktree where the user starts Pi and invokes this skill is the **integration worktree** for the run. All implementation branches created by this skill are merged back into the current branch of that integration worktree, not into `/home/reno/projects/hamlet` unless that is the current worktree.

Issue tracking is intentionally **outside every git worktree**. The single source of truth for Hamlet tracker issues is:

```bash
TRACKER_DIR="$HOME/.issues/hamlet"
```

At the start of every run:

1. Record `INTEGRATION_REPO=$(git rev-parse --show-toplevel)` from the current working directory.
2. Record the current branch/ref in `INTEGRATION_REPO`; this is the **main working branch** for this run, even if it is a feature branch such as `feature/emoji-autocomplete`.
3. Set `TRACKER_DIR="$HOME/.issues/hamlet"` and run all tracker reads/writes against that directory.
4. Confirm `TRACKER_DIR` exists before reading or mutating tracker state. If it is missing, stop and ask the user rather than recreating or switching to a repo-local `issues/` directory.

Do not assume `/home/reno/projects/hamlet` is the integration target. It is only a repository worktree path, and must not receive implementation commits or merges unless the user actually invoked the skill from there.

`it` is the source of truth for issue files. Do not hand-write, rename, move, archive, or directly edit issue Markdown files unless the user explicitly asks. The only tracker mutation this skill normally performs is marking finished issues completed with `it status <id> completed --dir "$TRACKER_DIR"`.

Never use a repo-local `issues/` directory for this workflow. If a worktree contains one, treat it as stale/incorrect and do not read or edit it.

## Inputs

The user should provide a project name or tag, for example:

- `project-native-notifications`
- `native notifications` (normalize to `project-native-notifications`)
- any exact non-project tag if the repository uses one

If no project name/tag is provided, ask for one before continuing.

## `it` commands

Run these with the central tracker directory. Never point `it` at a worktree-local `issues/` directory:

```bash
TRACKER_DIR="$HOME/.issues/hamlet"
it list --dir "$TRACKER_DIR"
it list --dir "$TRACKER_DIR" --tag <tag>
it list --dir "$TRACKER_DIR" --tag <tag> --status in-progress
it list --dir "$TRACKER_DIR" --tag <tag> --status completed
it show <id> --dir "$TRACKER_DIR"
it status <id> completed --dir "$TRACKER_DIR"
```

If `it` is unavailable or errors in a way you cannot resolve, stop and ask the user how to proceed. Do not fall back to manual issue file edits.

## Worktree and port isolation

Every AFK issue must be implemented in its own script-created worktree, never directly in the integration worktree. Issue tracker state remains pinned to the central `TRACKER_DIR`; do not copy, sync, mutate, or rely on issue files inside any git worktree.

- Treat `INTEGRATION_REPO` as the integration worktree and record its current branch/ref before launching workers. This is the "main working branch" for the run, even when it is a feature branch.
- Use `scripts/create-worktree.sh` from `INTEGRATION_REPO` whenever creating or refreshing an issue worktree. Do not run the script from `/home/reno/projects/hamlet` unless that path is also `INTEGRATION_REPO`. Do not hand-create worktrees with `git worktree add` for this workflow.
- Create one issue branch/worktree per ready issue, using a branch name such as `issue/<id>-<kebab-title>`, for example:

  ```bash
  cd "$INTEGRATION_REPO"
  scripts/create-worktree.sh --base-ref <main-working-branch> issue/<id>-<kebab-title>
  ```

  The script creates the worktree under `~/projects/hamlet-wt/` by default, checks both the persistent SQLite allocation DB and currently bound system ports, reserves a non-overlapping port set, and writes `.hamlet-worktree.env`, `client/.env.local`, `server/.env`, and `server/livekit.local.yaml`.
- If a different root is explicitly needed, set `HAMLET_WORKTREE_ROOT`, but keep all issue worktrees grouped under one dedicated worktree directory rather than as siblings of the integration repo. If a different port DB is explicitly needed, set `HAMLET_WORKTREE_PORT_DB`; otherwise use the script default.
- Before assigning a worktree to an issue, confirm that `.hamlet-worktree.env` exists. You do not need to pre-check a static port plan; `scripts/create-worktree.sh` chooses ports from the DB and live system state. If worktree creation fails because no port slot is available, inspect with `scripts/create-worktree.sh --list-port-allocations`, optionally remove stale entries with `--prune-port-allocations`, or ask the user how to proceed.
- Never share one worktree between simultaneous issues. A worktree may be reused across issues only after the previous issue has been validated, committed, merged back to the main working branch in `INTEGRATION_REPO`, and the worktree has been hard-reset/cleaned back to the latest main working branch.
- Launch each worker subagent with `cwd` set to its assigned worktree root. Tell the worker to source `.hamlet-worktree.env` before running server/client commands that need app ports.
- Workers do not merge or update tracker status. The parent agent validates the worktree, creates an implementation commit there, merges that worktree branch into the main working branch in `INTEGRATION_REPO`, then marks the issue complete against the central `TRACKER_DIR`.
- If you remove a worktree after merging, release its reserved ports with `scripts/create-worktree.sh --release-port-allocation <worktree-dir>` from `INTEGRATION_REPO`. If you keep the worktree for reuse, keep its allocation reserved.

## Process

### 1. Resolve the project tag

1. Normalize the user's input into candidate tags:
   - exact input if it already looks like a tag
   - `project-<kebab-case-input>` for ordinary project names
   - exact kebab-case input as a fallback for older issue sets
2. Use `it list --dir "$TRACKER_DIR" --tag <candidate>` to find the matching issue set.
3. If exactly one candidate has issues, use it.
4. If none or multiple candidates match, ask the user to choose the intended tag.

### 2. Load issue context

For every issue in the selected tag, run `it show <id> --dir "$TRACKER_DIR"` and capture:

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
- If a blocker is outside the selected project, inspect it with `it show <id> --dir "$TRACKER_DIR"`; it must already be completed before the dependent issue can run.
- If a blocker ID is missing or unreadable, stop and ask the user how to proceed.
- If uncompleted project issues contain a dependency cycle, stop and ask the user to resolve the cycle.
- Treat HITL issues as not delegateable by default. Ask the user for the required decision/input, complete that interaction in the main agent, and only mark the HITL issue complete when its acceptance criteria are actually satisfied.

Topologically sort the AFK issues into parallel waves. Issues in the same wave must not depend on each other.

### 4. Announce the implementation order

Before launching workers, print a concise plan:

```text
Project: <tag>
Integration worktree: <INTEGRATION_REPO> (<main-working-branch>)
Tracker dir: <TRACKER_DIR>
Wave 1: #1 Title, #2 Title (parallel)
Wave 2: #3 Title (after #1/#2)
Blocked/HITL: #4 Title — needs human decision
Already completed: #5 Title
```

If the user asked only for a plan, stop here. Otherwise proceed without an additional approval prompt unless there are ambiguities, HITL issues, destructive changes, dirty unrelated worktree changes, port allocation failures, or dependency problems.

### 5. Prepare isolated worktrees for the next batch

Before launching any worker batch:

1. In `INTEGRATION_REPO`, run `git status --short`. Because this workflow commits and merges issue branches, the integration worktree should be clean except for changes this workflow is intentionally about to commit. If there are unrelated user changes you might overwrite or accidentally commit, ask before continuing.
2. Record the main working branch/ref from `INTEGRATION_REPO`. If the repository is in a detached HEAD state or the branch/ref is ambiguous, ask before continuing.
3. For each ready issue selected for the batch, derive a unique branch name such as `issue/<id>-<kebab-title>` and run `scripts/create-worktree.sh --base-ref <main-working-branch> <issue-branch>` from `INTEGRATION_REPO`. Capture the reported absolute worktree path and assigned ports.
4. If `scripts/create-worktree.sh` reports that no free port slot is available, run `scripts/create-worktree.sh --list-port-allocations` and `scripts/create-worktree.sh --prune-port-allocations` from `INTEGRATION_REPO` to inspect/clean stale allocations when safe. If the failure remains ambiguous or would require releasing a live worktree's ports, ask the user before continuing.
5. Limit batch size to the number of successfully prepared issue worktrees and the subagent concurrency limit.
6. For each assigned worktree:
   - Confirm its `.hamlet-worktree.env` exists.
   - Confirm `git -C <worktree> status --short` is clean, or that any changes are known remnants of this workflow and safe to discard.
   - If reusing an existing worktree/branch, reset and clean it back to the current main working branch before handing it to a worker. Preserve linked dependencies such as `client/node_modules`; do not delete expensive dependency caches unnecessarily.

### 6. Launch all available worker subagents per batch

For each wave, maximize safe parallelism:

- Launch one `worker` subagent per ready AFK issue in the current batch.
- Use the subagent tool's parallel mode.
- Set concurrency to the lower of the maximum available (currently 8), the number of ready issues, and the number of successfully prepared issue worktrees.
- Do not invent filler work when there are fewer ready issues than available subagent slots.
- Do not put dependent issues in the same wave.
- Do not put two issues in the same worktree at the same time.

Give each worker enough context to work independently. Set the subagent `cwd` to the assigned worktree root, and use this task template with issue-specific details filled in:

```text
Implement tracker issue #<id>: <title>

Project tag: <project-tag>
Integration worktree: <INTEGRATION_REPO>
Integration branch/ref: <main-working-branch>
Tracker dir: <TRACKER_DIR>
Assigned worktree: <absolute path to worktree>
Issue context:
<paste the full `it show <id> --dir "$TRACKER_DIR"` output or a faithful summary including acceptance criteria and blockers>

Instructions:
- Work only in the assigned worktree: <absolute path to worktree>.
- Read AGENTS.md and any relevant subdirectory CLAUDE.md before editing.
- Source `.hamlet-worktree.env` before running server/client commands that need app URLs or ports.
- Keep the change scoped to this issue's acceptance criteria.
- Preserve behavior outside this slice unless the issue requires it.
- Add or update tests for the behavior you implement.
- Run the relevant formatter/lint/typecheck/test commands for touched areas when practical.
- Do not edit files in the integration worktree or any other repository worktree.
- Tracker issues for this run live only in <TRACKER_DIR>; do not use or edit any worktree-local `issues/` directory.
- Do not commit, stash, reset, checkout branches, rebase, push, merge, or edit issue tracker files.
- Do not mark the issue completed; the parent agent will validate, commit and merge the implementation into the integration branch, then update tracker status in <TRACKER_DIR>.
- If blocked by missing decisions, failing dependencies, merge conflicts, occupied ports, or unclear requirements, stop and report the blocker with what you tried.

Return:
- Summary
- Files changed
- Acceptance criteria satisfied
- Tests/checks run and results
- Any blockers or follow-up needed
```

### 7. Validate, commit, and merge each batch before continuing

After a batch finishes:

1. Read every subagent report.
2. For each assigned worktree, inspect `git -C <worktree> status --short` and relevant diffs.
3. Resolve integration conflicts or incomplete work in that issue's worktree, or launch follow-up workers for clearly scoped fixes in the same assigned worktree.
4. Run relevant checks for the areas touched, from inside the assigned worktree with `.hamlet-worktree.env` sourced when needed. Follow repository instructions: if a check fails, fix it even if the failure predates the change.
5. Create an implementation commit in the issue worktree. Do not include tracker files or issue status changes in the issue worktree commit. Use a clear message such as `Implement issue #<id>: <title>`.
6. Merge the issue worktree branch back into the main working branch in `INTEGRATION_REPO`. Prefer a normal merge that preserves the issue commit. If the merge conflicts, resolve the conflict in `INTEGRATION_REPO`, rerun affected checks, and continue only when the merge is sound.
7. Only after validation and merge, mark the issue completed against the central `TRACKER_DIR`:

   ```bash
   cd "$INTEGRATION_REPO"
   it status <id> completed --dir "$TRACKER_DIR"
   ```

8. Tracker status updates happen outside git. Do not commit tracker issue files, and do not commit anything in `/home/reno/projects/hamlet` unless it is the integration worktree.
9. Re-run `it show <id> --dir "$TRACKER_DIR"` or `it list --dir "$TRACKER_DIR" --tag <project-tag>` to verify the completed status.
10. Do not schedule dependent issues until their blockers have been validated, committed, merged, and verified completed in the central `TRACKER_DIR`.
11. After a worktree has been merged, it may be reset/cleaned back to the latest main working branch and reused for a later issue batch. If you remove the worktree instead, run `scripts/create-worktree.sh --release-port-allocation <worktree-dir>` from `INTEGRATION_REPO` to release its ports.

If an issue fails or remains blocked, leave it `in-progress`, report why, do not commit/merge partial work unless the user explicitly asks, and do not run dependent issues unless their blockers are otherwise satisfied.

### 8. Final verification and report

After all runnable waves finish:

1. Run the appropriate final project checks for all touched sides from the main working branch in `INTEGRATION_REPO`. Use the repo's aggregate check script if practical.
2. Re-run `it list --dir "$TRACKER_DIR" --tag <project-tag>` to verify statuses.
3. Report concisely:
   - project tag
   - integration worktree and branch
   - tracker dir
   - completed issues
   - issues left blocked or HITL, with reasons
   - files/areas changed
   - checks/tests run

Do not claim an issue is complete unless its tracker status is `completed` in the central `TRACKER_DIR` and its acceptance criteria were verified.
