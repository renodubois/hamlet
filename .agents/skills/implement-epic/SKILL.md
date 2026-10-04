---
name: implement-epic
description: "Implement a parent GitHub issue and its sub-issues end to end, in dependency order on the current branch, with per-child commits, progress updates, verification, review, and issue closure. Use when asked to implement an epic or a whole issue tree."
disable-model-invocation: true
---

# Implement Epic

Take a parent issue number or URL and implement the approved work beneath it. This is an **execution** workflow for specified work, not `/wayfinder` planning or `/to-tickets` decomposition.

Default delivery: one current branch, sequential implementation, a commit per implementation child, GitHub progress comments, and closure of verified issues. Do not push, create PRs, merge, or change branches without an explicit user request. Closing here means **implemented and verified on this branch**, not merged or deployed; say so in tracker comments.

## 1. Establish context and a safe baseline

- If no parent reference was supplied, ask for one using `ask_user`.
- Find the repository root and read its `AGENTS.md`, `llm-docs/agents/issue-tracker.md`, and `llm-docs/agents/triage-labels.md`. Read applicable component instructions, overviews, architecture guides, `llm-docs/CONTEXT.md`, and relevant ADRs as you identify the affected areas. Paths here are repository-root-relative.
- Read `../implement/SKILL.md`, `../tdd/SKILL.md`, and `../code-review/SKILL.md` relative to this skill directory; follow their supporting references when used. Apply the existing implementation workflow per child; this skill adds orchestration, not a replacement development process.
- Use `gh` for tracker operations. Resolve the repository explicitly from the issue URL or git remotes; pass `--repo <owner/repo>` to issue commands. Verify authentication/access before starting.
- Record the current branch, `git rev-parse HEAD` as the immutable **epic baseline**, and `git status --short`. Ask before proceeding with a detached HEAD, an in-progress merge/rebase, or unrelated uncommitted changes. Never reset, stash, stage, or commit the user's changes without permission. Stage only your own paths/hunks.

## 2. Discover the issue tree and dependency graph

Fetch the parent and every descendant's full body, comments, state, state reason, labels, and assignees. Identify issues by repository plus number (or full URL), not number alone.

Use native GitHub sub-issues, **recursively and with pagination**:

```bash
gh api --paginate "repos/<owner>/<repo>/issues/<number>/sub_issues?per_page=100"
```

Keep a visited set to deduplicate references and detect malformed cycles. Retain child ordering for deterministic tie-breaking, but do not mistake hierarchy or issue-number order for dependency order.

If native sub-issues are unavailable, use an explicitly designated child-issue task list in the parent body, verifying each reference. Do not turn arbitrary mentions, related links, or acceptance checkboxes into children. A checked task-list box is not proof that the linked issue is completed. If native links and an explicit child list disagree, surface the discrepancy for approval. Authentication, permission, rate-limit, and network failures are not empty lists; stop or retry, and never silently treat discovery failures as no work.

For each issue, fetch the issues blocking it, including blockers outside this tree:

```bash
gh api --paginate "repos/<owner>/<repo>/issues/<number>/dependencies/blocked_by?per_page=100"
```

Native dependencies are canonical. Where unavailable, use the explicit `Blocked by` convention in the tracker guidance. Surface conflicting textual dependencies rather than silently ignoring them. Fetch blocker states; do not add external blockers to implementation scope automatically. Unknown/inaccessible blocker state means blocked. A closed issue with a non-completion reason (for example `not_planned`/`wontfix`) is not evidence that its promised behavior exists.

Classify the tree:

- **Implementation issues**: open, specified deliverables with acceptance criteria.
- **Containers**: aggregate children; audit their own acceptance criteria too. Residual implementation requirements need explicit coverage, not an assumption that closing leaves satisfies them.
- **Already closed**: do not reimplement or reopen by default. Check that behavior required by open dependents exists on the current branch; tracker closure alone does not prove the code is present.
- **Needs human/clarification or claimed elsewhere**: surface `needs-info`, `needs-triage`, `ready-for-human`, `wontfix`, planning/research tickets, and other people's assignments. Do not override them implicitly. Absence of `ready-for-agent` alone does not exclude a fully specified issue the user explicitly approves.

Detect dependency cycles, open external blockers, missing specs, and overlapping/contradictory acceptance criteria. Also check dependencies on containers: their descendants and residual requirements must complete before the container can unblock another issue. If no children exist, offer `/implement` rather than inventing a breakdown.

## 3. Agree the execution plan once

Create a resumable ledger at `llm-docs/implement-epic/<owner>-<repo>-<parent-number>.md`. It is a checkpoint, not a duplicate spec: link to the issues and record only execution decisions and evidence. Keep all generated documentation under `llm-docs/`; never edit human-owned documentation or any `README.md`.

Record:

- Parent URL, branch, immutable epic baseline, and any pre-existing working-tree changes.
- The approved issue set, including nested containers and any parent-only requirements.
- A table of linked issue titles, blockers, execution status, acceptance coverage, test commands/results, review status, and commit SHAs.
- Agreed test seams, closure policy, unresolved decisions, and the next runnable issue.

Present a short dependency-ordered plan, excluded/blocked work, and test seams. Use `ask_user` to approve it before implementation or tracker writes. State that verified issues will be closed even if commits remain local, and that no push/PR/merge is included. Follow `/tdd`'s requirement to confirm test seams; approval may cover seams for several children at once. Ask again only for changed scope, a new seam, or a material design choice the issues do not settle.

Do not silently expand the scope to external dependencies, create missing tickets, remove blocking edges, or rewrite issue specifications. If the approved plan leaves blockers unresolved, implement only its runnable portion and report partial completion.

