<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { useI18n } from '../i18n'
import { useOverlayVisibility } from '../composables/useOverlayVisibility'
import { useIChingJournal } from '../composables/useIChingJournal'
import {
  findHexagramById,
  getCoinSidesForLine,
  getChangedPattern,
  getPrimaryPattern,
  type IChingReading,
  type LineValue,
} from '../config/iching'
import HexagramLines from './HexagramLines.vue'
import cucumberCoinImage from '../assets/iching/cucumber-coin.svg'
import coinFrontImage from '../../src-tauri/icons/128x128@2x.png'

type IChingViewState = 'intro' | 'generating' | 'result' | 'history' | 'history-result'

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()
const {
  getTodayReading,
  getHistory,
  createReading,
  rerollReading,
} = useIChingJournal()
const { visible, skipLeave, show, dismiss: dismissOverlay } = useOverlayVisibility()

const viewState = ref<IChingViewState>('intro')
const currentReading = ref<IChingReading | null>(null)
const generatedLines = ref<LineValue[]>([])
const revealIndex = ref(0)
const detailsOpen = ref(false)
const isAnimating = ref(false)
const isRerollConfirmOpen = ref(false)
const errorMessage = ref<string | null>(null)
const generationKey = ref(0)
const throwRound = ref(0)
const isThrowing = ref(false)
const currentThrowLine = ref<LineValue | null>(null)

let timers: ReturnType<typeof setTimeout>[] = []
let coinAudioContext: AudioContext | null = null

const history = computed(() => getHistory())
const reading = computed(() => currentReading.value)
const primaryDefinition = computed(() => reading.value ? findHexagramById(reading.value.primaryHexagramId) : null)
const changedDefinition = computed(() => reading.value?.changedHexagramId ? findHexagramById(reading.value.changedHexagramId) : null)
const primaryText = computed(() => reading.value ? t.value.iching.hexagrams[reading.value.primaryHexagramId] : null)
const changedText = computed(() => reading.value?.changedHexagramId ? t.value.iching.hexagrams[reading.value.changedHexagramId] : null)
const movingLineTexts = computed(() => {
  if (!reading.value || !primaryText.value) return []
  return reading.value.movingLineIndexes.map(index => ({
    index,
    label: t.value.iching.linePositions[index],
    text: primaryText.value!.lines[index],
    commentary: primaryText.value!.lineCommentaries?.[index],
  }))
})
const progressText = computed(() =>
  t.value.iching.progress.replace('{current}', String(Math.min(revealIndex.value, 6))))
const canShowHistory = computed(() => history.value.length > 0)
const lineIndexes = [0, 1, 2, 3, 4, 5] as const
const coinSides = computed(() => {
  if (currentThrowLine.value === null) return ['front', 'back', 'front'] as const
  return getCoinSidesForLine(currentThrowLine.value)
})

function clearTimers(): void {
  timers.forEach(timer => clearTimeout(timer))
  timers = []
}

function prefersReducedMotion(): boolean {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
}

function playCoinThrowSound(): void {
  try {
    coinAudioContext ??= new AudioContext()
    const context = coinAudioContext
    void context.resume()
    const start = context.currentTime

    for (let coin = 0; coin < 3; coin++) {
      const hitAt = start + 0.56 + coin * 0.052
      const output = context.createGain()
      output.gain.setValueAtTime(0.0001, hitAt)
      output.gain.exponentialRampToValueAtTime(0.075 - coin * 0.009, hitAt + 0.002)
      output.gain.exponentialRampToValueAtTime(0.0001, hitAt + 0.16)
      output.connect(context.destination)

      const noiseLength = Math.floor(context.sampleRate * 0.045)
      const noiseBuffer = context.createBuffer(1, noiseLength, context.sampleRate)
      const noiseData = noiseBuffer.getChannelData(0)
      for (let sample = 0; sample < noiseLength; sample++) {
        noiseData[sample] = (Math.random() * 2 - 1) * (1 - sample / noiseLength)
      }
      const noise = context.createBufferSource()
      const noiseFilter = context.createBiquadFilter()
      noise.buffer = noiseBuffer
      noiseFilter.type = 'bandpass'
      noiseFilter.frequency.value = 1750 + coin * 110
      noiseFilter.Q.value = 1.8
      noise.connect(noiseFilter)
      noiseFilter.connect(output)
      noise.start(hitAt)

      for (const [frequency, volume] of [[720, 0.7], [1280, 0.42], [1960, 0.2]] as const) {
        const oscillator = context.createOscillator()
        const partialGain = context.createGain()
        oscillator.type = frequency === 720 ? 'triangle' : 'sine'
        oscillator.frequency.setValueAtTime(frequency * (1 + coin * 0.018), hitAt)
        oscillator.frequency.exponentialRampToValueAtTime(frequency * 0.88, hitAt + 0.14)
        partialGain.gain.value = volume
        oscillator.connect(partialGain)
        partialGain.connect(output)
        oscillator.start(hitAt)
        oscillator.stop(hitAt + 0.17)
      }
    }
  } catch (error) {
    console.debug('[iching] coin sound unavailable', error)
  }
}

