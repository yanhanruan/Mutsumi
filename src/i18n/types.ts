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

  // ── Weather visibility (Task 4) ─────────────────────────────────
  showWeather: string

  // ── Pomodoro badge ──────────────────────────────────────────────
  pomFocus:  string
  pomBreak:  string

  // ── Pet click reactions ─────────────────────────────────────────
  clickPhrases: readonly string[]

  // ── Context-menu action labels ──────────────────────────────────
  contextMenuItems: {
    pat_head:      string
    feed:          string
    sleep:         string
    fast_learning: string
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

  // ── Late-night reminder ─────────────────────────────────────────
  lateNightReminder: string
}
