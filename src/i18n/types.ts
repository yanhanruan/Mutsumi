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
  petStatus:         string
  energy:            string
  affection:         string
  mood:              string
  system:            string
  launchOnStartup:   string
  save:              string
  resetPet:          string
  close:             string
  savedMsg:          string
  resetMsg:          string
  autostartOnMsg:    string
  autostartOffMsg:   string

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
