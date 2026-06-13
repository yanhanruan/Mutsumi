import type { Translations } from '../types'

export const en: Translations = {
  // ── Settings window ──────────────────────────────────────────────
  settingsTitle:     'settings',
  pomodoro:          'Pomodoro',
  focusLabel:        'Focus',
  breakLabel:        'Break',
  minuteUnit:        'min',
  petStatus:         'Mutsumi status',
  energy:            'Energy',
  affection:         'Affection',
  mood:              'Mood',
  system:            'System',
  launchOnStartup:   'Launch on startup',
  save:              'Save',
  resetPet:          'Reset Mutsumi',
  close:             'Close',
  savedMsg:          'Saved.',
  resetMsg:          'Reset.',
  autostartOnMsg:    'Launch on startup enabled.',
  autostartOffMsg:   'Launch on startup disabled.',

  // ── About window ─────────────────────────────────────────────────
  aboutTitle:             'about',
  aboutVersionInfo:       'Version Info',
  aboutAppSummary:        'App Summary',
  aboutAppSummaryBody:    'Mutsumi is a lightweight desktop companion that keeps Wakaba Mutsumi on your desktop, helping you focus, rest, and enjoy small everyday interactions.',
  aboutMainFeatures:      'Main Features',
  aboutFeaturesList:      ['Desktop character companion and interactions', 'Pomodoro focus timer', 'Tarot card draws and history', 'Weather hints and status display'],
  aboutDeveloper:         'Developer',
  aboutContact:           'Contact',
  aboutCopyright:         'Copyright',
  aboutLatestReleaseLead: 'Download the latest version from',
  aboutSourceCodeLead:    'Source code is available on',
  aboutLatestReleaseLink: 'GitHub Releases',
  aboutSourceCodeLink:    'GitHub',
  aboutDeveloperYoho:     'yOHO',
  aboutDeveloperMutsumiHead: '-睦头人おれ.',
  aboutContactQqLabel:    'QQ',
  aboutCopyrightMit:      'MIT License',

  // ── Character size ────────────────────────────────────────────────
  characterSize:   'Character Size',
  charSizeSmall:   'Small',
  charSizeMedium:  'Medium',
  charSizeLarge:   'Large',

  // ── Language ──────────────────────────────────────────────────────
  language: 'Language',

  // ── Weather visibility ────────────────────────────────────────────
  showWeather: 'Show Weather',

  // ── Pomodoro badge ────────────────────────────────────────────────
  pomFocus: 'Focus',
  pomBreak: 'Break',

  // ── Context-menu action labels ────────────────────────────────────
  contextMenuItems: {
    pat_head:      'Pat Head',
    feed:          'Feed Matcha Parfait',
    sleep:         'Sleep',
    fast_learning: 'Fast Learning',
    tarot:         'Tarot Reading',
    sys_state:     'System Status',
    hide:          'Hide App',
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

  // ── Tarot overlay UI chrome ───────────────────────────────────────
  tarot: {
    interpreting: 'Interpreting the stars…',
    hint:         'Tap the card to reveal your fortune.',
    upright:      'Upright',
    reversed:     'Reversed',
    redraw:       'Redraw',
    download:     'Download card',
    saved:        'Saved to Downloads',
    openFolder:   'Open folder',
    close:        'Close',
    today:        "Today's Card",
    history:      'Recent',
    empty:        'No readings yet.',
  },
}
