import { ref } from 'vue'

/**
 * Show / dismiss state shared by the in-pet overlays (Tarot reading, System
 * State). PetWindow grows the main window, paints the overlay, and on close
 * shrinks the window back *before* the overlay is torn down. If the overlay
 * played its leave fade during that shrink, the still-fading panel would be
 * carried to the pet's small window as a brief "afterimage" glitch.
 *
 * `dismiss()` raises `skipLeave` so a `<Transition :css="!skipLeave">` removes
 * the overlay instantly (no leave animation); `show()` lowers it again so the
 * next open still fades in. Wire both into the component template:
 *
 * ```vue
 * <Transition name="fade" :css="!skipLeave">
 *   <div v-if="visible"> … </div>
 * </Transition>
 * ```
 *
 * The component keeps its own `open()` / close logic; it just calls `show()` /
 * `dismiss()` for the visibility transition (wrap `dismiss` if extra teardown,
 * e.g. clearing a timer, is needed).
 */
export function useOverlayVisibility() {
  const visible = ref(false)
  /** While true, the leave transition is suppressed (see module doc). */
  const skipLeave = ref(false)

  /** Reveal the overlay with its normal enter fade. */
  function show(): void {
    skipLeave.value = false
    visible.value = true
  }

  /** Tear the overlay down instantly — call AFTER the window has shrunk back. */
  function dismiss(): void {
    skipLeave.value = true
    visible.value = false
  }

  return { visible, skipLeave, show, dismiss }
}
