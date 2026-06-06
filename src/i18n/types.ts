/**
 * Canonical shape of a locale's translation bundle.
 * Every locale file must satisfy this interface completely —
 * no optional keys, so missing translations are a compile error.
 */
export interface Translations {
  // ── Settings window ────────────────────────────────────────────
  settingsTitle:     string
  pomodoro:          string
  focusLabel:        string
  breakLabel:        string
  minuteUnit:        string
  /** Replaces "Pet status" — now "Mutsumi status" / "若叶睦状态" / "若葉睦の状態". */
  petStatus:         string
  energy:            string
  affection:         string
  mood:              string
  system:            string
  launchOnStartup:   string
  save:              string
  /** Replaces "Reset pet" — now "Reset Mutsumi" / "重置若叶睦" / "若葉睦をリセット". */
  resetPet:          string
  close:             string
  savedMsg:          string
  resetMsg:          string
  autostartOnMsg:    string
  autostartOffMsg:   string

  // ── Character size (Task 3) ─────────────────────────────────────
  characterSize:   string
  charSizeSmall:   string
  charSizeMedium:  string
  charSizeLarge:   string

  // ── Language (manual override) ──────────────────────────────────
  language: string

  // ── Weather visibility (Task 4) ─────────────────────────────────
  showWeather: string

  // ── Pomodoro badge ──────────────────────────────────────────────
  pomFocus:  string
  pomBreak:  string

  // ── Context-menu action labels ──────────────────────────────────
  contextMenuItems: {
    pat_head:      string
    feed:          string
    sleep:         string
    fast_learning: string
    /** Opens the in-pet tarot overlay (frontend-only). */
    tarot:         string
    /** Task 5: hide the main window to tray. */
    hide:          string
  }

  // ── Context-menu response bubbles ───────────────────────────────
  contextResponses: {
    pat_head:      string
    feed:          string
    sleep:         string
    fast_learning: string
  }

  // ── Tarot overlay UI chrome ─────────────────────────────────────
  // (Card names + interpretations are localized in src/config/tarot.ts.)
  tarot: {
    interpreting: string   // loading line while the reading "computes"
    hint:         string   // prompt to tap the face-down card
    upright:      string   // orientation badge
    reversed:     string   // orientation badge
    redraw:       string   // control button title
    download:     string   // control button title (save card image)
    saved:        string   // toast after a successful download
    openFolder:   string   // toast link — reveal the saved file's folder
    close:        string   // control button title
    today:        string   // "card of the day" badge
    history:      string   // history toggle / panel heading
    empty:        string   // empty-history placeholder
  }

  // ── Late-night reminder ─────────────────────────────────────────
  lateNightReminder: string
}
