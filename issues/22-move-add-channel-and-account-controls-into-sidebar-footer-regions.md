---
id: 22
name: Move add-channel and account controls into sidebar footer regions
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, sidebar]
blocked_by: [20]
---


# Move add-channel and account controls into sidebar footer regions

**Type**: AFK

**User stories covered**: 13, 14, 15, 16, 17, 18, 19, 43

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Reorganize add-channel, create-channel, profile/settings, and logout affordances into stable sidebar bottom/footer regions. Normal navigation should stay uncluttered, channel creation controls should appear only when needed, and errors/settings/account actions should remain close to the controls that produce them.

## Acceptance criteria

- [ ] Add-channel is fixed near the bottom of the sidebar and opens compact creation controls only when needed.
- [ ] Channel creation errors render near the create-channel controls without overlapping channel navigation.
- [ ] A fixed user panel shows avatar/display name and provides compact access to settings/profile and logout.
- [ ] Existing create-channel, settings, display-name/avatar, and logout behavior remains unchanged.

## Blocked by

- #20