function requestClose(): void {
  emit('close')
}

function openHistory(): void {
  viewState.value = 'history'
  detailsOpen.value = false
}

function viewHistoryReading(item: IChingReading): void {
  currentReading.value = item
  viewState.value = 'history-result'
  detailsOpen.value = false
}

function backFromHistory(): void {
  const today = getTodayReading()
  currentReading.value = today
  viewState.value = today ? 'result' : 'intro'
  detailsOpen.value = false
}

function showResult(readingToShow: IChingReading, state: IChingViewState = 'result'): void {
  currentReading.value = readingToShow
  viewState.value = state
  detailsOpen.value = false
  isAnimating.value = false
  generatedLines.value = []
  revealIndex.value = 0
  isThrowing.value = false
  currentThrowLine.value = null
}

function scheduleReveal(readingToReveal: IChingReading): void {
  const key = ++generationKey.value
  const reducedMotion = prefersReducedMotion()
  const roundMs = reducedMotion ? 160 : 1020
  const flipMs = reducedMotion ? 30 : 340
  const settleMs = reducedMotion ? 55 : 740
  const revealMs = reducedMotion ? 80 : 850
  generatedLines.value = []
  revealIndex.value = 0

  const beginRound = (index: number): void => {
    if (generationKey.value !== key) return
    isThrowing.value = true
    throwRound.value += 1
    playCoinThrowSound()

    timers.push(setTimeout(() => {
      if (generationKey.value !== key) return
      currentThrowLine.value = readingToReveal.lines[index]
    }, flipMs))

    timers.push(setTimeout(() => {
      if (generationKey.value !== key) return
      isThrowing.value = false
    }, settleMs))

    timers.push(setTimeout(() => {
      if (generationKey.value !== key) return
      generatedLines.value = [...generatedLines.value, readingToReveal.lines[index]]
      revealIndex.value = index + 1
    }, revealMs))
  }

  beginRound(0)
  for (let i = 1; i < readingToReveal.lines.length; i++) {
    timers.push(setTimeout(() => beginRound(i), roundMs * i))
  }
  timers.push(setTimeout(() => {
    if (generationKey.value !== key) return
    showResult(readingToReveal)
  }, roundMs * readingToReveal.lines.length + (reducedMotion ? 60 : 180)))
}

function startGeneration(replaceToday: boolean): void {
  if (isAnimating.value) {
    errorMessage.value = t.value.iching.generationBusy
    return
  }
  errorMessage.value = null
  clearTimers()
  isRerollConfirmOpen.value = false
  isAnimating.value = true
  viewState.value = 'generating'
  detailsOpen.value = false
  const previous = currentReading.value

  try {
    const nextReading = replaceToday ? rerollReading() : createReading()
    scheduleReveal(nextReading)
  } catch (error) {
    console.warn('[iching] generation failed', error)
    clearTimers()
    isAnimating.value = false
    errorMessage.value = t.value.iching.generationError
    if (previous) showResult(previous)
    else viewState.value = 'intro'
  }
}

