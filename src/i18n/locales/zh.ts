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
}
