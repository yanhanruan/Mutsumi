# Release notes

One file per release, named exactly after the tag: `vX.Y.Z.md` (e.g.
`v1.6.0.md`). Written for **users**, in the project's trilingual style
(zh + en, ja where it matters), and committed **in the release PR** so the
notes are reviewed like code.

At tag-build time, [`release.yml`](../../.github/workflows/release.yml) uses
this file as the GitHub Release body **and** the in-app update pop-up notes
(via `latest.json`). If the file for a tag is absent, CI falls back to the
annotated tag message.

Two rules that exist because we got burned:

- **Notes must be final before the tag is pushed.** `latest.json` freezes the
  notes at draft-generation time — hand-editing the GitHub release page later
  changes the web page only; the in-app pop-up keeps the frozen text.
- **Don't hand-write notes anywhere else.** A notes file outside this
  directory (the old root `RELEASE_NOTES_v1.5.0.md`) is wired to nothing and
  will drift from what users actually see.

See [`docs/RELEASING.md`](../RELEASING.md) for the full release ritual.
