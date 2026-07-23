# Live2D renderer migration — living plan

The goal is **not** "replace WebP animations with Live2D." It is to build a
**renderer-independent** presentation layer where frame animations, Live2D
motions, expressions, outfits, and lip sync are composed through one state
machine — while the existing WebP assets keep working and migrate gradually.

This adapts the external "Tauri 2 Desktop Pet Live2D Migration Development Plan"
to what this repo already has, rather than following it verbatim. We already
own a clean split that the plan assumes we need to build from scratch:

| Plan concept                 | Already exists here                                  |
| ---------------------------- | ---------------------------------------------------- |
| Manifest (WHAT to play)      | `DEFAULT_ANIMATIONS` in `src/config/animations.ts`   |
| Scheduler / state machine    | `useAnimator` in `src/composables/useAnimator.ts`    |
| Return-to-rest policy        | `resolveBaseline` (sleep > audio > idle variant)     |
| Chains / one-shots / loops   | `ANIM_CHAINS`, `IDLE_VARIANTS`, `MUSIC_MODE_ANIMS`   |

So the migration reuses these instead of introducing a parallel manifest.

## Working agreement

- **Incremental.** One small, independently-revertible step per commit. No
  large rewrite. Preserve existing WebP behavior unless removal is explicitly
  required.
- **Tests for logic; humans for looks.** Automated tests cover scheduling,
  renderer selection, fallback, and resource cleanup. They **never** stand in
  for manual sign-off of anything visual (animation feel, expressions, outfits,
  lip sync, transparency, performance). Those are manual acceptance gates.
- **Never commit without explicit instruction** (see `docs/rules/CLAUDE.md`).

## Renderer strategy — build our own, don't vendor Cubism

**Decision:** instead of integrating the proprietary **Live2D Cubism SDK** we
build our **own open puppet runtime** — parameter-driven 2D mesh deformation —
in TypeScript + WebGL2, rendered inside the existing transparent pet WebView.

Why this is viable and not hubris: Live2D's only genuinely closed piece is the
`.moc3` binary model format. The *technique* (deform layered meshes by
parameters at runtime) is open and proven — [Inochi2D](https://docs.inochi2d.com/)
is a full open standard that does exactly this with its own format, and
[inox2d](https://github.com/Inochi2D/inox2d) is its Rust port with a WASM/WebGL
backend. We use those as **reference** for the model + math (not a dependency).

Consequences vs the Cubism path:

- **No licensing gate.** No Cubism SDK license, no "Expandable Application"
  review. (The whole "release licensing" acceptance gate disappears.)
- **Open tooling.** Rigs can be authored in the open **Inochi Creator** instead
  of the proprietary Cubism Editor; we can adopt the Inochi2D `.inp` format so
  we get its editor for free rather than building a rigging tool.
- **In-WebView.** Renders onto the transparent pet window we already have — no
  native overlay window, Rust backend keeps owning files/config/system.
- **Unchanged remaining cost:** we still need character art authored as
  *layered, rigged* parts (the plan's Phase 6.1) — true for Cubism too. Bootstrap
  the engine with a procedural/sample test rig; real art comes later.

The dual-renderer / channels / fallback architecture below is unchanged — we
just swap the second renderer from `CubismRenderer` to our `PuppetRenderer`.

## Incremental steps (adapted commit order)

1. **Phase 0 — freeze & audit assets.** ✅ 0.1 done.
2. Define the `PetRenderer` interface; wrap `useAnimator` as `FrameRenderer`
   behind it (no visual change).
3. Split animator state into independent channels (body / expression / outfit /
   mouth / lookAt / overlay).
4. **Puppet runtime (from scratch).** ← current
   - P1 pure core: parameters + mesh deformation math (no GPU/art). ✅ started.
   - P2 WebGL2 spike: textured mesh in the transparent window, one live
     parameter + auto-blink — proves WebGL + transparency + deformation.
   - P3 scene graph, draw order, standard params (blink/breath/gaze), physics.
   - P4 model loader (adopt Inochi2D `.inp`, or a minimal own format).
5. Production `PuppetRenderer` + renderer selection & fallback matrix.
6. Feature flag (`legacy-frame-only` / `puppet-preview` / `hybrid` / …).
7. Expression channel, then outfit presets.
8. Lip sync v1 (audio-amplitude → `ParamMouthOpenY`).
9. Migrate action families one at a time (preserve action IDs + frame fallback).
10. Frame-cache / performance, then E2E, then staged release.

## Manual acceptance gates (automation must stop here)

Legacy behavior equivalence · puppet renders in the transparent window (no black
box, WebGL cleanup) · base rig · first outfit · expression+blink+lipsync combos ·
first migrated actions · performance on target Windows hardware · fallback/rollback.

Each passed gate records: commit SHA, device, WebView2 version, recordings,
known issues, result. (No licensing gate — the runtime is ours + open.)

## Status

### Phase 0.1 — legacy frame asset inventory ✅ (code) / ⏳ (manual sample)

`src/config/assetInventory.ts` audits the registry's declared frame counts
against the WebP files on disk (missing/gap frames, count mismatch, extra
files, duplicate indices, orphan/missing dirs, registry-internal count
conflicts). Pure and unit-tested (`assetInventory.test.ts`); the live audit of
the committed assets runs in `npm test` (`assetInventory.live.test.ts`) and
fails the suite on drift.

Run the report directly:

```
npm run audit:assets
```

Baseline at time of writing: **12/12 animation dirs OK**; `tarot/` is a
non-animation dir (card art), correctly reported as an informational orphan.

**Manual gate (not yet done):** a human should still eyeball a representative
sample — the longest clip (`idle`, 426), the shortest (`music3`, 139), one with
complex transparent edges, and one flagged item — before we trust the audit as
the Phase 0 baseline. The audit checks *presence/contiguity*, not visual
quality; it must not be treated as proof the frames themselves are good.

### Phase P1 — puppet deformation core ✅ (code)

`src/puppet/` holds the from-scratch runtime's pure math (no DOM/WebGL/art):

- `parameters.ts` — the parameter model (`ParamAngleX`, `ParamEyeLOpen`, …):
  clamp, normalize-to-[0,1], NaN→rest-value, default/index maps.
- `deform.ts` — `deform(base, bindings, defs, values)`: interpolates each
  parameter's keyforms and sums them **additively** onto the base mesh, so
  blink/breath/gaze/lip-sync compose instead of overwriting. Never mutates the
  rest pose; tolerates partial rigs; no NaN reaches the mesh.

Unit-tested (`parameters.test.ts`, `deform.test.ts`). This is the engine's
heart; the WebGL upload + a visible test rig (P2) is the next slice and is where
the first *manual* visual check will be needed (does it render in the
transparent window, no black box).
