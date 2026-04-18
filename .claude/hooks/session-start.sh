#!/bin/bash
set -euo pipefail

# Only run in Claude Code web sandboxes; locally we don't want to surprise
# developers by installing system packages or re-downloading browser binaries.
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# The hook runs from the repo root ($CLAUDE_PROJECT_DIR). Client deps and
# Playwright's Chromium live under client/.
cd "${CLAUDE_PROJECT_DIR:-$(pwd)}/client"

# `npm install` is idempotent and cheaper on re-runs than `npm ci`, which
# also plays nicely with the container state caching between sessions.
npm install

# Install Playwright's Chromium so `npm run test:e2e` works out of the box.
# `--with-deps` is intentionally omitted: it shells out to apt-get, which in
# this sandbox hits PPAs that return 403. The base image already ships the
# shared libs Chromium needs.
#
# If the sandbox's host allowlist does not include cdn.playwright.dev, the
# download 403s — we don't want that to fail the whole hook (everything
# else in the session still works), so log a clear remediation hint and
# carry on.
if ! npx --yes playwright install chromium; then
  echo "----------------------------------------------------------------"
  echo "Playwright Chromium install failed."
  echo "Most likely cause: this sandbox's outbound network allowlist does"
  echo "not include cdn.playwright.dev. Add it (plus storage.googleapis"
  echo ".com, which Playwright falls back to for some builds) and the"
  echo "next session will install Chromium automatically."
  echo "E2E tests (npm run test:e2e) will not run until this is resolved;"
  echo "Vitest, typecheck, and lint still work."
  echo "----------------------------------------------------------------"
fi
