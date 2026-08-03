# Mutsumi eye-layer spec (for a real, smooth parametric blink)

The puppet blinks by **sliding an opaque eyelid down over the stationary eyes** —
the Live2D/PSD way. It's smooth and never ghosts. It needs the character authored
as **separate layers**, not one flattened frame. This documents the layer set the
rig consumes and how the blink is built from it.

## What the rig needs

Aligned, same-size PNG layers (Mutsumi's are **768×768**), dropped in
`public/assets/mutsumi_layers/`. The current set (a full-body PSD export):

```
back_hair  hair_front  face  ears  eyebrow  eyelash  irides  nose  mouth
topwear  bottomwear  hand_left  hand_right  leg_left  leg_right
```

The blink only cares about three of them; the rest are just composited into the
body. The **three that matter**:

| Layer | Requirement |
|-------|-------------|
| `face` | **Solid, opaque skin** across the whole head — including *behind* the eyes (no eye holes cut out). The eyelid is sampled from this skin. |
| `irides` | The eyeballs (iris + pupil + highlight), on their own layer. Static during a blink. |
| `eyelash_original` | The **upper** eyelashes / eye line, on their own layer. This rides the eyelid as it closes. |
| `eyelash_outer_corner` | *(optional)* The eye's outer tail (the canthus). A **fixed pivot** — drawn *behind* the lid (over the irides), so the closing skin **buries** it while the main lash sweeps down. |

The lash may be a single `eyelash` layer instead of the split pair; the rig falls
back to that and moves the whole thing.

That's it — **no dedicated eyelid or "whites" layer is required.** Because `face`
is solid skin, the rig *synthesises* the eyelid at load: an opaque skin patch
(the skin colour sampled from `face`) with the `eyelash` layer on top.

## How the blink is built (`main.ts` + `faceRig.ts`)

At load the layers are composited into **four deforming groups**, drawn back→front:

1. **back** — everything through the open eyes (`back_hair … face … irides`), plus
   the fixed-pivot `eyelash_outer_corner` (buried by the lid when it shuts).
2. **lidSkin** — a synthesised, opaque skin curtain (one rect per eye, tinted to
   that eye's *local* skin and feather-blurred so a shaded brow doesn't leave a
   hard patch). Closes with a **shallow, corner-covering arc**: the skin pins at the
   top and stretches down over the irides — the aperture closes from the top, and a
   high corner floor keeps the whole eye occluded.
3. **lidLash** — the moving `eyelash_original`, closing onto a **deep, corner-
   pivoting ‿ curve with lifted outer tips**. Centre drops most, canthi least, so
   the lash **changes curvature** as it descends, interpolating toward a designed
   closed-eye line rather than sliding rigidly.
4. **front** — `eyebrow`, `hair_front` (stay above the lid).

Splitting skin from lash lets each close to its *own* shape over one shared mesh:
the skin covers (no squash, no cross-fade, no ghosting) while the lash forms the
expressive closed curve. Per-eye geometry (`cx`, `half`, `pinV`, `lashV`, `travel`)
is measured from the `irides` / `eyelash_original` alpha, split into two eyes at the
nose-bridge column — so each eye pivots at its own corners and a left/right
asymmetry in the art can't misfit a single global shutter.

The blink timeline is **ballistic + asymmetric**: a fast snap shut (~75 ms,
ease-out) and a slower, gentle reopen (~180 ms, smoothstep) — matching real blink
kinematics, never a linear ramp.

### The one remaining limit — the eyelid SKIN
The closing *curve* is fully procedural and matches the designed closed eye. The
one thing synthesis can't invent is the eyelid's **shading**: `face.png` has baked
shadows (e.g. under one brow), so a flat-tinted curtain can read as a patch there —
per-eye local tint + feathering reduce it but don't erase it. The clean fix is a
drawn closed-eye layer (`eye_closed`: the shaded eyelid + closed lash, aligned to
the 768 canvas), which the rig can slide/reveal instead of synthesising — matching
your art exactly on both sides.

## If you re-author the character
Keep the invariants — **solid-skin `face`**, separate **`irides`**, separate
**`eyelash_original`** (+ optional **`eyelash_outer_corner`**) — and any aligned
layer set drops straight in.

## Testing it
Reload `puppet-spike.html`, tick **"Real Mutsumi"**. The status readout shows
**"layered blink"** when the layers are found. She blinks on the auto-blink /
`Blink` button, or scrub it by hand with the **Eye open** slider. If the layers
are missing it falls back to the single baked frame (breathing only).
