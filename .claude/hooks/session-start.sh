#!/bin/bash
set -euo pipefail

# Only run in Claude Code web sandboxes; locally we don't want to surprise
# developers by installing system packages or re-downloading browser binaries.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# The hook runs from the repo root ($CLAUDE_PROJECT_DIR). Client deps and
# Playwright's Chromium live under client-electron/.
cd "${CLAUDE_PROJECT_DIR:-$(pwd)}/client-electron"

# `npm install` is idempotent and cheaper on re-runs than `npm ci`, which
# also plays nicely with the container state caching between sessions.
npm install

# Prefer a Chromium binary that's already baked into the sandbox image.
# Claude Code's web sandbox ships Playwright browsers under /opt/pw-browsers
# and blocks outbound requests to cdn.playwright.dev, so downloading our own
# would 403. The baked binary is a version-compatible Chromium build; we
# point Playwright at it via launchOptions.executablePath (wired up in
# playwright.config.ts) and export the env var via $CLAUDE_ENV_FILE so
# subsequent shell commands in the session inherit it.
BAKED_CHROMIUM="/opt/pw-browsers/chromium"
if [ -x "$BAKED_CHROMIUM" ]; then
  echo "Using pre-installed Chromium at $BAKED_CHROMIUM"
  if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
    echo "export PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=$BAKED_CHROMIUM" >> "$CLAUDE_ENV_FILE"
  fi
  exit 0
fi

# Fallback: try to download. If the sandbox's host allowlist does not
# include cdn.playwright.dev the download 403s — we don't want that to
# fail the whole hook (everything else in the session still works), so
# log a clear remediation hint and carry on.
if ! npx --yes playwright install chromium; then
  echo "----------------------------------------------------------------"
  echo "Playwright Chromium install failed."
  echo "Most likely cause: this sandbox's outbound network allowlist does"
  echo "not include cdn.playwright.dev, and no pre-installed Chromium was"
  echo "found at $BAKED_CHROMIUM. Either allowlist cdn.playwright.dev or"
  echo "bake a Chromium binary at that path."
  echo "E2E tests (npm run test:e2e) will not run until this is resolved;"
  echo "Vitest, typecheck, and lint still work."
  echo "----------------------------------------------------------------"
fi
