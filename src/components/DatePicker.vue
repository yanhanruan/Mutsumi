<script setup lang="ts">
/**
 * DatePicker — a compact, themed single-date picker.
 *
 * Replaces the native `<input type="date">` (whose calendar popup is OS-drawn and
 * can't be themed). It is tuned to the chat UI's own controls — the trigger
 * matches ChatHistory's `.hist-search` input and the popover matches the
 * `EmojiPicker` glass, so it sits seamlessly beside them.
 *
 * v-model is a `'YYYY-MM-DD'` string (or `''` for unset) — the same shape the chat
 * history filters already feed to `dayStart` / `dayEnd`, so the backend contract is
 * unchanged. Month / weekday / selected-date text come from `Intl` against the
 * active locale; only the Today / Clear / placeholder chrome uses i18n keys.
 */
import { ref, computed, watch, nextTick, onUnmounted } from 'vue'
import { useI18n } from '../i18n'

const props = withDefaults(
  defineProps<{
    modelValue: string
    placeholder?: string
    align?: 'left' | 'right'
  }>(),
  { placeholder: '', align: 'left' },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const { t, locale } = useI18n()

const localeTag = computed(
  () => ({ en: 'en-US', zh: 'zh-CN', ja: 'ja-JP' }[locale.value] ?? 'en-US'),
)

// 0 = Sunday. Flip to 1 for a Monday-first grid.
const WEEK_START = 0

// ── Date helpers ───────────────────────────────────────────────────────────
const pad = (n: number) => String(n).padStart(2, '0')
const isoOf = (y: number, m: number, d: number) => `${y}-${pad(m + 1)}-${pad(d)}`

/** Parse a 'YYYY-MM-DD' string into a local Date (or null). */
function parseIso(s: string): Date | null {
  if (!s) return null
  const d = new Date(`${s}T00:00:00`)
  return Number.isNaN(d.getTime()) ? null : d
}

const today = new Date()
const todayIso = isoOf(today.getFullYear(), today.getMonth(), today.getDate())

// ── Open / close ───────────────────────────────────────────────────────────
const root = ref<HTMLElement | null>(null)
const open = ref(false)

// Capture phase + a `contains` check so clicking *another* picker's trigger
// closes this one even if that handler stops propagation — only one picker
// is ever open at a time.
function onOutside(e: Event) {
  if (!root.value?.contains(e.target as Node)) open.value = false
}
watch(open, isOpen => {
  if (isOpen) {
    // Sync the view to the selected month (or today) each time it opens.
    const sel = parseIso(props.modelValue) ?? today
    viewYear.value = sel.getFullYear()
    viewMonth.value = sel.getMonth()
    nextTick(() => document.addEventListener('click', onOutside, true))
  } else {
    document.removeEventListener('click', onOutside, true)
  }
})
onUnmounted(() => document.removeEventListener('click', onOutside, true))

// ── Displayed month ────────────────────────────────────────────────────────
const viewYear = ref(today.getFullYear())
const viewMonth = ref(today.getMonth())

function prevMonth() {
  if (viewMonth.value === 0) { viewMonth.value = 11; viewYear.value-- }
  else viewMonth.value--
}
function nextMonth() {
  if (viewMonth.value === 11) { viewMonth.value = 0; viewYear.value++ }
  else viewMonth.value++
}

// ── Localized labels (Intl — no hardcoded strings) ─────────────────────────
const monthLabel = computed(() =>
  new Date(viewYear.value, viewMonth.value, 1).toLocaleDateString(localeTag.value, {
    year: 'numeric',
    month: 'long',
  }),
)

/** Short weekday names in the configured week order. */
const weekdays = computed(() => {
  const fmt = new Intl.DateTimeFormat(localeTag.value, { weekday: 'short' })
  // 2023-01-01 was a Sunday — a stable reference week.
  return Array.from({ length: 7 }, (_, i) =>
    fmt.format(new Date(2023, 0, 1 + ((WEEK_START + i) % 7))),
  )
})

const triggerLabel = computed(() => {
  const d = parseIso(props.modelValue)
  if (!d) return props.placeholder || t.value.chat.datePlaceholder
  return d.toLocaleDateString(localeTag.value, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  })
})

