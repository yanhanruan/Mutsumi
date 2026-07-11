/**
 * i18n — unit tests.
 *
 * Tests:
 *  1. detectLocale() maps navigator.language values to the correct locale.
 *  2. useI18n() returns reactive translations that update with setLocale().
 *  3. All locale bundles are complete and type-safe (no missing keys).
 *  4. EN fallback for unsupported language tags.
 */

import { describe, it, expect, afterEach } from 'vitest'
import { detectLocale, setLocale, useI18n, type Locale } from './index'
import { en } from './locales/en'
import { zh } from './locales/zh'
import { ja } from './locales/ja'
import { HEXAGRAMS, HEXAGRAM_PALACES, PALACE_POSITIONS } from '../config/iching'

// ── Helpers ────────────────────────────────────────────────────────

/** Reset locale to 'en' between tests so state doesn't leak. */
afterEach(() => setLocale('en'))

// ── detectLocale ───────────────────────────────────────────────────

describe('detectLocale', () => {
  it('returns "en" for English US', () => {
    expect(detectLocale('en-US')).toBe('en')
  })

  it('returns "en" for English GB', () => {
    expect(detectLocale('en-GB')).toBe('en')
  })

  it('returns "en" for bare "en"', () => {
    expect(detectLocale('en')).toBe('en')
  })

  it('returns "zh" for zh-CN', () => {
    expect(detectLocale('zh-CN')).toBe('zh')
  })

  it('returns "zh" for zh-TW', () => {
    expect(detectLocale('zh-TW')).toBe('zh')
  })

  it('returns "zh" for bare "zh"', () => {
    expect(detectLocale('zh')).toBe('zh')
  })

  it('returns "zh" for case-insensitive ZH-CN', () => {
    expect(detectLocale('ZH-CN')).toBe('zh')
  })

  it('returns "ja" for Japanese', () => {
    expect(detectLocale('ja')).toBe('ja')
  })

  it('returns "ja" for ja-JP', () => {
    expect(detectLocale('ja-JP')).toBe('ja')
  })

  it('falls back to "en" for French', () => {
    expect(detectLocale('fr-FR')).toBe('en')
  })

  it('falls back to "en" for Korean', () => {
    expect(detectLocale('ko-KR')).toBe('en')
  })

  it('falls back to "en" for empty string', () => {
    expect(detectLocale('')).toBe('en')
  })
})

// ── useI18n ────────────────────────────────────────────────────────

describe('useI18n', () => {
  it('returns English translations when locale is "en"', () => {
    setLocale('en')
    const { t } = useI18n()
    expect(t.value.save).toBe('Save')
    expect(t.value.pomFocus).toBe('Focus')
  })

  it('returns Chinese translations when locale is "zh"', () => {
    setLocale('zh')
    const { t } = useI18n()
    expect(t.value.save).toBe('保存')
    expect(t.value.pomFocus).toBe('专注')
  })

  it('returns Japanese translations when locale is "ja"', () => {
    setLocale('ja')
    const { t } = useI18n()
    expect(t.value.save).toBe('保存')
    expect(t.value.pomFocus).toBe('集中')
  })

  it('t is reactive — updates when setLocale is called', () => {
    setLocale('en')
    const { t } = useI18n()
    expect(t.value.save).toBe('Save')
    setLocale('zh')
    expect(t.value.save).toBe('保存')
    setLocale('ja')
    expect(t.value.save).toBe('保存')
  })

  it('locale ref reflects current locale', () => {
    setLocale('ja')
    const { locale } = useI18n()
    expect(locale.value).toBe<Locale>('ja')
  })

  it('has all context menu items for each locale', () => {
    const actionKeys = ['pat_head', 'feed', 'sleep', 'fast_learning'] as const
    for (const l of ['en', 'zh', 'ja'] as Locale[]) {
      setLocale(l)
      const { t } = useI18n()
      for (const k of actionKeys) {
        expect(t.value.contextMenuItems[k]).toBeTruthy()
        expect(t.value.contextResponses[k]).toBeTruthy()
      }
      expect(t.value.contextMenuItems.tarot).toBeTruthy()
      expect(t.value.contextMenuItems.iching).toBeTruthy()
      expect(t.value.contextMenuItems.chat).toBeTruthy()
      expect(t.value.contextMenuItems.hide).toBeTruthy()
    }
  })

  it('has a late-night reminder for each locale', () => {
    for (const l of ['en', 'zh', 'ja'] as Locale[]) {
      setLocale(l)
      const { t } = useI18n()
      expect(t.value.lateNightReminder.length).toBeGreaterThan(0)
    }
  })
})

// ── Locale bundle completeness ─────────────────────────────────────