function open(): void {
  show()
  clearTimers()
  errorMessage.value = null
  isRerollConfirmOpen.value = false
  const today = getTodayReading()
  if (today) showResult(today)
  else {
    currentReading.value = null
    viewState.value = 'intro'
    detailsOpen.value = false
    generatedLines.value = []
    revealIndex.value = 0
  }
}

function dismiss(): void {
  clearTimers()
  isAnimating.value = false
  isThrowing.value = false
  isRerollConfirmOpen.value = false
  dismissOverlay()
}

function onKeydown(e: KeyboardEvent): void {
  if (!visible.value) return
  if (e.key !== 'Escape') return
  e.preventDefault()
  if (isRerollConfirmOpen.value) isRerollConfirmOpen.value = false
  else if (viewState.value === 'history' || viewState.value === 'history-result') backFromHistory()
  else requestClose()
}

watch(visible, v => {
  if (v) window.addEventListener('keydown', onKeydown)
  else window.removeEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  clearTimers()
  window.removeEventListener('keydown', onKeydown)
  void coinAudioContext?.close()
  coinAudioContext = null
})

defineExpose({ open, dismiss })
</script>

<template>
  <Transition name="iching-fade" :css="!skipLeave">
    <div v-if="visible" class="iching-overlay pet-ui-overlay">
      <div class="iching-controls">
        <button
          v-if="viewState !== 'intro'"
          class="ctrl"
          :data-tip="t.iching.history"
          :disabled="isAnimating"
          @click.stop="openHistory"
        >☰</button>
        <button class="ctrl" :data-tip="t.iching.close" @click.stop="requestClose">×</button>
      </div>

      <section v-if="viewState === 'intro'" class="panel intro-panel">
        <h1>{{ t.iching.title }}</h1>
        <p>{{ t.iching.intro }}</p>
        <p class="quiet">{{ t.iching.quietPrompt }}</p>
        <button class="primary-action" @click="startGeneration(false)">{{ t.iching.begin }}</button>
        <button v-if="canShowHistory" class="text-action" @click="openHistory">{{ t.iching.history }}</button>
        <p v-if="errorMessage" class="error-line">{{ errorMessage }}</p>
      </section>

      <section v-else-if="viewState === 'generating'" class="panel generating-panel" aria-live="polite">
        <div class="coin-row" :class="{ throwing: isThrowing }" :aria-label="t.iching.coinsAria">
          <span
            v-for="(side, index) in coinSides"
            :key="`${throwRound}-${index}`"
            class="coin"
            :class="`coin-side-${side}`"
          >
            <span class="coin-face" :class="`coin-face-${side}`">
              <img
                :src="side === 'front' ? coinFrontImage : cucumberCoinImage"
                alt=""
                draggable="false"
              >
            </span>
          </span>
        </div>
        <div class="progress">{{ progressText }}</div>
        <div class="building-lines">
          <div
            v-for="index in [5, 4, 3, 2, 1, 0]"
            :key="index"
            class="building-slot"
            :class="{ visible: generatedLines[index] }"
          >
            <span v-if="generatedLines[index]" class="mini-line" :class="{ yin: ![7, 9].includes(generatedLines[index]) }">
              <span /><span />
            </span>
          </div>
        </div>
      </section>

      <section v-else-if="(viewState === 'result' || viewState === 'history-result') && reading && primaryDefinition && primaryText" class="panel result-panel">
        <div class="eyebrow">{{ viewState === 'history-result' ? reading.dateKey : t.iching.todaysReading }}</div>
        <section class="fortune-summary">
          <strong>{{ t.iching.fortune.title }}：{{ t.iching.fortune.levels[reading.fortuneLevel] }}</strong>
          <p>{{ t.iching.fortune.summaries[reading.fortuneLevel] }}</p>
        </section>
        <div class="hexagram-pair" :class="{ single: !changedDefinition }">
          <div class="hexagram-summary">
            <div class="hexagram-label">{{ t.iching.primaryHexagram }}</div>
            <HexagramLines
              :pattern="getPrimaryPattern(reading.lines)"
              :moving-line-indexes="reading.movingLineIndexes"
              :moving-label="t.iching.movingLineAria"
            />
            <h1>{{ primaryText.name }}</h1>
            <p class="hexagram-brief">{{ primaryText.imageText ?? primaryText.reflection }}</p>
          </div>
          <div v-if="changedDefinition && changedText" class="change-arrow" aria-hidden="true">→</div>
          <div v-if="changedDefinition && changedText" class="hexagram-summary">
            <div class="hexagram-label">{{ t.iching.changedHexagram }}</div>
            <HexagramLines
              :pattern="getChangedPattern(reading.lines)"
              :moving-label="t.iching.movingLineAria"
            />
            <h1>{{ changedText.name }}</h1>
            <p class="hexagram-brief">{{ changedText.imageText ?? changedText.reflection }}</p>
          </div>
        </div>
        <div class="actions">
          <button class="secondary-action" @click="detailsOpen = !detailsOpen">
            {{ detailsOpen ? t.iching.collapseDetails : t.iching.details }}
          </button>
          <button v-if="viewState === 'result'" class="secondary-action" @click="isRerollConfirmOpen = true">
            {{ t.iching.generateAnother }}
          </button>
          <button v-else class="secondary-action" @click="backFromHistory">{{ t.iching.back }}</button>
        </div>

        <Transition name="iching-fade">
          <div v-if="detailsOpen" class="details">
            <p class="details-transition">
              <template v-if="reading.movingLineIndexes.length === 0">
                <strong>{{ t.iching.noMovingLines }}</strong><br>
                {{ t.iching.noChangeExplanation }}
              </template>
              <template v-else>
                <strong>{{ t.iching.movingLines }}: {{ movingLineTexts.map(line => line.label).join(', ') }}</strong><br>
                {{ t.iching.transitionExplanation }}
              </template>
            </p>

            <section class="details-hexagram">
              <h3>{{ t.iching.primaryHexagram }} · {{ primaryText.name }}</h3>
              <p class="palace-info">{{ t.iching.palaceNames[primaryDefinition.palace] }} · {{ t.iching.palacePositions[primaryDefinition.palacePosition] }}</p>
              <h2>{{ t.iching.reflection }}</h2>
              <p>{{ primaryText.reflection }}</p>
              <h2>{{ t.iching.judgment }}</h2>
              <p>{{ primaryText.judgment }}</p>
              <template v-if="primaryText.commentary">
                <h2>{{ t.iching.commentary }}</h2>
                <p>{{ primaryText.commentary }}</p>
              </template>
              <template v-if="primaryText.imageText">
                <h2>{{ t.iching.imageText }}</h2>
                <p>{{ primaryText.imageText }}</p>
              </template>
              <h2>{{ t.iching.allLines }}</h2>
              <ul>
                <li
                  v-for="index in lineIndexes"
                  :key="index"
                  :class="{ 'is-moving-line': reading.movingLineIndexes.includes(index) }"
                >
                  <strong>{{ t.iching.linePositions[index] }}<template v-if="reading.movingLineIndexes.includes(index)"> · {{ t.iching.movingLine }}</template></strong>
                  <span>{{ primaryText.lines[index] }}</span>
                  <small v-if="primaryText.lineCommentaries?.[index]">{{ primaryText.lineCommentaries[index] }}</small>
                </li>
              </ul>
            </section>

            <section v-if="changedDefinition && changedText" class="details-hexagram">
              <h3>{{ t.iching.changedHexagram }} · {{ changedText.name }}</h3>
              <p class="palace-info">{{ t.iching.palaceNames[changedDefinition.palace] }} · {{ t.iching.palacePositions[changedDefinition.palacePosition] }}</p>
              <h2>{{ t.iching.reflection }}</h2>
              <p>{{ changedText.reflection }}</p>
              <h2>{{ t.iching.judgment }}</h2>
              <p>{{ changedText.judgment }}</p>
              <template v-if="changedText.commentary">
                <h2>{{ t.iching.commentary }}</h2>
                <p>{{ changedText.commentary }}</p>
              </template>
              <template v-if="changedText.imageText">
                <h2>{{ t.iching.imageText }}</h2>
                <p>{{ changedText.imageText }}</p>
              </template>
              <h2>{{ t.iching.allLines }}</h2>
              <ul>
                <li v-for="index in lineIndexes" :key="index">
                  <strong>{{ t.iching.linePositions[index] }}</strong>
                  <span>{{ changedText.lines[index] }}</span>
                  <small v-if="changedText.lineCommentaries?.[index]">{{ changedText.lineCommentaries[index] }}</small>
                </li>
              </ul>
            </section>
            <p class="disclaimer">{{ t.iching.disclaimer }}</p>
          </div>
        </Transition>

        <p v-if="errorMessage" class="error-line">{{ errorMessage }}</p>
      </section>

      <section v-else-if="viewState === 'history'" class="panel history-panel">
        <div class="history-head">
          <button class="text-action" @click="backFromHistory">{{ t.iching.back }}</button>
          <h1>{{ t.iching.history }}</h1>
        </div>
        <p v-if="history.length === 0" class="empty">{{ t.iching.emptyHistory }}</p>
        <ul v-else class="history-list">
          <li v-for="item in history" :key="item.id">
            <button @click="viewHistoryReading(item)">
              <span>{{ item.dateKey }}</span>
              <strong>{{ t.iching.hexagrams[item.primaryHexagramId].name }}</strong>
              <small>
                {{ item.movingLineIndexes.length ? `${t.iching.movingLines}: ${item.movingLineIndexes.length}` : t.iching.noMovingLines }}
                <template v-if="item.changedHexagramId"> · {{ t.iching.hexagrams[item.changedHexagramId].name }}</template>
              </small>
            </button>
          </li>
        </ul>
      </section>

      <div v-if="isRerollConfirmOpen" class="dialog-backdrop" role="presentation">
        <div class="confirm-dialog" role="dialog" aria-modal="true" :aria-label="t.iching.rerollTitle">
          <h1>{{ t.iching.rerollTitle }}</h1>
          <p>{{ t.iching.rerollDescription }}</p>
          <p class="notice">{{ t.iching.replacementNotice }}</p>
          <div class="dialog-actions">
            <button class="secondary-action" @click="isRerollConfirmOpen = false">{{ t.iching.keepCurrent }}</button>
            <button class="primary-action" @click="startGeneration(true)">{{ t.iching.confirmReroll }}</button>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.iching-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px 12px;
  box-sizing: border-box;
  font-family: system-ui, "Segoe UI", "Noto Sans SC", "Noto Sans JP", sans-serif;
  color: #1f3328;
  user-select: none;
}

