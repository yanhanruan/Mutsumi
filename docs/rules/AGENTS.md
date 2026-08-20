# Codex rules for this project

## Workflow

- **Never commit without explicit user instruction.**
  When changes are ready, stage them (`git add`) and stop.
  Only run `git commit` after the user says "commit" or equivalent.

### Mandatory pre-commit PRD review

- Before every commit, spawn or reuse a dedicated read-only reviewer using
  `gpt-5.6-terra` with `high` reasoning. The reviewer must inspect the complete
  staged diff against the applicable PRD; for the macOS adaptation, the PRD is
  `docs/MACOS-ADAPTATION-PLAN.md`.
- The reviewer must not edit, stage, or commit files. Its report must classify
  findings by severity, cite file/line evidence, explain impact, distinguish
  confirmed defects from risks or non-issues, and finish with a commit verdict.
- The primary agent must independently verify every finding. Fix confirmed
  defects and tell the user how they were resolved. For incorrect or overstated
  findings, explain the rejection with concrete evidence.
- Do not commit while a confirmed blocking finding remains unresolved unless
  the user explicitly changes the relevant product requirement or accepts the
  documented risk. A prior review does not cover changes made after that review;
  rerun the reviewer on the final staged diff immediately before committing.

## i18n

- **Always implement i18n for user-facing strings.** Never hardcode display
  text in components. Route every player-facing string through the existing
  i18n system: add the key to `Translations` in `src/i18n/types.ts`, provide
  `en` / `zh` / `ja` in `src/i18n/locales/*`, and read it via `useI18n()`.
- For localized *data* (not just chrome), use a `{ en, zh, ja }` shape and
  select by the active locale (see `LocalizedText` in `src/config/tarot.ts`).
- Supported locales: `en`, `zh`, `ja` (fallback `en`).
