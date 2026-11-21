<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from '../i18n'
import { useOverlayVisibility } from '../composables/useOverlayVisibility'
import { useIChingJournal, type IChingReading } from '../composables/useIChingJournal'
import { getCoinSidesForLine } from '../config/iching'
import HexagramLines from './HexagramLines.vue'
import coinBack from '../assets/iching/cucumber-coin.svg'
import coinFront from '../../app-icon-cucumber-puppy-transparent-compress.png'

const emit = defineEmits<{ close: [] }>()
const { t } = useI18n()
const { history, getTodayReading, castToday } = useIChingJournal()
const { visible, skipLeave, show, dismiss: dismissOverlay } = useOverlayVisibility()

const today = ref<IChingReading | null>(null)
const showingHistory = ref(false)
const selected = ref<IChingReading | null>(null)
const detailsOpen = ref(false)
const confirming = ref(false)
const failed = ref(false)
const revealed = ref<number | null>(null)
let revealTimer: ReturnType<typeof setTimeout> | undefined
let closeRequested = false

const reading = computed(() => showingHistory.value ? selected.value : today.value)
const cards = computed(() => {
  const item = reading.value
  if (!item) return []
  const copy = t.value.iching
  const primary = { hexagram: item.primary, label: copy.primaryHexagram, moving: item.movingLineIndexes }
  const changed = item.changed && { hexagram: item.changed, label: copy.changedHexagram, moving: [] }
  return (changed ? [primary, changed] : [primary]).map(card => ({
    ...card,
    text: copy.hexagrams[card.hexagram.id],
  }))
})
const round = computed(() => Math.min(revealed.value ?? 0, 5))
const coins = computed(() => today.value ? getCoinSidesForLine(today.value.lines[round.value]) : [])
const progress = computed(() => t.value.iching.progress.replace('{current}', String(revealed.value ?? 0)))

function stopReveal() {
  clearTimeout(revealTimer)
  revealed.value = null
}

function requestClose() {
  if (!visible.value || closeRequested) return
  closeRequested = true
  clearTimeout(revealTimer)
  emit('close')
}

function startReading(replace: boolean) {
  if (!visible.value || closeRequested || revealed.value !== null) return
  if (replace && !confirming.value) return
  confirming.value = false
  detailsOpen.value = false
  failed.value = false

  try {
    today.value = castToday(replace)
  } catch (error) {
    console.warn('[iching] could not save reading', error)
    failed.value = true
    return
  }

  const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
  const interval = reduced ? 120 : 850
  revealed.value = 0

  function revealNext() {
    if (revealed.value === null) return
    if (revealed.value === 6) {
      stopReveal()
      return
    }
    revealed.value++
    revealTimer = setTimeout(revealNext, revealed.value === 6 ? (reduced ? 120 : 350) : interval)
  }
  revealTimer = setTimeout(revealNext, interval)
}

function openHistory() {
  showingHistory.value = true
  selected.value = null
  detailsOpen.value = false
  confirming.value = false
}

function back() {
  detailsOpen.value = false
  if (selected.value) selected.value = null
  else showingHistory.value = false
}

function open() {
  stopReveal()
  closeRequested = false
  today.value = getTodayReading()
  showingHistory.value = false
  selected.value = null
  detailsOpen.value = false
  confirming.value = false
  failed.value = false
  show()
}

function dismiss() {
  stopReveal()
  dismissOverlay()
}

function onKeydown(event: KeyboardEvent) {
  if (!visible.value || closeRequested || event.key !== 'Escape') return
  event.preventDefault()
  if (confirming.value) confirming.value = false
  else if (showingHistory.value) back()
  else requestClose()
}

watch(visible, active => {
  if (active) window.addEventListener('keydown', onKeydown)
  else window.removeEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  clearTimeout(revealTimer)
  window.removeEventListener('keydown', onKeydown)
})

defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="iching-fade" :css="!skipLeave">
    <div v-if="visible" class="iching-overlay pet-ui-overlay">
      <div class="controls">
        <button class="ctrl" :aria-label="t.iching.history" :title="t.iching.history"
          :disabled="revealed !== null || (showingHistory && !selected)" @click="openHistory">☰</button>
        <button class="ctrl" :aria-label="t.iching.close" :title="t.iching.close" @click="requestClose">×</button>
      </div>

      <section v-if="revealed !== null && today" class="panel centered" aria-live="polite">
        <div :key="round" class="coins" :aria-label="t.iching.coinsAria">
          <img v-for="(side, index) in coins" :key="index" :src="side === 'front' ? coinFront : coinBack"
            alt="" draggable="false">
        </div>
        <p>{{ progress }}</p>
        <HexagramLines :pattern="today.primary.lines" :revealed="revealed" />
      </section>

      <section v-else-if="showingHistory && !selected" class="panel">
        <button class="action" @click="back">{{ t.iching.back }}</button>
        <h1>{{ t.iching.history }}</h1>
        <p v-if="!history.length">{{ t.iching.emptyHistory }}</p>
        <ul v-else class="history">
          <li v-for="item in history" :key="item.dateKey">
            <button class="action history-entry" @click="selected = item">
              <time :datetime="item.dateKey">{{ item.dateKey }}</time>
              <strong>{{ t.iching.hexagrams[item.primary.id].name }}</strong>
              <span v-if="item.changed"> → {{ t.iching.hexagrams[item.changed.id].name }}</span>
            </button>
          </li>
        </ul>
      </section>

      <section v-else-if="reading" class="panel">
        <h1>{{ showingHistory ? reading.dateKey : t.iching.todaysReading }}</h1>
        <div class="fortune">
          <strong>{{ t.iching.fortune.title }} · {{ t.iching.fortune.levels[reading.fortuneLevel] }}</strong>
          <p>{{ t.iching.fortune.summaries[reading.fortuneLevel] }}</p>
        </div>

        <div class="hexagrams">
          <article v-for="card in cards" :key="card.hexagram.id" class="hexagram-card">
            <small>{{ card.label }}</small>
            <h2>{{ card.text.name }}</h2>
            <HexagramLines :pattern="card.hexagram.lines" :moving-line-indexes="card.moving"
              :moving-label="t.iching.movingLineAria" />
            <p>{{ card.text.imageText ?? card.text.reflection }}</p>
          </article>
        </div>

        <div class="actions">
          <button class="action" :aria-expanded="detailsOpen" @click="detailsOpen = !detailsOpen">
            {{ detailsOpen ? t.iching.collapseDetails : t.iching.details }}
          </button>
          <button v-if="showingHistory" class="action" @click="back">{{ t.iching.back }}</button>
          <button v-else-if="!confirming" class="action" @click="confirming = true">{{ t.iching.generateAnother }}</button>
        </div>
        <section v-if="confirming" class="fortune" role="group" :aria-label="t.iching.rerollTitle">
          <strong>{{ t.iching.rerollTitle }}</strong>
          <p>{{ t.iching.rerollDescription }}</p>
          <div class="actions">
            <button class="action" @click="confirming = false">{{ t.iching.keepCurrent }}</button>
            <button class="action primary" @click="startReading(true)">{{ t.iching.confirmReroll }}</button>
          </div>
        </section>

        <div v-if="detailsOpen" class="details">
          <p v-if="reading.movingLineIndexes.length">
            <strong>{{ t.iching.movingLines }} · {{ reading.movingLineIndexes.map(i => t.iching.linePositions[i]).join(' · ') }}</strong>
            <br>{{ t.iching.transitionExplanation }}
          </p>
          <p v-else><strong>{{ t.iching.noMovingLines }}</strong><br>{{ t.iching.noChangeExplanation }}</p>

          <section v-for="card in cards" :key="card.hexagram.id">
            <h2>{{ card.label }} · {{ card.text.name }}</h2>
            <small>{{ t.iching.palaceNames[card.hexagram.palace] }} · {{ t.iching.palacePositions[card.hexagram.palacePosition] }}</small>
            <h3>{{ t.iching.reflection }}</h3>
            <p>{{ card.text.reflection }}</p>
            <h3>{{ t.iching.judgment }}</h3>
            <p>{{ card.text.judgment }}</p>
            <template v-if="card.text.commentary">
              <h3>{{ t.iching.commentary }}</h3>
              <p>{{ card.text.commentary }}</p>
            </template>
            <template v-if="card.text.imageText">
              <h3>{{ t.iching.imageText }}</h3>
              <p>{{ card.text.imageText }}</p>
            </template>
            <h3>{{ t.iching.allLines }}</h3>
            <ol class="line-details">
              <li v-for="(line, index) in card.text.lines" :key="index" :class="{ moving: card.moving.includes(index) }">
                <strong>{{ t.iching.linePositions[index] }}<template v-if="card.moving.includes(index)"> · {{ t.iching.movingLine }}</template></strong>
                <p>{{ line }}</p>
                <small v-if="card.text.lineCommentaries?.[index]">{{ card.text.lineCommentaries[index] }}</small>
              </li>
            </ol>
          </section>
          <p>{{ t.iching.disclaimer }}</p>
        </div>
        <p v-if="failed" class="error" role="alert">{{ t.iching.generationError }}</p>
      </section>

      <section v-else class="panel centered">
        <h1>{{ t.iching.title }}</h1>
        <p>{{ t.iching.intro }}</p>
        <p>{{ t.iching.quietPrompt }}</p>
        <button class="action primary" @click="startReading(false)">{{ t.iching.begin }}</button>
        <p v-if="failed" class="error" role="alert">{{ t.iching.generationError }}</p>
      </section>
    </div>
  </Transition>