// ── Day grid (6 weeks × 7 days) ────────────────────────────────────────────
interface Cell {
  iso: string
  day: number
  inMonth: boolean
  isToday: boolean
  isSelected: boolean
}

const weeks = computed<Cell[][]>(() => {
  const y = viewYear.value
  const m = viewMonth.value
  const first = new Date(y, m, 1)
  // How many leading days from the previous month fill the first row.
  const lead = (first.getDay() - WEEK_START + 7) % 7
  const start = new Date(y, m, 1 - lead)

  const rows: Cell[][] = []
  const cursor = new Date(start)
  for (let w = 0; w < 6; w++) {
    const row: Cell[] = []
    for (let d = 0; d < 7; d++) {
      const cy = cursor.getFullYear()
      const cm = cursor.getMonth()
      const cd = cursor.getDate()
      const iso = isoOf(cy, cm, cd)
      row.push({
        iso,
        day: cd,
        inMonth: cm === m,
        isToday: iso === todayIso,
        isSelected: iso === props.modelValue,
      })
      cursor.setDate(cd + 1)
    }
    rows.push(row)
  }
  return rows
})

// ── Actions ────────────────────────────────────────────────────────────────
function pick(iso: string) {
  emit('update:modelValue', iso)
  open.value = false
}
function goToday() {
  // Jump the view to today's month and select it.
  viewYear.value = today.getFullYear()
  viewMonth.value = today.getMonth()
  pick(todayIso)
}
function clear() {
  emit('update:modelValue', '')
  open.value = false
}
</script>

<template>
  <div ref="root" class="dp">
    <button
      type="button"
      class="dp-trigger"
      :class="{ 'is-empty': !modelValue, 'is-open': open }"
      @click="open = !open"
    >
      <svg class="dp-cal" width="13" height="13" viewBox="0 0 24 24" fill="none">
        <rect x="3" y="4.5" width="18" height="16" rx="2.5" stroke="currentColor" stroke-width="2"/>
        <path d="M3 9h18M8 2.5v4M16 2.5v4" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
      </svg>
      <span class="dp-label">{{ triggerLabel }}</span>
    </button>

    <Transition name="dp">
      <div v-if="open" class="dp-menu" :class="`align-${align}`">
        <div class="dp-head">
          <button type="button" class="dp-nav" @click="prevMonth">
            <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
              <path d="M6.5 1L2.5 5l4 4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
          <span class="dp-month">{{ monthLabel }}</span>
          <button type="button" class="dp-nav" @click="nextMonth">
            <svg width="9" height="9" viewBox="0 0 10 10" fill="none">
              <path d="M3.5 1l4 4-4 4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
          </button>
        </div>

        <div class="dp-week">
          <span v-for="(w, i) in weekdays" :key="i" class="dp-wd">{{ w }}</span>
        </div>

        <div class="dp-grid">
          <template v-for="(row, wi) in weeks" :key="wi">
            <button
              v-for="cell in row"
              :key="cell.iso"
              type="button"
              class="dp-day"
              :class="{
                'is-out': !cell.inMonth,
                'is-today': cell.isToday,
                'is-selected': cell.isSelected,
              }"
              @click="pick(cell.iso)"
            >
              {{ cell.day }}
            </button>
          </template>
        </div>

        <div class="dp-foot">
          <button type="button" class="dp-action" @click="clear">{{ t.chat.dateClear }}</button>
          <button type="button" class="dp-action dp-action--accent" @click="goToday">{{ t.chat.dateToday }}</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.dp {
  position: relative;
  flex: 1;
  min-width: 0;
}

