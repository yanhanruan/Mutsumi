import type { Translations } from '../types'

export const ja: Translations = {
  // ── Settings window ──────────────────────────────────────────────
  settingsTitle:     '設定',
  pomodoro:          'ポモドーロ',
  focusLabel:        '集中',
  breakLabel:        '休憩',
  minuteUnit:        '分',
  petStatus:         'ペットの状態',
  energy:            'エネルギー',
  affection:         '愛情度',
  mood:              '気分',
  system:            'システム',
  launchOnStartup:   '起動時に自動起動',
  save:              '保存',
  resetPet:          'ペットをリセット',
  close:             '閉じる',
  savedMsg:          '保存しました。',
  resetMsg:          'リセットしました。',
  autostartOnMsg:    '自動起動を有効にしました。',
  autostartOffMsg:   '自動起動を無効にしました。',

  // ── Pomodoro badge ────────────────────────────────────────────────
  pomFocus: '集中',
  pomBreak: '休憩',

  // ── Pet click reactions ───────────────────────────────────────────
  clickPhrases: [
    'おもちゃじゃないんだから！',
    'あっ〜やさしくして！',
    'なに、なに〜？',
    'ひゃ！くすぐったい！',
    'ちょっと！やめてよ！',
  ],

  // ── Context-menu action labels ────────────────────────────────────
  contextMenuItems: {
    pat_head:      'なでなで',
    feed:          '抹茶パフェをあげる',
    sleep:         'おやすみ',
    fast_learning: '速習モード',
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
}
