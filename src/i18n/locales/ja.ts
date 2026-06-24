import type { Translations } from '../types'

export const ja: Translations = {
  // ── Settings window ──────────────────────────────────────────────
  settingsTitle:     '設定',
  pomodoro:          'ポモドーロ',
  focusLabel:        '集中',
  breakLabel:        '休憩',
  minuteUnit:        '分',
  petStatus:         '若葉睦の状態',
  energy:            'エネルギー',
  affection:         '愛情度',
  mood:              '気分',
  system:            'システム',
  launchOnStartup:   '起動時に自動起動',
  save:              '保存',
  resetPet:          '若葉睦をリセット',
  close:             '閉じる',
  savedMsg:          '保存しました。',
  resetMsg:          'リセットしました。',
  autostartOnMsg:    '自動起動を有効にしました。',
  autostartOffMsg:   '自動起動を無効にしました。',

  // ── About window ─────────────────────────────────────────────────
  aboutTitle:             'このアプリについて',
  aboutVersionInfo:       'バージョン情報',
  aboutAppSummary:        'アプリ概要',
  aboutAppSummaryBody:    'Mutsumi は、若葉睦をデスクトップにそっと常駐させる軽量なデスクトップコンパニオンです。集中、休憩、日々の小さな反応をやさしく支えます。',
  aboutMainFeatures:      '主な機能',
  aboutFeaturesList:      ['デスクトップキャラクターとのふれあい', 'ポモドーロ集中タイマー', 'タロットカードと履歴', '天気と状態の表示'],
  aboutDeveloper:         '開発者',
  aboutContact:           '連絡先',
  aboutCopyright:         '著作権表示',
  aboutLatestReleaseLead: '最新版は',
  aboutSourceCodeLead:    'コードは',
  aboutLatestReleaseLink: 'GitHub Releases',
  aboutSourceCodeLink:    'GitHub リポジトリ',
  aboutDeveloperYoho:     'yOHO',
  aboutDeveloperMutsumiHead: '-睦头人おれ.',
  aboutContactQqLabel:    'QQ',
  aboutCopyrightMit:      'MIT License',

  // ── Character size ────────────────────────────────────────────────
  characterSize:   'キャラクターサイズ',
  charSizeSmall:   '小',
  charSizeMedium:  '中',
  charSizeLarge:   '大',

  // ── Language ──────────────────────────────────────────────────────
  language: '言語',

  // ── Weather visibility ────────────────────────────────────────────
  showWeather: '天気を表示',

  // ── Pomodoro badge ────────────────────────────────────────────────
  pomFocus: '集中',
  pomBreak: '休憩',

  // ── Context-menu action labels ────────────────────────────────────
  contextMenuItems: {
    pat_head:      'なでなで',
    feed:          '抹茶パフェをあげる',
    sleep:         'おやすみ',
    fast_learning: '速習モード',
    tarot:         'タロット占い',
    sys_state:     'システム状態',
    hide:          'アプリを隠す',
  },

  // ── Context-menu response bubbles ─────────────────────────────────
  contextResponses: {
    pat_head:      'えへへ〜きもちいい♪',
    feed:          '抹茶パフェ！！だいすき！🍵',
    sleep:         'zzz……（すやすや……）',
    fast_learning: 'がんばって勉強するよ！📚✨',
  },

  // ── Late-night reminder ───────────────────────────────────────────
  lateNightReminder: '夜更かしは健康によくない！',

  // ── Tarot overlay UI chrome ───────────────────────────────────────
  tarot: {
    interpreting: '星を読み解いています…',
    hint:         'カードをタップして運勢を表示。',
    upright:      '正位置',
    reversed:     '逆位置',
    redraw:       '引き直す',
    download:     'カードを保存',
    saved:        'ダウンロードに保存しました',
    openFolder:   'フォルダを開く',
    close:        '閉じる',
    today:        '今日のカード',
    history:      '最近',
    empty:        'まだ占いの記録がありません。',
  },

  // ── System State Overlay ────────────────────────────────────────
  sys: {
    title:       'システム状態',
    cpu:         'CPU',
    memory:      'メモリ',
    network:     'ネットワーク',
    online:      'オンライン',
    offline:     'オフライン',
    wifi:        'Wi-Fi',
    ethernet:    '有線LAN',
    uptime:      '起動時間',
    battery:     'バッテリー',
    charging:    '充電中 (完了まで {time})',
    chargingPlain: '充電中',
    discharging: '使用中 (残り {time})',
    dischargingPlain: '使用中',
    pluggedIn:   '電源接続中',
    tabStatus:   '状態',
    tabHardware: 'ハードウェア',
    hw: {
      loading:    'ハードウェア情報を読み込み中…',
      error:      'ハードウェア情報を取得できませんでした。',
      cores:      'コア',
      threads:    'スレッド',
      frequency:  '周波数',
      total:      '合計',
      used:       '使用中',
      available:  '空き',
      gpu:        'GPU',
      vram:       'VRAM',
      storage:    'ストレージ',
      ssd:        'SSD',
      hdd:        'HDD',
      partitions: 'パーティション',
      filesystem: 'ファイルシステム',
      free:       '空き',
    },
  },
}