## 4. Work the dependency frontier

Repeat until every approved deliverable is done or no runnable work remains:

1. **Refresh** the next candidate's body/comments, state, assignees, and blockers. Pick an approved open issue whose prerequisites are satisfied, using hierarchy order to break ties. A prerequisite is satisfied only when its required behavior is available on this branch and its tracker gate is resolved. A newly added child or materially changed requirement needs approval before entering scope.
2. **Claim and announce** the child using the configured tracker conventions (assign to `@me` if unassigned; do not steal another person's assignment). Post a concise start comment naming the parent, branch, and intended deliverable.
3. **Implement** that child's acceptance criteria using `/implement` and `/tdd` at the agreed seams. Run the relevant typechecking/static checks regularly and focused tests after each slice. Honor stricter component instructions for full-suite runs. Do not batch all tests before all implementation or have multiple agents write concurrently to this shared branch.
4. **Verify and commit** only this child's work, with its issue reference in the commit message. The commit must precede `/code-review`, because that skill reviews committed changes through `HEAD`. Capture the pre-child HEAD as the child's review baseline. For pre-existing behavior, verify it and record evidence rather than creating an empty commit.
5. **Review** using `/code-review` against that child's baseline, with the child spec and applicable parent constraints supplied explicitly. Its Standards and Spec reviews run in parallel read-only sub-agents. If the child needed no changes and the diff is empty, do not invoke `/code-review` (it rejects empty diffs): instead run parallel read-only Standards and Spec acceptance reviews of the relevant existing code and test evidence, identifying exactly which criteria they cover. Record this as an existing-behavior review, not a diff review. Resolve material findings, commit any fixes, rerun affected checks, and review the fixes before calling the child complete.
6. **Record and close** only after its acceptance criteria and required checks pass and material review findings are resolved. Use a closure comment with the delivered behavior, acceptance/test evidence, commit SHA(s), and branch. Explicitly state whether the commits are local/unpushed and that merge/deployment has not been verified. Close as completed using the tracker convention, then verify the resulting state. If tracker writes fail, record `implemented; tracker update pending` and resolve that before treating dependent tracker gates as open for execution.
7. **Checkpoint** the ledger with actual results and SHAs, and post a concise parent progress comment linking the finished child. Recompute the frontier; do not keep following a stale initial list.

If an issue fails tests, needs a human decision, or cannot be completed, leave it open, record the precise blocker and any partial commits, and keep its dependents blocked. Continue other independent approved work only if the working tree and committed branch are in a safe, verified state. Otherwise stop with a recoverable checkpoint; do not commit broken work just to move on.

Intermediate containers can close once all required descendants and their own criteria are verified. If a container has residual implementation work, schedule it after its prerequisites under the same implement/verify/commit/review rules. Do not close the root parent yet.

## 5. Verify the whole epic and close the parent

- Refresh the entire tree and dependencies. Surface new children or changed requirements for approval. Do not close the parent with unresolved required children, blockers, or excluded in-scope work.
- Audit the parent's acceptance criteria against the integrated behavior, including requirements not stated in individual children. Implement approved parent-only work with the same verification and review gates.
- Run the full required suites for every affected component, plus applicable typechecking, formatting/static checks, and cross-child integration checks. Record exact commands and results; skipped/unavailable required checks mean incomplete, not passing.
- Run `/code-review` from the **original epic baseline** to `HEAD`, explicitly supplying the parent and all approved child specs. Keep the two review axes parallel and read-only. If no implementation changed across the entire run, use the existing-behavior acceptance-review path from step 4 rather than requesting an empty diff review; review the integrated parent criteria, not merely the children in isolation. Fix material findings, commit fixes, rerun affected checks, and re-review. Passing per-child reviews does not replace this integrated review.
- If the combined review uncovers a regression in a child already closed by this run, comment and reopen that issue while fixing it; close again only with renewed evidence. Never leave a known broken child marked complete.
- Update the ledger with the final SHAs and verification. Include any ledger-only changes in an explicitly staged checkpoint commit; do not sweep unrelated files into it.
- Close the parent as completed only when all required work and checks pass. Its completion comment links the children, summarizes integrated acceptance coverage, test/review results, branch and commit SHAs, and clearly states push/merge/deployment status. Do not claim it shipped.
- End with a concise summary: completed versus remaining issues, verification, branch/commits, and whether anything was pushed. If partial, leave the parent open with the blocker and next action.

## Resume rather than restart

When invoked again for the same parent, read the ledger, issue history, and git history first. Preserve the original epic baseline for the combined review; never replace it with the resumed HEAD. Require the recorded branch to be the current branch, and verify that the epic baseline and recorded implementation/fix commits are ancestors of `HEAD` (for example, `git merge-base --is-ancestor <sha> HEAD`). Check for uncommitted partial work too. Merely finding commits elsewhere locally is insufficient. If these checks fail, ask how to reconcile rather than checking out/resetting automatically.

Reconcile local acceptance evidence with live tracker state. Distinguish `planned`, `in progress`, `implemented; tracker update pending`, `completed`, and `blocked`; do not infer completion from a ledger row, a commit message, or an issue's closed state alone. Finish pending verification/review/tracker updates before repeating implementation, avoid duplicate progress comments, and resume at the first verified runnable frontier issue.

## API references

Read operations above follow GitHub's official REST documentation:

- Sub-issues: https://docs.github.com/en/rest/issues/sub-issues
- Dependencies: https://docs.github.com/en/rest/issues/issue-dependencies