/* ── Trigger (matches ChatHistory's .hist-search input) ───────────── */
.dp-trigger {
  width: 100%;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 6px 9px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  border-radius: 9px;
  background: rgba(255, 255, 255, 0.9);
  color: #1a2e1a;
  font-size: 12px;
  font-family: inherit;
  cursor: pointer;
  text-align: left;
  transition: border-color 150ms ease, box-shadow 150ms ease, transform 80ms ease;
}
.dp-trigger:hover,
.dp-trigger.is-open,
.dp-trigger:focus-visible {
  outline: none;
  border-color: rgba(119, 153, 119, 0.85);
  box-shadow: 0 0 0 2px rgba(119, 153, 119, 0.18);
}
.dp-trigger:active { transform: scale(0.98); }
.dp-trigger.is-empty .dp-label { color: rgba(40, 70, 40, 0.45); }
.dp-cal { flex-shrink: 0; color: rgba(40, 80, 40, 0.5); }
.dp-label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

/* ── Popover menu (matches EmojiPicker glass; opens downward) ──────── */
.dp-menu {
  position: absolute;
  top: calc(100% + 5px);
  z-index: 10;
  width: 236px;
  box-sizing: border-box;
  padding: 8px;
  border: 1px solid rgba(148, 185, 148, 0.5);
  border-radius: 12px;
  background: rgba(244, 250, 244, 0.98);
  backdrop-filter: blur(20px) saturate(160%);
  -webkit-backdrop-filter: blur(20px) saturate(160%);
  box-shadow: 0 6px 22px rgba(60, 90, 60, 0.22);
}
.dp-menu.align-left  { left: 0; }
.dp-menu.align-right { right: 0; }

/* Open / close animation (matches the chat's emoji-pop) */
.dp-enter-active, .dp-leave-active { transition: opacity 140ms ease, transform 140ms ease; }
.dp-enter-from, .dp-leave-to { opacity: 0; transform: translateY(-6px) scale(0.98); }

/* ── Month header ─────────────────────────────────────────────────── */
.dp-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}
.dp-month {
  font-size: 12.5px;
  font-weight: 700;
  color: rgba(30, 52, 30, 0.9);
}
.dp-nav {
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: rgba(40, 70, 40, 0.55);
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, transform 80ms ease;
}
.dp-nav:hover  { background: rgba(119, 153, 119, 0.16); color: #1a2e1a; }
.dp-nav:active { transform: scale(0.9); }

/* ── Weekday row ──────────────────────────────────────────────────── */
.dp-week {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  margin-bottom: 3px;
}
.dp-wd {
  text-align: center;
  font-size: 10px;
  font-weight: 600;
  color: rgba(40, 70, 40, 0.45);
  padding: 2px 0;
}

/* ── Day grid ─────────────────────────────────────────────────────── */
.dp-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 2px;
}
.dp-day {
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: #1a2e1a;
  font-size: 12px;
  font-weight: 500;
  font-family: inherit;
  cursor: pointer;
  transition: background 110ms ease, color 110ms ease, transform 90ms ease;
}
.dp-day:hover  { background: rgba(119, 153, 119, 0.16); }
.dp-day:active { transform: scale(0.88); }
.dp-day.is-out { color: rgba(40, 70, 40, 0.3); }
.dp-day.is-today { box-shadow: inset 0 0 0 1.5px rgba(119, 153, 119, 0.5); }
.dp-day.is-selected {
  background: linear-gradient(135deg, #779977, #5a8060);
  color: #f3f8f3;
  font-weight: 600;
  box-shadow: none;
}

/* ── Footer actions ───────────────────────────────────────────────── */
.dp-foot {
  display: flex;
  justify-content: space-between;
  margin-top: 7px;
  padding-top: 7px;
  border-top: 1px solid rgba(148, 185, 148, 0.4);
}
.dp-action {
  padding: 3px 9px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: rgba(40, 70, 40, 0.6);
  font-size: 11px;
  font-weight: 600;
  font-family: inherit;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.dp-action:hover { background: rgba(119, 153, 119, 0.12); color: #1a2e1a; }
.dp-action--accent { color: #5a8060; }
.dp-action--accent:hover { background: rgba(119, 153, 119, 0.16); color: #466646; }
</style>
