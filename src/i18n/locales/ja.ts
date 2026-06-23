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

  // ── Music controller visibility ───────────────────────────────────
  showMusic: '音楽コントローラーを表示',

  // ── Search engine ─────────────────────────────────────────────────
  searchEngine: '検索エンジン',
  searchEngines: {
    duckduckgo: 'DuckDuckGo',
    bingCn:     'Bing（中国）',
    bing:       'Bing',
    google:     'Google',
    baidu:      'Baidu',
  },
  searchEnabled:     'ウェブ検索',
  searchEnabledHint: 'オンの時は時事・リアルタイムな質問でネット検索を参照します。オフなら睦の知識と記憶のみで返答します（高速）。',

  // ── チャットモデル ────────────────────────────────────────────────
  chatModel:       'チャットモデル',
  chatModelHint:   'チャットを動かすモデルを切り替えます：max は最も高性能、flash は最速、plus は品質と速度のバランス型。',
  modelDefaultTag: 'デフォルト',

  // ── チャットの記憶リセット ────────────────────────────────────────
  chatMemory:         'チャットの記憶',
  chatMemoryHint:     '睦が覚えたあなたのことをすべて忘れ、最初からやり直します。',
  clearMemory:        '記憶を消去',
  clearMemoryConfirm: 'もう一度押して確定',
  clearMemoryDoneMsg: '記憶を消去しました。',

  // ── Qwen / 百錬 (DashScope) API キー ──────────────────────────────
  apiKey:               'Qwen API キー',
  apiKeyHint:           'チャット機能を利用するには、阿里雲（Alibaba Cloud）百炼（Bailian）のAPIキー（無料利用枠あり）が必要です。百炼コンソールでAPIキーを作成し、ここに貼り付けてください。APIキーは安全に暗号化され、本端末内にのみ保存されます。設定後、すぐに有効になります。',
  apiKeyPlaceholder:    'sk-... API キーを貼り付け',
  apiKeySetPlaceholder: '設定済み · 貼り付けて置き換え',
  apiKeyStatusSet:      '✓ 設定済み',
  apiKeyStatusUnset:    '未設定 — チャットは利用できません',
  apiKeyClear:          'クリア',
  apiKeySavedMsg:       'API キーを保存しました。',
  apiKeyClearedMsg:     'API キーをクリアしました。',
  apiKeyHelp:           'API キーの取得方法は？',

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
    chat:          'おしゃべり',
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
  music: {
    unknownTitle:  '不明な曲',
    unknownArtist: '不明なアーティスト',
    prev:          '前の曲',
    play:          '再生',
    pause:         '一時停止',
    next:          '次の曲',
    replay:        '最初から',
    skipBack:      '10秒戻る',
    skipForward:   '10秒進む',
    mute:          'ミュート',
    unmute:        'ミュート解除',
    volume:        '音量',
    source:        'オーディオソース',
    autoSource:    '自動（再生中）',
    unknownSource: '不明なアプリ',
  },

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

  // ── Chat overlay UI chrome ────────────────────────────────────────
  chat: {
    title:       '睦とおしゃべり',
    placeholder: '話しかけてみて…',
    send:        '送信',
    thinking:    '……',
    empty:       '聞いているよ。',
    close:       '閉じる',
    minimize:    '最小化',
    error:       '問題が発生しました。',
    attachFile:  'ファイルを添付',
    voiceInput:  '音声入力',
    voiceListening:  '聞き取り中…クリックで停止',
    voiceUnsupported: 'この環境では音声入力に対応していません',
    jumpToLatest:    '最新へ移動',
    history:         '履歴',
    historyClose:    'チャットに戻る',
    searchPlaceholder: 'メッセージを検索…',
    dateFrom:        '開始',
    dateTo:          '終了',
    datePlaceholder: '日付を選択',
    dateToday:       '今日',
    dateClear:       'クリア',
    searchNoResults: '一致するメッセージはありません。',
    searchHint:      'キーワード検索、または日付で絞り込み。',
    emoji:           '絵文字',
    emojiSearch:     '絵文字を検索…',
    emojiNoResults:  '絵文字が見つかりません。',
    attachImage:     '画像を送る',
    imageTooLarge:   '……重すぎ。5MB までにして。🐱',
    imageBadType:    '画像だけだよ、ふぅ~ 📸',
    imageTooMany:    '三枚までね~',
    imageAlt:        '共有された画像',
    dropHint:        'ここに画像をドロップ',
  },
}
