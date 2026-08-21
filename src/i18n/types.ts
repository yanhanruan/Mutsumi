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
  autostartUnavailable: string
  autostartFailedMsg:   string

  // ── About window ────────────────────────────────────────────────
  aboutTitle:             string
  aboutVersionInfo:       string
  aboutAppSummary:        string
  aboutAppSummaryBody:    string
  aboutMainFeatures:      string
  aboutFeaturesList:      string[]
  aboutDeveloper:            string
  aboutAcknowledgements:     string
  aboutAckList:              string[]
  aboutCopyright:            string
  aboutLatestReleaseLead:    string
  aboutSourceCodeLead:       string
  aboutLatestReleaseLink:    string
  aboutSourceCodeLink:       string
  aboutDeveloperYoho:        string
  aboutDeveloperMutsumiHead: string
  aboutDevRoleDev:           string
  aboutDevRolePromo:         string
  aboutDevPromo:             string
  aboutCopyrightMit:         string
  /** "Check for updates" button in the About window. */
  aboutCheckUpdates:         string
  /** "Last checked" label preceding the last update-check timestamp. */
  aboutLastChecked:          string
  /** Shown in place of a timestamp when the app has never checked. */
  aboutLastCheckedNever:     string

  // ── Update pop-up (auto-update) ─────────────────────────────────
  updateWindowTitle:          string
  updateAvailableTitle:       string
  updateCurrentVersionLabel:  string
  updateNewVersionLabel:      string
  updateReleaseNotesTitle:    string
  updateNowBtn:               string
  updateRemindLaterBtn:       string
  updateSnoozeDaysLabel:      string
  /** Reminder that the About page can update anytime, shown after snoozing. */
  updateSnoozeHint:           string
  /** Download progress line; contains a `{percent}` placeholder. */
  updateDownloading:          string
  updateInstalling:           string
  /** Shown after a successful install, just before the app relaunches. */
  updateInstalledRestarting:  string
  updateUpToDate:             string
  updateCheckFailed:          string
  /** Retry button on the failed view (re-check or re-offer the install). */
  updateRetryBtn:             string
  /** Status word appended to About's "last checked" when the check succeeded. */
  aboutCheckStatusSuccess:    string
  /** Status word appended to About's "last checked" when the check failed. */
  aboutCheckStatusError:      string
  /** Settings toggle: check for updates automatically. */
  updateAutoCheck:            string

  // ── Character size (Task 3) ─────────────────────────────────────
  characterSize:   string
  charSizeSmall:   string
  charSizeMedium:  string
  charSizeLarge:   string

  // ── Language (manual override) ──────────────────────────────────
  language: string

  // ── Weather visibility (Task 4) ─────────────────────────────────
  showWeather: string

  // ── Music controller visibility ─────────────────────────────────
  showMusic: string
  showAudioActivity: string
  audioActivityDegradedHint: string

  // ── Sleep "zzz" effect visibility ───────────────────────────────
  showZzz: string

  // ── Flying screensaver ──────────────────────────────────────────
  flyingScreensaver:     string
  flyingWaitLabel:       string
  /** Contains a `{keys}` placeholder for the platform-formatted shortcut. */
  flyingScreensaverHint: string
  flyingScreensaverUnavailable: string
  /** Shown when a global hotkey failed to register ({keys} = accelerator). */
  hotkeyUnavailable:     string

  // ── Search engine (chat search-enhancement) ─────────────────────
  searchEngine: string
  searchEngines: {
    duckduckgo: string
    bingCn:     string
    bing:       string
    google:     string
    baidu:      string
  }
  /** Master on/off toggle for web search. */
  searchEnabled:     string
  searchEnabledHint: string

  // ── Chat model ──────────────────────────────────────────────────
  chatModel:       string
  chatModelHint:   string
  modelDefaultTag: string

  // ── Chat memory reset ───────────────────────────────────────────
  chatMemory:           string
  chatMemoryHint:       string
  clearMemory:          string
  clearMemoryConfirm:   string
  clearMemoryDoneMsg:   string

  // ── Qwen / 百炼 (DashScope) API key ──────────────────────────────
  apiKey:               string
  apiKeyHint:           string
  apiKeyPlaceholder:    string
  apiKeySetPlaceholder: string
  apiKeyStatusSet:      string
  apiKeyStatusUnset:    string
  apiKeyClear:          string
  apiKeySavedMsg:       string
  apiKeyClearedMsg:     string
  apiKeySaveFailedMsg:  string
  apiKeyClearFailedMsg: string
  apiKeyCredentialStoreUnavailable: string
  apiKeyHelp:           string

  // ── Pomodoro badge ──────────────────────────────────────────────
  pomFocus:  string
  pomBreak:  string

  // ── Context-menu action labels ──────────────────────────────────
  contextMenuItems: {
    pat_head:      string
    feed:          string
    sleep:         string
    /** Shown in place of `sleep` while she is already asleep (toggle to wake). */
    wake:          string
    fast_learning: string
    /** Opens the in-pet tarot overlay (frontend-only). */
    tarot:         string
    /** Opens the in-pet chat overlay (frontend-only). */
    chat:          string
    /** Opens the system state overlay (frontend-only). */
    sys_state:     string
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

  // ── Chat overlay UI chrome ──────────────────────────────────────
  chat: {
    title:       string   // panel header
    placeholder: string   // text-input placeholder
    send:        string   // send-button title
    thinking:    string   // status shown while awaiting the first token
    empty:       string   // empty-conversation hint
    close:       string   // close-button title
    minimize:    string   // titlebar minimize-button tooltip
    error:       string   // generic failure line (fallback for unclassified errors)
    // ── Classified failure lines (mapped from the Rust ChatError code) ──
    errorNetwork:             string   // transport down / DNS / connection refused
    errorTimeout:             string   // request timed out
    errorApiKeyMissing:       string   // no API key configured
    errorApiKeyInvalid:       string   // key rejected by the provider (401)
    errorInsufficientBalance: string   // account out of credit / quota exhausted
    errorContentFiltered:     string   // input rejected by content moderation
    errorRateLimited:         string   // throttled — too many requests
    errorServer:              string   // provider-side 5xx
    attachFile:  string   // attach-file button tooltip (incomplete feature)
    voiceInput:  string   // voice-input button tooltip (idle)
    voiceListening:  string   // voice-input button tooltip while recording
    voiceUnsupported: string  // voice-input button tooltip when STT unavailable
    jumpToLatest: string  // "back to present" button (shown when browsing history)
    // ── History search panel ──
    history:          string   // history button tooltip + panel title
    historyClose:     string   // history panel close-button title
    searchPlaceholder: string  // keyword search input placeholder
    dateFrom:         string   // date-range "from" label
    dateTo:           string   // date-range "to" label
    datePlaceholder:  string   // date-picker trigger placeholder (no date chosen)
    dateToday:        string   // date-picker "today" action
    dateClear:        string   // date-picker "clear" action + clear-filters affordance
    searchNoResults:  string   // empty-results hint
    searchHint:       string   // initial hint before any query
    // ── Emoji picker ──
    emoji:            string   // emoji-button tooltip
    emojiSearch:      string   // emoji search input placeholder
    emojiNoResults:   string   // empty emoji-search hint
    // ── Image input ──
    attachImage:      string   // attach-image button tooltip
    imageTooLarge:    string   // in-character alert: a file exceeds 5MB
    imageBadType:     string   // in-character alert: not an accepted image type
    imageTooMany:     string   // in-character alert: more than the image cap
    imageAlt:         string   // <img> alt text for sent images
    dropHint:         string   // dashed drop-overlay caption
  }

  // ── Mini music controller ───────────────────────────────────────
  music: {
    unknownTitle:  string   // fallback when the track has no title
    unknownArtist: string   // fallback when the track has no artist
    prev:          string   // previous-track button tooltip
    play:          string   // play button tooltip
    pause:         string   // pause button tooltip
    next:          string   // next-track button tooltip
    replay:        string   // restart-current-track button tooltip
    skipBack:      string   // seek backward 10 s button tooltip
    skipForward:   string   // seek forward 10 s button tooltip
    mute:          string   // mute button tooltip
    unmute:        string   // unmute button tooltip
    volume:        string   // volume slider aria-label
    source:        string   // source-switcher button tooltip / heading
    autoSource:    string   // "Auto" entry — follow the active source
    unknownSource: string   // fallback app name when the source app id is empty
  }

  // ── Late-night reminder ─────────────────────────────────────────
  lateNightReminder: string

  // ── Sleep-talking ───────────────────────────────────────────────
  /** Random murmurs shown when she's poked while asleep (she stays asleep). */
  sleepTalk: string[]

  // ── System State Overlay ────────────────────────────────────────
  sys: {
    title:       string
    cpu:         string
    memory:      string
    network:     string
    online:      string
    offline:     string
    unavailable: string
    wifi:        string
    ethernet:    string
    uptime:      string
    battery:     string
    charging:    string
    chargingPlain: string
    discharging: string
    dischargingPlain: string
    pluggedIn:   string
    /** Tab switching between live status and static hardware specs. */
    tabStatus:   string
    tabHardware: string
    /** Hardware specs page (static CPU/RAM/GPU/storage info). */
    hw: {
      loading:    string
      error:      string
      cores:      string
      threads:    string
      frequency:  string
      total:      string
      used:       string
      available:  string
      gpu:        string
      vram:       string
      storage:    string
      ssd:        string
      hdd:        string
      partitions: string
      filesystem: string
      free:       string
    }
  }
}