.iching-controls {
  position: absolute;
  top: 8px;
  right: 8px;
  display: flex;
  gap: 6px;
  z-index: 3;
}

.ctrl {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 1px solid rgba(119, 153, 119, 0.40);
  background: rgba(245, 250, 245, 0.92);
  color: #2a4a2a;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(40, 70, 40, 0.14);
}

.ctrl:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ctrl[data-tip] {
  position: relative;
}

.ctrl[data-tip]::after {
  content: attr(data-tip);
  position: absolute;
  top: calc(100% + 7px);
  left: 50%;
  transform: translateX(-50%);
  white-space: nowrap;
  padding: 4px 9px;
  border-radius: 8px;
  font-size: 11px;
  font-weight: 600;
  background: rgba(245, 250, 245, 0.97);
  border: 1px solid rgba(148, 185, 148, 0.45);
  opacity: 0;
  visibility: hidden;
}

.ctrl[data-tip]:hover::after {
  opacity: 1;
  visibility: visible;
}

.panel {
  width: min(94%, 348px);
  max-height: 82vh;
  overflow-y: auto;
  box-sizing: border-box;
  border-radius: 14px;
  border: 1px solid rgba(130, 158, 128, 0.42);
  background:
    linear-gradient(180deg, rgba(250, 252, 247, 0.96), rgba(238, 246, 235, 0.94));
  box-shadow: 0 8px 24px rgba(40, 70, 40, 0.18), inset 0 1px 0 rgba(255, 255, 255, 0.78);
  backdrop-filter: blur(20px) saturate(155%);
  -webkit-backdrop-filter: blur(20px) saturate(155%);
  padding: 16px;
  scrollbar-width: none;
}

