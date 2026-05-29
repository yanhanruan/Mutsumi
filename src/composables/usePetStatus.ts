/**
 * usePetStatus — listens to pet-state-update events and drives the
 * correct idle animation variant based on energy/affection levels.
 *
 * Mirrors the IdleVariant logic from the Rust backend (state.rs):
 *   - energy ≤ 30 && affection > 25  → idle_low_energy
 *   - energy > 30 && affection ≤ 25  → idle_low_affection
 *   - energy ≤ 30 && affection ≤ 25  → idle_exhausted
 *   - both above threshold            → idle (normal)
 *
 * The backend includes `idle_variant` in every `pet-state-update` payload.
 * This composable reads it and calls `setIdleVariant()` from useAnimator so
 * the state machine always falls back to the correct idle tier.
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import type { AnimationName } from './useAnimator'

// ── Types ──────────────────────────────────────────────────────────

export type IdleVariantName = 'normal' | 'low_energy' | 'low_affection' | 'exhausted'

export interface PetStatusSnapshot {
  energy:       number
  affection:    number
  mood:         string
  idle_variant: IdleVariantName
}

/** Map from the Rust IdleVariant snake_case to the Vue AnimationName. */
const VARIANT_TO_ANIM: Record<IdleVariantName, AnimationName> = {
  normal:         'idle',
  low_energy:     'idle_low_energy',
  low_affection:  'idle_low_affection',
  exhausted:      'idle_exhausted',
}

interface StateSnapshot {
  pet:      PetStatusSnapshot
  pomodoro: unknown
}

// ── Composable ─────────────────────────────────────────────────────

export function usePetStatus(
  setIdleVariant: (name: AnimationName) => void,
) {
  const energy      = ref<number>(100)
  const affection   = ref<number>(50)
  const mood        = ref<string>('content')
  const idleVariant = ref<IdleVariantName>('normal')

  function applySnapshot(snap: PetStatusSnapshot) {
    energy.value      = snap.energy
    affection.value   = snap.affection
    mood.value        = snap.mood
    idleVariant.value = snap.idle_variant
    setIdleVariant(VARIANT_TO_ANIM[snap.idle_variant] ?? 'idle')
  }

  let unlisten: UnlistenFn | null = null

  onMounted(async () => {
    // Pull initial state so the correct idle variant is set before the first event.
    try {
      const s = await invoke<StateSnapshot>('get_state')
      applySnapshot(s.pet)
    } catch { /* live events will correct state */ }

    unlisten = await listen<PetStatusSnapshot>('pet-state-update', e => {
      applySnapshot(e.payload)
    })
  })

  onUnmounted(() => {
    unlisten?.()
  })

  return { energy, affection, mood, idleVariant }
}
