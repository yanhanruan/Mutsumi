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
}
