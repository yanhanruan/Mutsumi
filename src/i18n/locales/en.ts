import type { Translations } from '../types'

export const en: Translations = {
  // ── Settings window ──────────────────────────────────────────────
  settingsTitle:     'settings',
  pomodoro:          'Pomodoro',
  focusLabel:        'Focus',
  breakLabel:        'Break',
  minuteUnit:        'min',
  petStatus:         'Pet status',
  energy:            'Energy',
  affection:         'Affection',
  mood:              'Mood',
  system:            'System',
  launchOnStartup:   'Launch on startup',
  save:              'Save',
  resetPet:          'Reset pet',
  close:             'Close',
  savedMsg:          'Saved.',
  resetMsg:          'Reset.',
  autostartOnMsg:    'Launch on startup enabled.',
  autostartOffMsg:   'Launch on startup disabled.',

  // ── Pomodoro badge ────────────────────────────────────────────────
  pomFocus: 'Focus',
  pomBreak: 'Break',

  // ── Pet click reactions ───────────────────────────────────────────
  clickPhrases: [
    "I'm not a toy, you know!",
    'Ahh~ be gentle!',
    'What do you want~?',
    'Eek! That tickles!',
    'H-Hey! Stop that!',
  ],

  // ── Context-menu action labels ────────────────────────────────────
  contextMenuItems: {
    pat_head:      'Pat Head',
    feed:          'Feed Matcha Parfait',
    sleep:         'Sleep',
    fast_learning: 'Fast Learning',
  },

  // ── Context-menu response bubbles ─────────────────────────────────
  contextResponses: {
    pat_head:      'Ehehe~ that feels nice ♪',
    feed:          'Matcha parfait!! My favourite! 🍵',
    sleep:         "Zzz… (snoozin' away…)",
    fast_learning: "I-I'll study extra hard! 📚✨",
  },

  // ── Late-night reminder ───────────────────────────────────────────
  lateNightReminder: "You shouldn't stay up so late!",
}
