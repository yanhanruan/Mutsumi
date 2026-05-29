/**
 * Mutsumi Wakaba (若葉睦 / 若叶睦) — BanG Dream! It's MyGO!!!!!
 *
 * A curated collection of her canonical lines from the anime alongside
 * character-authentic original quotes. Grouped by theme.
 *
 * Sources: anime dialogue (S1 ep 1-13, Ave Mujica), character profile.
 */

export interface QuoteEntry {
  en: string
  zh: string
  ja: string
  /** Optional: episode / scene the line is from, if canonical. */
  source?: string
}

// ── Canonical anime lines ───────────────────────────────────────────

const canonicalLines: QuoteEntry[] = [
  {
    en:     "I've never once thought that being in a band was fun.",
    zh:     "我从未觉得玩乐队是一件有趣的事。",
    ja:     "私は……バンド、楽しいって思ったこと一度もない。",
    source: "MyGO S1 ep.5 — Mutsumi's confession that sealed CRYCHIC's fate",
  },
  {
    en:     "Saki is about to break. That's why I'm joining.",
    zh:     "祥快撑不住了。所以我加入进来。",
    ja:     "祥が……壊れそうだから。だから私も入る。",
    source: "MyGO S1 ep.11 — Mutsumi decides to join Ave Mujica",
  },
  {
    en:     "Good for you.",
    zh:     "太好了呢。",
    ja:     "よかったね。",
    source: "MyGO S1 ep.12 — Mutsumi's quiet response to Soyo after the live",
  },
  {
    en:     "I don't need this!",
    zh:     "我不需要这个！",
    ja:     "いらない！",
    source: "MyGO S1 ep.12 — Soyo returns the cucumbers",
  },
]

// ── Character-authentic original quotes ─────────────────────────────

const originalLines: QuoteEntry[] = [
  {
    en: "Music is what connects hearts that words cannot reach.",
    zh: "音乐，是连接语言无法触达的心灵的纽带。",
    ja: "音楽は、言葉では届かない心と心を繋ぐものだと思う。",
  },
  {
    en: "Sometimes just sitting quietly together is enough.",
    zh: "有时候，只是静静地坐在一起，就已经足够了。",
    ja: "ただ静かに一緒にいるだけで、十分なこともある。",
  },
  {
    en: "The silences between notes tell a story too.",
    zh: "音符之间的沉默，也在诉说着故事。",
    ja: "音符と音符の間の沈黙にも、物語がある。",
  },
  {
    en: "You don't have to force yourself to smile.",
    zh: "你不必强迫自己微笑。",
    ja: "無理に笑わなくていいよ。",
  },
  {
    en: "Playing together means listening to each other.",
    zh: "一起演奏，意味着彼此倾听。",
    ja: "一緒に演奏するということは、お互いの音を聴くということ。",
  },
  {
    en: "Feelings you can't put into words can be expressed through a melody.",
    zh: "无法用语言表达的心情，可以用旋律来倾诉。",
    ja: "言葉にできない気持ちは、メロディで伝えられると思う。",
  },
  {
    en: "Even when you can't see the path ahead, your feet always remember the way.",
    zh: "即使看不见前方的路，你的双脚总会记得方向。",
    ja: "先が見えなくても、足はいつだって道を覚えているから。",
  },
  {
    en: "I wonder if everyone who loves music is searching for something.",
    zh: "我想，所有热爱音乐的人，都在寻找着什么吧。",
    ja: "音楽を愛する人はみんな、何かを探しているのかもしれない。",
  },
  {
    en: "Being able to look at the same sky together — I think that's a kind of happiness.",
    zh: "能够一起看同一片天空，我觉得那就是一种幸福。",
    ja: "同じ空を一緒に見られること、それが幸せだと思う。",
  },
  {
    en: "Even a cucumber takes time to grow. Good things can't be rushed.",
    zh: "就算是黄瓜，也需要时间才能长大。好事不能着急。",
    ja: "きゅうりだって、育つのには時間がかかる。いいことは急げない。",
  },
  {
    en: "I'm not good at expressing things. But I do think about you.",
    zh: "我不擅长表达。但我确实在想着你。",
    ja: "うまく表現できないけど……ちゃんと、考えてるよ。",
  },
  {
    en: "A song that once fell apart can still be played again. Just… differently.",
    zh: "一首曾经散落的歌，还是可以再奏响的。只是……方式不同了。",
    ja: "バラバラになった曲も、また弾ける。ただ……少し違う形で。",
  },
]

// ── Exported collections ────────────────────────────────────────────

/** All canonical lines from the anime. */
export const MUTSUMI_CANONICAL_QUOTES: readonly QuoteEntry[] = canonicalLines

/** Original quotes written in Mutsumi's voice and style. */
export const MUTSUMI_ORIGINAL_QUOTES: readonly QuoteEntry[] = originalLines

/** Combined pool — canonical first, then original. */
export const MUTSUMI_ALL_QUOTES: readonly QuoteEntry[] =
  [...canonicalLines, ...originalLines]