.panel::-webkit-scrollbar {
  display: none;
}

h1 {
  margin: 0;
  font-size: 18px;
  line-height: 1.25;
  color: #172a20;
}

h2 {
  margin: 12px 0 5px;
  font-size: 11px;
  text-transform: uppercase;
  color: rgba(45, 85, 45, 0.62);
}

p {
  margin: 8px 0 0;
  font-size: 12.5px;
  line-height: 1.55;
}

.quiet,
.notice,
.disclaimer,
.empty {
  color: rgba(31, 51, 40, 0.68);
}

.primary-action,
.secondary-action,
.text-action {
  border: 1px solid rgba(119, 153, 119, 0.38);
  border-radius: 999px;
  padding: 7px 12px;
  font: inherit;
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}

.primary-action {
  margin-top: 14px;
  background: linear-gradient(135deg, #668c68, #456f55);
  color: #fff;
  border-color: transparent;
}

.secondary-action {
  background: rgba(247, 251, 246, 0.86);
  color: #254432;
}

.text-action {
  background: transparent;
  border-color: transparent;
  color: #315f42;
  padding-left: 0;
  padding-right: 0;
}

.primary-action:focus-visible,
.secondary-action:focus-visible,
.text-action:focus-visible,
.ctrl:focus-visible {
  outline: 2px solid rgba(87, 128, 88, 0.7);
  outline-offset: 2px;
}

.hexagram-pair {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 18px minmax(0, 1fr);
  gap: 6px;
  align-items: start;
}
.fortune-summary { margin-bottom: 12px; padding: 10px; border: 1px solid rgba(119,153,119,.25); border-radius: 10px; background: rgba(246,250,244,.78); color: #294533; }
.fortune-summary p { margin: 5px 0; font-size: 11px; line-height: 1.5; }

.hexagram-pair.single {
  grid-template-columns: minmax(0, 1fr);
}

.hexagram-summary {
  min-width: 0;
  display: grid;
  grid-template-rows: auto 100px auto 1fr;
  justify-items: center;
  text-align: center;
}

.hexagram-label {
  margin-bottom: 7px;
  font-size: 10px;
  font-weight: 800;
  color: rgba(45, 85, 45, 0.62);
}

.change-arrow {
  margin-top: 56px;
  font-size: 17px;
  color: rgba(70, 103, 76, 0.58);
}

.eyebrow,
.progress {
  margin-bottom: 10px;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: rgba(45, 85, 45, 0.58);
}

.hexagram-summary h1 {
  max-width: 100%;
  margin-top: 8px;
  font-size: 14px;
  overflow-wrap: anywhere;
}

.hexagram-brief {
  max-width: 150px;
  margin-top: 5px;
  font-size: 10.5px;
  line-height: 1.45;
  color: rgba(31, 51, 40, 0.72);
}

.actions,
.dialog-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}

.dialog-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: stretch;
}

