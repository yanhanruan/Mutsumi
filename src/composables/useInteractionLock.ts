/**
 * useInteractionLock — a tiny module-level flag shared across the pet UI.
 *
 * Set while the user is mid-drag on an interactive overlay (the music progress
 * bar, the volume slider). While locked, `useHitTest` keeps the window
 * interactive and never toggles per-pixel click-through.
 *
 * Why this exists: `setIgnoreCursorEvents(true)` makes the window transparent to
 * the mouse at the OS level, which (a) breaks an in-progress pointer capture and
 * (b) fires a synthetic `mouseleave` that collapses the hover panel. During a
 * drag the click-through hit-test would otherwise flap on/off many times a
 * second, chopping one drag into many `pointerdown`/`pointerup` cycles — each of
 * which commits its own seek, so the player jumps repeatedly. Holding the lock
 * for the duration of the gesture keeps the window stable so the drag stays a
 * single, continuous interaction.
 */
import { ref } from 'vue'

// Module-level singleton: shared by every caller of useInteractionLock().
const dragging = ref(false)

export function useInteractionLock() {
  return {
    /** True while an overlay drag (seek / volume) is in progress. */
    isDragging: () => dragging.value,
    beginDrag:  () => { dragging.value = true },
    endDrag:    () => { dragging.value = false },
  }
}
