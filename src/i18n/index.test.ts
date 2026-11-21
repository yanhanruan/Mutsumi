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
import { HEXAGRAMS, HEXAGRAM_PALACES, PALACE_POSITIONS, patternKey } from '../config/iching'
import zhClassics from '@freizl/yijing/zh-CN/64gua.json'

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
    for (const { iching } of [en, zh, ja]) {
      const { title, rerollTitle, history, hexagrams, linePositions } = iching
      expect([title, rerollTitle, history].every(label => label.trim())).toBe(true)
      expect(linePositions).toHaveLength(6)
      expect(Object.keys(hexagrams)).toHaveLength(64)

      for (const { id } of HEXAGRAMS) {
        const { name, subtitle, judgment, reflection, lines } = hexagrams[id]
        expect(lines).toHaveLength(6)
        for (const text of [name, subtitle, judgment, reflection, ...lines]) {
          expect(text.trim(), id).not.toBe('')
          expect(text, id).not.toMatch(/TODO|TBD|translation missing/i)
        }
      }
    }
  })

  it('Chinese I Ching content uses complete classical hexagram texts', () => {
    expect(zh.iching.hexagrams.hexagram01).toMatchObject({ name: '乾为天', judgment: '乾：元亨，利贞。' })

    const linePositions = [/^初[六九]/, /^[六九]二/, /^[六九]三/, /^[六九]四/, /^[六九]五/, /^上[六九]/]
    for (const definition of HEXAGRAMS) {
      const hexagram = zh.iching.hexagrams[definition.id]
      const source = zhClassics[definition.kingWenNumber - 1]
      expect(source.id).toBe(patternKey(definition.lines))
      expect(hexagram).toMatchObject({
        judgment: source.gua_ci, commentary: source.tuan_ci, imageText: source.da_xiang,
        lines: source.yao_ci.slice(0, 6), lineCommentaries: source.xiao_xiang.slice(0, 6),
      })
      hexagram.lines.forEach((line, i) => expect(line).toMatch(linePositions[i]))
    }
  })

  it('all locales contain every Eight-Palace name and position label', () => {
    for (const bundle of [en, zh, ja]) {
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
