# Codex rules for this project

## Workflow

- **Never commit without explicit user instruction.**
  When changes are ready, stage them (`git add`) and stop.
  Only run `git commit` after the user says "commit" or equivalent.

## i18n

- **Always implement i18n for user-facing strings.** Never hardcode display
  text in components. Route every player-facing string through the existing
  i18n system: add the key to `Translations` in `src/i18n/types.ts`, provide
  `en` / `zh` / `ja` in `src/i18n/locales/*`, and read it via `useI18n()`.
- For localized *data* (not just chrome), use a `{ en, zh, ja }` shape and
  select by the active locale (see `LocalizedText` in `src/config/tarot.ts`).
- Supported locales: `en`, `zh`, `ja` (fallback `en`).
