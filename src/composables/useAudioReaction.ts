/**
 * useAudioReaction — translates the backend's audio events into animator
 * inputs. That is its whole job; it holds no animation data (classification
 * sets live in src/config/animations.ts) and makes no playback decisions
 * (those are the animator's).
 *
 * For every audio transition it does exactly two things:
 *   1. setAudioActive(…) — update the animator's mirror of the backend
 *      AudioState, unconditionally. Requests below can be gated (sleep/
 *      flight) or interrupted (pat_head); the mirror never is, and the
 *      animator's baseline resolver relies on it to restore music mode when
 *      those states end.
 *   2. queue/cancel the matching enter/exit animation for LIVE transitions.
 *
 * The Rust side (audio.rs) does continuity-based detection: it only emits
 * audio-started after 3 s of sustained activity and audio-stopped after 6 s
 * of sustained silence, so no debounce is needed here. What is needed is
 * reconciling against the pending slot. Race example without the
 * cancel-pending guard:
 *   1. music playing, user pauses audio for 7 s
 *   2. audio-stopped fires → queueAnim('headphones_off') → pending set
 *   3. user resumes audio before the music loop ends
 *   4. audio-started fires while currentName is still 'music'
 *   5. Without cancelling the pending, the music loop ends, consumes the
 *      stale 'headphones_off', and the pet drops to idle — even though
 *      audio is playing.
 * The guards below mirror Python's _begin / _end_music_sequence.
 */
import { onMounted, onUnmounted, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { MUSIC_MODE_ANIMS, type AnimationName } from '../config/animations'

export function useAudioReaction(
  queueAnim:      (name: AnimationName) => void,
  currentName:    Ref<AnimationName>,
  getPending:     () => AnimationName | null,
  cancelPending:  () => void,
  setAudioActive: (active: boolean) => void,
) {
  let unlistenStarted: UnlistenFn | null = null
  let unlistenStopped: UnlistenFn | null = null

  function onAudioStarted() {
    setAudioActive(true)
    if (MUSIC_MODE_ANIMS.has(currentName.value)) {
      // Already in / entering music. A pending 'headphones_off' here means a
      // stop event fired earlier and is still queued — cancel it so the music
      // chain doesn't drop out when the current loop ends.
      if (getPending() === 'headphones_off') {
        cancelPending()
      }
    } else {
      // In idle, click, or headphones_off (i.e., currently exiting music).
      // Queue headphones_on to enter / re-enter. The pending slot overrides
      // the headphones_off ending, so this also correctly handles the
      // "audio resumed mid-exit" case.
      queueAnim('headphones_on')
    }
  }

  function onAudioStopped() {
    setAudioActive(false)
    if (MUSIC_MODE_ANIMS.has(currentName.value)) {
      queueAnim('headphones_off')
    } else if (getPending() === 'headphones_on') {
      // Audio started briefly then stopped before headphones_on landed —
      // cancel the queued entrance.
      cancelPending()
    }
  }

  onMounted(async () => {
    // Pull the cached Rust state first.  If audio-started fired before this
    // listener registered (WebView2 cold-start can take 1-5 s; the tracker
    // threshold fires at poll #6 = 3 s), the event is already gone and
    // ContinuityTracker will never re-emit it.  Reading the managed
    // AudioState here covers that window.
    const alreadyPlaying = await invoke<boolean>('get_audio_state')
    setAudioActive(alreadyPlaying)
    if (alreadyPlaying && !MUSIC_MODE_ANIMS.has(currentName.value)) {
      queueAnim('headphones_on')
    }

    unlistenStarted = await listen('audio-started', onAudioStarted)
    unlistenStopped = await listen('audio-stopped', onAudioStopped)
  })

  onUnmounted(() => {
    unlistenStarted?.()
    unlistenStopped?.()
  })
}
