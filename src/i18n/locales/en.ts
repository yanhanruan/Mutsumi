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

  // ── Music controller visibility ───────────────────────────────────
  showMusic: 'Show Music Controller',

  // ── Search engine ─────────────────────────────────────────────────
  searchEngine: 'Search Engine',
  searchEngines: {
    duckduckgo: 'DuckDuckGo',
    bingCn:     'Bing (China)',
    bing:       'Bing',
    google:     'Google',
    baidu:      'Baidu',
  },
  searchEnabled:     'Web search',
  searchEnabledHint: 'When on, current-events / real-time questions are looked up online for grounding; when off, replies use only the model and memory (faster).',

  // ── Chat model ────────────────────────────────────────────────────
  chatModel:       'Chat model',
  chatModelHint:   'Switch the model behind chat: max is strongest, flash is fastest, plus balances quality and speed.',
  modelDefaultTag: 'default',

  // ── Chat memory reset ─────────────────────────────────────────────
  chatMemory:         'Chat Memory',
  chatMemoryHint:     'Forget everything Mutsumi has learned about you and start fresh.',
  clearMemory:        'Clear memory',
  clearMemoryConfirm: 'Tap again to confirm',
  clearMemoryDoneMsg: 'Memory cleared.',

  // ── Qwen / Bailian (DashScope) API key ────────────────────────────
  apiKey:               'Qwen API Key',
  apiKeyHint:           'The chat feature requires an API Key from Alibaba Cloud Bailian (free quota available). Create an API Key in the Bailian Console and paste it here. It will be securely encrypted and stored only on your local device. The configuration takes effect immediately after being saved.',
  apiKeyPlaceholder:    'sk-... paste your API key',
  apiKeySetPlaceholder: 'Configured · paste to replace',
  apiKeyStatusSet:      '✓ Configured',
  apiKeyStatusUnset:    'Not set — chat will not work',
  apiKeyClear:          'Clear',
  apiKeySavedMsg:       'API key saved.',
  apiKeyClearedMsg:     'API key cleared.',
  apiKeyHelp:           'How to get an API key?',

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
    chat:          'Chat',
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
  music: {
    unknownTitle:  'Unknown track',
    unknownArtist: 'Unknown artist',
    prev:          'Previous',
    play:          'Play',
    pause:         'Pause',
    next:          'Next',
    replay:        'Replay',
    skipBack:      'Back 10s',
    skipForward:   'Forward 10s',
    mute:          'Mute',
    unmute:        'Unmute',
    volume:        'Volume',
    source:        'Audio source',
    autoSource:    'Auto (active)',
    unknownSource: 'Unknown app',
  },

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

  // ── Chat overlay UI chrome ────────────────────────────────────────
  chat: {
    title:       'Chat with Mutsumi',
    placeholder: 'Say something…',
    send:        'Send',
    thinking:    '…',
    empty:       'She is listening.',
    close:       'Close',
    minimize:    'Minimize',
    error:       'Something went wrong.',
    attachFile:  'Attach file',
    voiceInput:  'Voice input',
    voiceListening:  'Listening… click to stop',
    voiceUnsupported: 'Voice input is not supported in this environment',
    jumpToLatest:    'Jump to latest',
    history:         'History',
    historyClose:    'Back to chat',
    searchPlaceholder: 'Search messages…',
    dateFrom:        'From',
    dateTo:          'To',
    datePlaceholder: 'Pick a date',
    dateToday:       'Today',
    dateClear:       'Clear',
    searchNoResults: 'No matching messages.',
    searchHint:      'Search by keyword, or filter by date.',
    emoji:           'Emoji',
    emojiSearch:     'Search emoji…',
    emojiNoResults:  'No emoji found.',
    attachImage:     'Send an image',
    imageTooLarge:   '…too big, my paws can\'t lift it. Keep it under 5MB. 🐱',
    imageBadType:    'Images only, please~ 📸',
    imageTooMany:    'Three at most, okay~',
    imageAlt:        'Shared image',
    dropHint:        'Drop the image here',
  },
}
