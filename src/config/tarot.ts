/**
 * tarot.ts — Tarot feature configuration (single source of truth).
 *
 * ─────────────────────────────────────────────────────────────────────
 *  ASSET-SWAP INTERFACE (Task 3)
 * ─────────────────────────────────────────────────────────────────────
 * Every image URL and audio file path the tarot module touches is defined
 * HERE and nowhere else. When official art / audio arrive, edit only the
 * `TAROT_ASSETS` block below (and optionally each card's `image`); no
 * component logic needs to change.
 *
 * Placeholder strategy while assets are missing:
 *   - Images: an empty string ('') means "no art yet" → the component
 *     renders a code-generated CSS gradient derived from each card's `hue`.
 *     To use a real/remote image instead, set the path (e.g.
 *     `/assets/tarot/00_fool.webp` or a https URL).
 *   - Audio: empty paths mean "no audio yet" → useTarotSound falls back to
 *     a synthesized Web Audio beep + console.log. Set a real path to play
 *     an actual file.
 */

// ── Asset paths (edit these when real assets land) ──────────────────────

export const TAROT_ASSETS = {
  /** Card-back artwork. '' → CSS gradient placeholder. */
  cardBackImage: '',

  /**
   * Optional builder for a card's front artwork from its id. Return '' to
   * fall back to the per-card gradient. Swap the body when real files exist,
   * e.g. `id => \`/assets/tarot/\${String(id).padStart(2, '0')}.webp\``.
   */
  cardFrontImage: (_id: number): string => '',

  /** Sound effect file paths. '' → synthesized Web Audio beep placeholder. */
  audio: {
    draw: '',
    flip: '',
  },
} as const

// ── Timing knobs ────────────────────────────────────────────────────────

export const TAROT_TIMINGS = {
  /** 3D flip duration (ms). Must match the CSS transition in TarotDraw.vue. */
  flipMs: 700,
  /** Lower / upper bound of the simulated "interpreting" delay (ms). */
  loadingMinMs: 1000,
  loadingMaxMs: 2000,
} as const

// ── Card data model ─────────────────────────────────────────────────────

export interface TarotCard {
  /** 0–21, the Major Arcana index. */
  id: number
  card_name: string
  /**
   * Placeholder/real front image URL. '' → render a CSS gradient using `hue`.
   * Kept per-card so individual cards can be swapped independently of the
   * global `TAROT_ASSETS.cardFrontImage` builder.
   */
  image_placeholder: string
  /** Base hue (0–360) for the generated gradient when no image is set. */
  hue: number
  /** Preset fortune interpretation (placeholder text for testing). */
  fortune_text: string
}

// ── The 22 Major Arcana (static config — no API, no network) ────────────

export const MAJOR_ARCANA: readonly TarotCard[] = [
  { id: 0,  card_name: 'The Fool',           image_placeholder: '', hue: 48,  fortune_text: 'A fresh beginning calls. Step off the cliff with an open heart — today rewards the curious and the brave.' },
  { id: 1,  card_name: 'The Magician',       image_placeholder: '', hue: 12,  fortune_text: 'You hold every tool you need. Focus your will and what felt scattered will snap into a single, clear intention.' },
  { id: 2,  card_name: 'The High Priestess', image_placeholder: '', hue: 250, fortune_text: 'Trust the quiet voice beneath the noise. The answer you seek is already known to you — listen inward.' },
  { id: 3,  card_name: 'The Empress',        image_placeholder: '', hue: 330, fortune_text: 'Abundance is ripening around you. Nurture an idea, a bond, or yourself, and watch it bloom in kind.' },
  { id: 4,  card_name: 'The Emperor',        image_placeholder: '', hue: 0,   fortune_text: 'Structure brings freedom. Set a firm boundary or a steady plan today and the chaos will fall into line.' },
  { id: 5,  card_name: 'The Hierophant',     image_placeholder: '', hue: 36,  fortune_text: 'Wisdom from tradition or a mentor lights the path. Honor what has been proven before improvising anew.' },
  { id: 6,  card_name: 'The Lovers',         image_placeholder: '', hue: 345, fortune_text: 'A choice of the heart draws near. Align your actions with your values and the right connection deepens.' },
  { id: 7,  card_name: 'The Chariot',        image_placeholder: '', hue: 210, fortune_text: 'Victory favors momentum. Grip the reins of opposing forces and drive straight toward your goal.' },
  { id: 8,  card_name: 'Strength',           image_placeholder: '', hue: 30,  fortune_text: 'Gentle courage outlasts brute force. Meet today’s challenge with patience and a calm, steady hand.' },
  { id: 9,  card_name: 'The Hermit',         image_placeholder: '', hue: 200, fortune_text: 'Step back from the crowd. A little solitude will turn your inner lamp up bright enough to see the next step.' },
  { id: 10, card_name: 'Wheel of Fortune',   image_placeholder: '', hue: 280, fortune_text: 'The wheel turns in your favor. A shift in luck arrives — ride the upswing and stay flexible as it spins.' },
  { id: 11, card_name: 'Justice',            image_placeholder: '', hue: 190, fortune_text: 'Balance is restored through honesty. Speak plainly and act fairly; what you put out returns in equal measure.' },
  { id: 12, card_name: 'The Hanged Man',     image_placeholder: '', hue: 170, fortune_text: 'Pause and shift your view. Surrendering control for a moment reveals the answer that striving could not.' },
  { id: 13, card_name: 'Death',              image_placeholder: '', hue: 270, fortune_text: 'An ending clears the way. Release what has run its course and a powerful transformation can finally begin.' },
  { id: 14, card_name: 'Temperance',         image_placeholder: '', hue: 160, fortune_text: 'Blend extremes into harmony. Moderation and patience today brew exactly the calm you have been craving.' },
  { id: 15, card_name: 'The Devil',          image_placeholder: '', hue: 350, fortune_text: 'Notice the chains you chose. A habit or attachment binds you only as tightly as you permit — you can step free.' },
  { id: 16, card_name: 'The Tower',          image_placeholder: '', hue: 8,   fortune_text: 'A sudden jolt shakes loose what was unstable. It feels abrupt, but this clearing makes room for truer ground.' },
  { id: 17, card_name: 'The Star',           image_placeholder: '', hue: 195, fortune_text: 'Hope returns, soft and bright. After the storm, healing flows freely — make a wish and trust the calm ahead.' },
  { id: 18, card_name: 'The Moon',           image_placeholder: '', hue: 235, fortune_text: 'Not all is as it seems tonight. Move slowly through the fog; let intuition, not fear, guide your footing.' },
  { id: 19, card_name: 'The Sun',            image_placeholder: '', hue: 45,  fortune_text: 'Pure joy and clarity shine down. Success and warmth are within reach — let yourself fully enjoy the light.' },
  { id: 20, card_name: 'Judgement',          image_placeholder: '', hue: 215, fortune_text: 'A call to rise renews you. Reflect, forgive, and answer honestly; a fresh chapter awaits your yes.' },
  { id: 21, card_name: 'The World',          image_placeholder: '', hue: 140, fortune_text: 'A cycle completes in triumph. You have come full circle — celebrate the achievement, then step into the next.' },
]
