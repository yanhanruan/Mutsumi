/**
 * useAudioReaction — subscribe to backend audio events and queue the
 * matching pet animations.
 *
 * The Rust side (audio.rs) now does continuity-based detection: it only
 * emits audio-started after 3 s of sustained activity and audio-stopped
 * after 6 s of sustained silence. So this side no longer needs its own
 * debounce — events arrive already filtered.
 *
 * What this side still has to do is reconcile against in-flight pending
 * animations. Race example without the cancel-pending guard:
 *   1. music playing, user pauses audio for 7 s
 *   2. audio-stopped fires → queueAnim('headphones_off') → pending set
 *   3. user resumes audio before the music loop ends
 *   4. audio-started fires while currentName is still 'music'
 *   5. Without cancelling the pending, the music loop ends, consumes the
 *      stale 'headphones_off', and the pet drops to idle — even though
 *      audio is playing.
 * The two guards below mirror Python's _begin / _end_music_sequence.
 */
import { onMounted, onUnmounted, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import type { AnimationName } from './useAnimator'

// Animations that represent "in / entering music mode". headphones_off is
// intentionally NOT in this set — it's the *exit* animation that leads back
// to idle. If audio resumes while headphones_off is playing, we DO need to
// re-enter via headphones_on, not skip it.
const IN_MUSIC_ANIMS: ReadonlySet<AnimationName> = new Set([
  'headphones_on',
  'music1',
  'music2',
  'music3',
  'music4',
])

export function useAudioReaction(
  queueAnim:     (name: AnimationName) => void,
  currentName:   Ref<AnimationName>,
  getPending:    () => AnimationName | null,
  cancelPending: () => void,
) {
  let unlistenStarted: UnlistenFn | null = null
  let unlistenStopped: UnlistenFn | null = null

  onMounted(async () => {
    // Pull the cached Rust state first.  If audio-started fired before this
    // listener registered (WebView2 cold-start can take 1-5 s; the tracker
    // threshold fires at poll #6 = 3 s), the event is already gone and
    // ContinuityTracker will never re-emit it.  Reading the managed
    // AudioState here covers that window.
    const alreadyPlaying = await invoke<boolean>('get_audio_state')
    if (alreadyPlaying && !IN_MUSIC_ANIMS.has(currentName.value)) {
      queueAnim('headphones_on')
    }

    unlistenStarted = await listen('audio-started', () => {
      if (IN_MUSIC_ANIMS.has(currentName.value)) {
        // Already in / entering music. A pending 'headphones_off' here
        // means a stop event fired earlier and is still queued — cancel it
        // so the music chain doesn't drop out when the current loop ends.
        if (getPending() === 'headphones_off') {
          cancelPending()
        }
      } else {
        // In idle, click, or headphones_off (i.e., currently exiting music).
        // Queue headphones_on to enter / re-enter. The pending-anim slot
        // overrides the hardwired headphones_off → idle chain, so this also
        // correctly handles the "audio resumed mid-exit" case.
        queueAnim('headphones_on')
      }
    })

    unlistenStopped = await listen('audio-stopped', () => {
      if (IN_MUSIC_ANIMS.has(currentName.value)) {
        queueAnim('headphones_off')
      } else if (getPending() === 'headphones_on') {
        // Audio started briefly then stopped before headphones_on landed —
        // cancel the queued entrance.
        cancelPending()
      }
    })
  })

  onUnmounted(() => {
    unlistenStarted?.()
    unlistenStopped?.()
  })
}
