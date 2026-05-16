# Draft issue breakdown: Iced native client conversion

Source: `docs/iced-native-client-conversion-prd.md`

Status: Draft for review. These are proposed tracer-bullet slices for the `/to-issues` workflow; they are not the final individual issue files yet.

## Review questions

- Does the granularity feel right, or are any slices too coarse/too fine?
- Are the dependency relationships correct?
- Should any slices be merged or split further?
- Are the correct slices marked as HITL and AFK?

## Proposed issue slices

### 1. Native Iced app boot, server URL preference, and fixture harness

**Type**: AFK

**Blocked by**: None

**User stories covered**: 1–3, 74–75, 79

### 2. Cookie-backed login/register with clear errors

**Type**: AFK

**Blocked by**: Issue 1

**User stories covered**: 4–8, 70–71, 73, 79

### 3. Startup session restore, invalid-session recovery, and logout

**Type**: AFK

**Blocked by**: Issue 2

**User stories covered**: 9–11, 70, 73–74

### 4. Channel navigation with message-history read path

**Type**: AFK

**Blocked by**: Issue 3

**User stories covered**: 12–15, 23–26, 70–71

### 5. Send messages with authenticated SSE live updates

**Type**: AFK

**Blocked by**: Issue 4

**User stories covered**: 27–29, 70–72, 74, 79

### 6. Edit/delete own messages and reconcile message SSE events

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 30–36

### 7. Typing notifications and expiring indicators

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 45–47

### 8. Create text/voice channels and apply live channel-create events

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 16–18, 21

### 9. Accessible channel reorder with optimistic rollback and live reorder events

**Type**: AFK

**Blocked by**: Issue 8

**User stories covered**: 19–20, 22, browser drag/drop compromise from 82

### 10. Settings shell and display-name profile updates

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 50–53, 56

### 11. Avatar display, fallback generation, upload/delete, and image URL handling

**Type**: AFK

**Blocked by**: Issue 10

**User stories covered**: 48–49, 54–56

### 12. Safe URL recognition and external link opening

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 37–38

### 13. Native embeds, image previews, embed events, and suppress action

**Type**: AFK

**Blocked by**: Issues 6 and 12

**User stories covered**: 32, 39–42, iframe/embed compromise from 82

### 14. Emoji picker and keyboard-friendly draft insertion

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 43–44

### 15. Voice presence in channel list without joining audio

**Type**: AFK

**Blocked by**: Issue 5

**User stories covered**: 57–59

### 16. LiveKit voice join/leave/switch with native audio worker

**Type**: AFK

**Blocked by**: Issue 15

**User stories covered**: 60–62, 66–67

### 17. Voice controls, speaking indicators, device preferences, and permission handling

**Type**: AFK

**Blocked by**: Issue 16

**User stories covered**: 63–65, 68–69, 78

### 18. Package native desktop alpha with native QA checklist and boundary docs

**Type**: AFK

**Blocked by**: Issues 9, 11, 13, 14, 15, and 17

**User stories covered**: 75, 77, 79–82