</template>

<style scoped>
.iching-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  box-sizing: border-box;
  color: #263b2f;
  font: 13px/1.5 system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
}

.controls, .actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.controls {
  justify-content: flex-end;
}

.ctrl, .action {
  color: inherit;
  font: inherit;
  cursor: pointer;
  background: rgba(245, 250, 245, 0.95);
  border: 1px solid rgba(119, 153, 119, 0.35);
}

.ctrl {
  width: 28px;
  height: 28px;
  border-radius: 50%;
}

.ctrl:disabled {
  opacity: 0.45;
  cursor: default;
}

.action {
  padding: 5px 10px;
  border-radius: 10px;
}

.primary {
  background: #41674d;
  color: #fff;
}

.ctrl:not(:disabled):hover, .action:not(.primary):hover { background: #fff; }
.primary:hover { background: #34553e; }

.panel {
  flex: 1;
  min-height: 0;
  padding: 12px;
  overflow: auto;
  border-radius: 16px;
  background: rgba(245, 250, 245, 0.96);
  border: 1px solid rgba(119, 153, 119, 0.35);
  scrollbar-width: thin;
}

.centered {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
}

h1 { margin: 4px 0 8px; font-size: 18px; }
h2 { margin: 5px 0; font-size: 15px; }
h3 { margin: 8px 0 2px; font-size: 13px; }
p { margin: 4px 0 8px; }
small, time { color: #465747; }
.error { color: #812e2d; }

.fortune, .hexagram-card, .line-details li {
  padding: 8px;
  border-radius: 10px;
  background: rgba(119, 153, 119, 0.12);
}

.fortune { margin: 8px 0; }

.hexagrams {
  display: flex;
  gap: 6px;
  margin: 8px 0;
}

.hexagram-card {
  flex: 1;
  min-width: 0;
  text-align: center;
  display: flex;
  align-items: center;
  flex-direction: column;
  overflow-wrap: anywhere;
}

.hexagram-card p { margin-top: 8px; }
.details section + section { margin-top: 12px; }
.details { margin-top: 10px; }

.history, .line-details {
  list-style: none;
  padding: 0;
  display: grid;
  gap: 6px;
}

.history-entry {
  width: 100%;
  text-align: left;
}

.history-entry time {
  display: block;
}

.line-details .moving { background: rgba(186, 151, 88, 0.22); }

.coins {
  display: flex;
  gap: 8px;
  margin-bottom: 6px;
}

.coins img {
  width: 40px;
  height: 40px;
  padding: 4px;
  border-radius: 50%;
  border: 1px solid #9a783c;
  background: radial-gradient(circle, #fff2bf, #c29355);
}

.iching-fade-enter-active, .iching-fade-leave-active { transition: opacity 160ms ease; }
.iching-fade-enter-from, .iching-fade-leave-to { opacity: 0; }

@media (prefers-reduced-motion: no-preference) {
  .coins img { animation: toss 560ms ease-in-out both; }
  @keyframes toss {
    0% { transform: rotateY(180deg); }
    45% { transform: translateY(-20px) rotateY(420deg); }
    100% { transform: rotateY(720deg); }
  }
}
</style>