.dialog-actions .primary-action,
.dialog-actions .secondary-action {
  width: 100%;
  min-height: 38px;
  margin-top: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 7px 8px;
  text-align: center;
}

.details {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid rgba(119, 153, 119, 0.22);
}

.details-transition {
  margin-top: 0;
  padding-bottom: 10px;
  color: rgba(31, 51, 40, 0.72);
}

.details-hexagram + .details-hexagram {
  margin-top: 16px;
  padding-top: 14px;
  border-top: 1px solid rgba(119, 153, 119, 0.28);
}

.details h3 {
  margin: 0 0 8px;
  font-size: 14px;
  color: #203a2a;
}

.palace-info {
  margin-top: -3px;
  font-size: 11px;
  font-weight: 700;
  color: rgba(75, 101, 79, 0.72);
}

.details ul,
.history-list {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
}

.details li {
  display: grid;
  gap: 3px;
  padding: 6px 0;
  border-bottom: 1px solid rgba(119, 153, 119, 0.16);
  font-size: 12px;
}

.details li small {
  color: rgba(31, 51, 40, 0.66);
  font-size: 11px;
  line-height: 1.5;
}

.details li.is-moving-line {
  padding-left: 7px;
  border-left: 2px solid #8a6d37;
}

.history-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.history-list button {
  width: 100%;
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 3px 8px;
  align-items: center;
  padding: 8px 4px;
  border: 0;
  border-bottom: 1px solid rgba(119, 153, 119, 0.16);
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.history-list strong {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.history-list small {
  grid-column: 2;
  color: rgba(31, 51, 40, 0.62);
}

.generating-panel {
  display: flex;
  flex-direction: column;
  align-items: center;
  min-height: 300px;
}

.coin-row {
  display: flex;
  gap: 13px;
  margin: 8px 0 16px;
  perspective: 260px;
}

.coin {
  position: relative;
  width: 42px;
  height: 42px;
  border-radius: 50%;
  transform: translateY(0) rotate(0deg);
  filter: drop-shadow(0 4px 7px rgba(70, 54, 26, 0.2));
}

.coin-face {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  overflow: hidden;
  border-radius: 50%;
  border: 1px solid rgba(83, 53, 22, 0.72);
  background:
    radial-gradient(circle at 33% 24%, rgba(255, 246, 193, 0.95), rgba(214, 160, 71, 0.94) 48%, rgba(132, 83, 28, 0.96) 100%);
  box-shadow:
    inset 0 2px 1px rgba(255, 249, 213, 0.72),
    inset 0 -3px 6px rgba(89, 54, 18, 0.34);
}

.coin-face::before {
  content: '';
  position: absolute;
  inset: 4px;
  border-radius: 50%;
  border: 2px solid rgba(255, 237, 157, 0.82);
  box-shadow: inset 0 0 0 1px rgba(104, 75, 31, 0.28);
}

.coin-face-front img {
  width: 35px;
  height: 35px;
  border-radius: 50%;
  object-fit: cover;
  opacity: 0.92;
}

.coin-face-back img {
  width: 39px;
  height: 39px;
  object-fit: contain;
}

.building-lines {
  width: 126px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.building-slot {
  height: 8px;
  opacity: 0.18;
  border-radius: 999px;
  background: rgba(35, 55, 44, 0.18);
}

.building-slot.visible {
  opacity: 1;
  background: transparent;
}

.mini-line {
  display: flex;
  gap: 12px;
  height: 8px;
}

.mini-line span {
  flex: 1;
  border-radius: 999px;
  background: #23372c;
}

.mini-line:not(.yin) span:first-child {
  flex-basis: 100%;
}

.mini-line:not(.yin) span:last-child {
  display: none;
}

.dialog-backdrop {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 14px;
  background: rgba(25, 40, 30, 0.16);
  z-index: 4;
}

.confirm-dialog {
  width: min(92%, 310px);
  padding: 15px;
  border-radius: 14px;
  border: 1px solid rgba(130, 158, 128, 0.48);
  background: rgba(249, 252, 247, 0.98);
  box-shadow: 0 10px 26px rgba(30, 50, 35, 0.22);
}

.error-line {
  color: #6f3d3d;
  font-weight: 650;
}

.iching-fade-enter-active,
.iching-fade-leave-active {
  transition: opacity 220ms ease;
}

.iching-fade-enter-from,
.iching-fade-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: no-preference) {
  .coin-row.throwing .coin {
    animation: coin-throw 680ms cubic-bezier(0.22, 0.74, 0.24, 1) both;
  }

  @keyframes coin-throw {
    from { transform: translateY(0) rotate(0deg) scaleX(1); }
    28% { transform: translateY(-24px) rotate(92deg) scaleX(0.42); }
    49% { transform: translateY(-31px) rotate(176deg) scaleX(0.04); }
    51% { transform: translateY(-31px) rotate(184deg) scaleX(0.04); }
    74% { transform: translateY(-13px) rotate(270deg) scaleX(0.38); }
    90% { transform: translateY(2px) rotate(344deg) scaleX(0.9); }
    to { transform: translateY(0) rotate(360deg) scaleX(1); }
  }
}
</style>
