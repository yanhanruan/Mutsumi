<script setup lang="ts">
/**
 * EmojiPicker — a compact popover for searching and picking emojis.
 *
 * Rendered by ChatPanel just above the composer's emoji button (the parent owns
 * the open/close state and positions this absolutely). Browses by category when
 * the search box is empty; filters across en/zh/ja keywords (or a direct
 * character match) otherwise, and the hover tooltip shows the active locale's
 * keywords. Emits `select` with the chosen character; the parent inserts it at
 * the textarea caret and closes the popover.
 */
import { ref, computed, nextTick, onMounted } from 'vue'
import { useI18n } from '../i18n'
import { EMOJI_GROUPS, type Emoji, type EmojiGroup } from '../config/emoji'
import { vTip } from '../composables/useTooltip'

const emit = defineEmits<{ select: [char: string]; close: [] }>()
const { t, locale } = useI18n()

const query = ref('')
const searchRef = ref<HTMLInputElement | null>(null)

/** Active-locale keyword string for the hover tooltip (falls back to English). */
function localizedKeywords(e: Emoji): string {
  const lk = locale.value as keyof Emoji['keywords']
  return e.keywords[lk] ?? e.keywords.en
}

/** When searching, a single flattened+filtered group; otherwise the categories. */
const sections = computed<EmojiGroup[]>(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return EMOJI_GROUPS
  // Match across all languages so any of en/zh/ja works regardless of UI locale.
  const hits = EMOJI_GROUPS.flatMap(g => g.emojis).filter(
    e =>
      e.char === q ||
      e.keywords.en.includes(q) ||
      e.keywords.zh.includes(q) ||
      e.keywords.ja.includes(q),
  )
  return hits.length ? [{ icon: '🔎', emojis: hits }] : []
})

onMounted(() => nextTick(() => searchRef.value?.focus()))
</script>

<template>
  <div class="emoji-picker">
    <input
      ref="searchRef"
      v-model="query"
      class="emoji-search"
      type="search"
      :placeholder="t.chat.emojiSearch"
      maxlength="40"
      @keydown.esc.stop="emit('close')"
    />
    <div class="emoji-scroll">
      <p v-if="!sections.length" class="emoji-empty">{{ t.chat.emojiNoResults }}</p>
      <div v-for="(g, gi) in sections" :key="gi" class="emoji-section">
        <div class="emoji-grid">
          <button
            v-for="e in g.emojis"
            :key="e.char"
            class="emoji-cell"
            v-tip="localizedKeywords(e)"
            @click="emit('select', e.char)"
          >
            {{ e.char }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.emoji-picker {
  width: 244px;
  max-width: 78vw;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  box-sizing: border-box;
  border-radius: 12px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  background: rgba(244, 250, 244, 0.98);
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 6px 22px rgba(60, 90, 60, 0.22);
}

.emoji-search {
  width: 100%;
  box-sizing: border-box;
  padding: 6px 9px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.92);
  color: #1a2e1a;
  font-size: 12px;
  outline: none;
  transition: border-color 150ms ease, box-shadow 150ms ease;
}
.emoji-search:focus {
  border-color: rgba(119, 153, 119, 0.85);
  box-shadow: 0 0 0 2px rgba(119, 153, 119, 0.18);
}

.emoji-scroll {
  max-height: 200px;
  overflow-y: auto;
  padding-right: 2px;
}
.emoji-scroll::-webkit-scrollbar { width: 5px; }
.emoji-scroll::-webkit-scrollbar-thumb {
  background: rgba(119, 153, 119, 0.4);
  border-radius: 3px;
}
.emoji-empty {
  margin: 16px 0;
  text-align: center;
  font-size: 12px;
  color: rgba(40, 70, 40, 0.45);
}

.emoji-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 2px;
}
.emoji-cell {
  border: none;
  background: transparent;
  cursor: pointer;
  font-size: 19px;
  line-height: 1;
  padding: 5px 0;
  border-radius: 7px;
  transition: background 110ms ease, transform 110ms ease;
}
.emoji-cell:hover { background: rgba(119, 153, 119, 0.16); }
.emoji-cell:active { transform: scale(0.88); }
</style>
