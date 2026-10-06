# Relative message timestamps

## Agreed behavior

- Message timestamps use calendar boundaries in the viewer's local timezone.
- Messages from today show elapsed time: `just now` below one minute, whole minutes below one hour, then whole hours. Round down and use singular/plural labels (`1 minute ago`, `5 minutes ago`, `1 hour ago`).
- Default to a 12-hour clock with AM/PM; a 24-hour clock setting is deferred.
- Messages from yesterday show e.g. `yesterday at 1:41 PM`.
- Older messages use a numeric month/day/year date and time, e.g. `03/14/2026 1:41 PM`.
- Refresh displayed labels once per minute while the conversation remains open, including calendar-boundary changes.

- Hovering a timestamp reveals its exact local date and time including seconds, e.g. `03/14/2026 1:41:08 PM`.
- Future timestamps display `just now` until the viewer's clock catches up; the exact-time tooltip still shows the actual timestamp.