describe('locale bundle completeness', () => {
  // All required keys from the EN bundle must exist in all other bundles.
  const requiredKeys = Object.keys(en) as (keyof typeof en)[]

  it('zh bundle has all keys present in en', () => {
    for (const key of requiredKeys) {
      expect(zh).toHaveProperty(key)
    }
  })

  it('ja bundle has all keys present in en', () => {
    for (const key of requiredKeys) {
      expect(ja).toHaveProperty(key)
    }
  })

  // 10 keys: pat_head, feed, sleep, wake, fast_learning, tarot, iching, chat, sys_state, hide.
  it('en contextMenuItems has all 10 action labels (incl. sys_state + wake)', () => {
    expect(Object.keys(en.contextMenuItems)).toHaveLength(10)
  })

  it('zh contextMenuItems has all 10 action labels (incl. sys_state + wake)', () => {
    expect(Object.keys(zh.contextMenuItems)).toHaveLength(10)
  })

  it('ja contextMenuItems has all 10 action labels (incl. sys_state + wake)', () => {
    expect(Object.keys(ja.contextMenuItems)).toHaveLength(10)
  })

  it('all locales contain complete I Ching UI and hexagram content', () => {
    for (const bundle of [en, zh, ja]) {
      expect(bundle.iching.title).toBeTruthy()
      expect(bundle.iching.rerollTitle).toBeTruthy()
      expect(bundle.iching.history).toBeTruthy()
      expect(bundle.iching.linePositions).toHaveLength(6)
      expect(Object.keys(bundle.iching.hexagrams)).toHaveLength(64)

      for (const hexagram of HEXAGRAMS) {
        const text = bundle.iching.hexagrams[hexagram.id]
        expect(text.name.trim()).toBeTruthy()
        expect(text.subtitle.trim()).toBeTruthy()
        expect(text.judgment.trim()).toBeTruthy()
        expect(text.reflection.trim()).toBeTruthy()
        expect(text.lines).toHaveLength(6)
        for (const line of text.lines) {
          expect(line.trim()).toBeTruthy()
          expect(line).not.toMatch(/TODO|TBD|translation missing/i)
        }
      }
    }
  })

  it('Chinese I Ching content uses complete classical hexagram texts', () => {
    const qian = zh.iching.hexagrams.hexagram01
    const kun = zh.iching.hexagrams.hexagram02

    expect(qian.judgment).toBe('乾：元亨，利贞。')
    expect(qian.name).toBe('乾为天')
    expect(qian.shortName).toBe('乾')
    expect(zh.iching.hexagrams.hexagram02.name).toBe('坤为地')
    expect(zh.iching.hexagrams.hexagram11.name).toBe('地天泰')
    expect(zh.iching.trigramNames.qian).toBe('乾')
    expect(zh.iching.trigramNames.kun).toBe('坤')
    expect(qian.lines[0]).toBe('初九：潜龙，勿用。')
    expect(qian.commentary).toContain('大哉乾元')
    expect(qian.imageText).toBe('天行健，君子以自强不息。')
    expect(qian.lineCommentaries).toHaveLength(6)
    expect(kun.judgment).toContain('利牝马之贞')
    expect(zh.iching.hexagrams.hexagram09.reflection).toContain('整理细节、约束冲动并累积条件')

    for (const hexagram of Object.values(zh.iching.hexagrams)) {
      expect(hexagram.reflection.length).toBeGreaterThan(45)
      expect(hexagram.commentary?.trim()).toBeTruthy()
      expect(hexagram.imageText?.trim()).toBeTruthy()
      expect(hexagram.lineCommentaries).toHaveLength(6)
    }
  })

  it('all locales contain every Eight-Palace name and position label', () => {
    for (const bundle of [en, zh, ja]) {
      expect(Object.keys(bundle.iching.palaceNames)).toHaveLength(8)
      expect(Object.keys(bundle.iching.palacePositions)).toHaveLength(8)
      for (const palace of HEXAGRAM_PALACES) {
        expect(bundle.iching.palaceNames[palace].trim()).toBeTruthy()
      }
      for (const position of PALACE_POSITIONS) {
        expect(bundle.iching.palacePositions[position].trim()).toBeTruthy()
      }
    }
  })

  it('all locales have character size labels', () => {
    for (const bundle of [en, zh, ja]) {
      expect(bundle.characterSize).toBeTruthy()
      expect(bundle.charSizeSmall).toBeTruthy()
      expect(bundle.charSizeMedium).toBeTruthy()
      expect(bundle.charSizeLarge).toBeTruthy()
    }
  })

  it('all locales have showWeather label', () => {
    for (const bundle of [en, zh, ja]) {
      expect(bundle.showWeather).toBeTruthy()
    }
  })

  it('EN petStatus no longer contains "Pet"', () => {
    expect(en.petStatus).not.toContain('Pet')
    expect(en.resetPet).not.toContain('pet')
  })
})
