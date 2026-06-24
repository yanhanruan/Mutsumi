import type { Translations } from '../types'

export const zh: Translations = {
  // ── Settings window ──────────────────────────────────────────────
  settingsTitle:     '设置',
  pomodoro:          '番茄钟',
  focusLabel:        '专注',
  breakLabel:        '休息',
  minuteUnit:        '分钟',
  petStatus:         '若叶睦状态',
  energy:            '能量',
  affection:         '亲密度',
  mood:              '心情',
  system:            '系统',
  launchOnStartup:   '开机启动',
  save:              '保存',
  resetPet:          '重置若叶睦',
  close:             '关闭',
  savedMsg:          '已保存。',
  resetMsg:          '已重置。',
  autostartOnMsg:    '已开启开机启动。',
  autostartOffMsg:   '已关闭开机启动。',

  // ── About window ─────────────────────────────────────────────────
  aboutTitle:             '关于',
  aboutVersionInfo:       '版本信息',
  aboutAppSummary:        '应用简介',
  aboutAppSummaryBody:    'Mutsumi 是一款轻量的桌面陪伴应用，让若叶睦常驻在桌面上，陪你专注、休息和获得一点温柔的日常反馈。',
  aboutMainFeatures:      '主要功能',
  aboutFeaturesList:      ['桌面角色陪伴与互动', '番茄钟专注计时', '塔罗抽卡与记录', '天气提示与状态显示'],
  aboutDeveloper:         '开发者',
  aboutContact:           '联系方式',
  aboutCopyright:         '版权声明',
  aboutLatestReleaseLead: '最新版本可以前往',
  aboutSourceCodeLead:    '代码可以在',
  aboutLatestReleaseLink: 'GitHub Releases',
  aboutSourceCodeLink:    'GitHub 仓库',
  aboutDeveloperYoho:     'yOHO',
  aboutDeveloperMutsumiHead: '-睦头人おれ.',
  aboutContactQqLabel:    'QQ',
  aboutCopyrightMit:      'MIT License',

  // ── Character size ────────────────────────────────────────────────
  characterSize:   '角色大小',
  charSizeSmall:   '小',
  charSizeMedium:  '中',
  charSizeLarge:   '大',

  // ── Language ──────────────────────────────────────────────────────
  language: '语言',

  // ── Weather visibility ────────────────────────────────────────────
  showWeather: '显示天气',

  // ── Music controller visibility ───────────────────────────────────
  showMusic: '显示音乐控制器',

  // ── Search engine ─────────────────────────────────────────────────
  searchEngine: '搜索引擎',
  searchEngines: {
    duckduckgo: 'DuckDuckGo',
    bingCn:     'Bing 国内',
    bing:       'Bing 国际',
    google:     'Google',
    baidu:      '百度',
  },
  searchEnabled:     '联网搜索',
  searchEnabledHint: '开启后，遇到时事/实时类问题时会联网检索作为参考；关闭则只凭模型与记忆回答（更快）。',

  // ── 对话模型 ──────────────────────────────────────────────────────
  chatModel:       '对话模型',
  chatModelHint:   '切换驱动聊天的模型：max 更强、flash 更快、plus 兼顾质量与速度。',
  modelDefaultTag: '默认',

  // ── 聊天记忆重置 ──────────────────────────────────────────────────
  chatMemory:         '聊天记忆',
  chatMemoryHint:     '忘记睦记住的关于你的一切，从头开始。',
  clearMemory:        '清空记忆',
  clearMemoryConfirm: '再点一次确认',
  clearMemoryDoneMsg: '记忆已清空。',

  // ── 通义千问 / 百炼 API Key ────────────────────────────────────────
  apiKey:               '通义千问 API Key',
  apiKeyHint:           '聊天功能需要申请阿里云百炼（有免费额度）API Key。在百炼控制台创建后粘贴到这里，安全加密，仅保存在本机，配置后立即生效。',
  apiKeyPlaceholder:    'sk-... 粘贴你的 API Key',
  apiKeySetPlaceholder: '已配置 · 粘贴可替换',
  apiKeyStatusSet:      '✓ 已配置',
  apiKeyStatusUnset:    '尚未配置——聊天将无法使用',
  apiKeyClear:          '清除',
  apiKeySavedMsg:       'API Key 已保存。',
  apiKeyClearedMsg:     'API Key 已清除。',
  apiKeyHelp:           '如何获取 API Key？',

  // ── Pomodoro badge ────────────────────────────────────────────────
  pomFocus: '专注',
  pomBreak: '休息',

  // ── Context-menu action labels ────────────────────────────────────
  contextMenuItems: {
    pat_head:      '摸头',
    feed:          '投喂抹茶芭菲',
    sleep:         '睡觉',
    fast_learning: '快速学习',
    tarot:         '塔罗占卜',
    chat:          '聊天',
    hide:          '隐藏应用',
  },

  // ── Context-menu response bubbles ─────────────────────────────────
  contextResponses: {
    pat_head:      '嘿嘿~好舒服♪',
    feed:          '抹茶芭菲！！最喜欢了！🍵',
    sleep:         '呼呼……（睡着了……）',
    fast_learning: '我、我会努力学习的！📚✨',
  },

  // ── Late-night reminder ───────────────────────────────────────────
  music: {
    unknownTitle:  '未知曲目',
    unknownArtist: '未知歌手',
    prev:          '上一首',
    play:          '播放',
    pause:         '暂停',
    next:          '下一首',
    replay:        '重播',
    skipBack:      '后退10秒',
    skipForward:   '快进10秒',
    mute:          '静音',
    unmute:        '取消静音',
    volume:        '音量',
    source:        '音频来源',
    autoSource:    '自动（活动）',
    unknownSource: '未知应用',
  },

  lateNightReminder: '都这么晚了，早点睡吧！',

  // ── Tarot overlay UI chrome ───────────────────────────────────────
  tarot: {
    interpreting: '正在解读星象……',
    hint:         '轻触卡牌，揭示你的运势。',
    upright:      '正位',
    reversed:     '逆位',
    redraw:       '重抽',
    download:     '下载卡牌',
    saved:        '已保存到下载文件夹',
    openFolder:   '打开文件夹',
    close:        '关闭',
    today:        '今日卡牌',
    history:      '最近',
    empty:        '还没有占卜记录。',
  },

  // ── Chat overlay UI chrome ────────────────────────────────────────
  chat: {
    title:       '和睦聊天',
    placeholder: '说点什么……',
    send:        '发送',
    thinking:    '……',
    empty:       '她在听着。',
    close:       '关闭',
    minimize:    '最小化',
    error:       '出了点问题。',
    errorNetwork:             '……连不上网络，检查下网络连接？',
    errorTimeout:            '……等太久了，等会儿再试试。',
    errorApiKeyMissing:       '……还没填 API Key 呢，去设置里配置一下。',
    errorApiKeyInvalid:       '……这个 API Key 好像不对，检查一下？',
    errorInsufficientBalance: '……账户余额或额度用完了。',
    errorContentFiltered:     '……这个我没办法回应。',
    errorRateLimited:         '……太快啦，喘口气再问？',
    errorServer:             '……服务器那边出问题了，等会儿再试。',
    attachFile:  '添加文件',
    voiceInput:  '语音输入',
    voiceListening:  '正在聆听…点击停止',
    voiceUnsupported: '当前环境不支持语音输入',
    jumpToLatest:    '回到最新',
    history:         '历史记录',
    historyClose:    '返回对话',
    searchPlaceholder: '搜索聊天记录…',
    dateFrom:        '从',
    dateTo:          '到',
    datePlaceholder: '选择日期',
    dateToday:       '今天',
    dateClear:       '清除',
    searchNoResults: '没有匹配的消息。',
    searchHint:      '按关键词搜索，或按日期筛选。',
    emoji:           '表情',
    emojiSearch:     '搜索表情…',
    emojiNoResults:  '没有找到表情。',
    attachImage:     '发送图片',
    imageTooLarge:   '……文件超重啦（超过 5MB）~ 🐱',
    imageBadType:    '只能是图片哦，呼~ 📸',
    imageTooMany:    '最多三张啦~',
    imageAlt:        '分享的图片',
    dropHint:        '把图片放到这里',
  },
}
