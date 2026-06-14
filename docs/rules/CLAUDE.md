# Claude rules for this project

## Workflow

- **Never commit without explicit user instruction.**
  When changes are ready, stage them (`git add`) and stop.
  Only run `git commit` after the user says "commit" or equivalent.

- **Stop the dev app with Ctrl-C, never a force-kill.**
  When you need to stop `npm run tauri dev` (or the app), shut it down
  gracefully (Ctrl-C / close the window) so it can run its cleanup.
  Do NOT `Stop-Process -Force` / `taskkill /F` / `kill -9` the running app:
  force-killing it mid-call can wedge OS-level resources it was using
  (e.g. the Windows SMTC media-session broker), breaking media detection
  and control for the whole machine until a full reboot.

## i18n

- **Always implement i18n for user-facing strings.** Never hardcode display
  text in components. Route every player-facing string through the existing
  i18n system: add the key to `Translations` in `src/i18n/types.ts`, provide
  `en` / `zh` / `ja` in `src/i18n/locales/*`, and read it via `useI18n()`.
- For localized *data* (not just chrome), use a `{ en, zh, ja }` shape and
  select by the active locale (see `LocalizedText` in `src/config/tarot.ts`).
- Supported locales: `en`, `zh`, `ja` (fallback `en`).
