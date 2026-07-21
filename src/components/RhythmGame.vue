<script setup lang="ts">
/**
 * RhythmGame — QQ炫舞-style canvas rhythm game overlay.
 *
 * Architecture:
 *   - useRhythmGame (pure logic) owns timing, judgment, scoring.
 *   - This component owns Canvas rendering + keyboard input.
 *   - On judgment, emits a "dance" event so PetWindow can sync pet animation.
 *
 * The game follows a three-screen flow:
 *   1. Song Select  — pick a preset song
 *   2. Gameplay     — arrow notes fall; press keys in time
 *   3. Result       — final score / grade / stats
 */

import { ref, onMounted, onUnmounted } from 'vue'
import { useRhythmGame, JUDGMENT, type JudgmentResult, type DanceIntensity, type GamePhase } from '../composables/useRhythmGame'
import { PRESET_SONGS, type RhythmSong, type SongDifficulty, type Note } from '../config/rhythmSongs'

// ── Selectable song entries (flattened: song × difficulty) ─────────
interface SongSelectEntry {
  song: RhythmSong
  diff: SongDifficulty
  label: string
  diffLabel: string
}
const SELECTABLE_ENTRIES: SongSelectEntry[] = PRESET_SONGS.flatMap(s =>
  s.difficulties.map(d => ({
    song: s,
    diff: d,
    label: s.title,
    diffLabel: d.difficulty === 'easy' ? 'Easy' : d.difficulty === 'normal' ? 'Normal' : 'Hard',
  }))
)
const DIFF_COLORS: Record<string, string> = {
  easy: '#7EDA8E',
  normal: '#FFD93D',
  hard: '#FF6B6B',
}

// ── Emits ──────────────────────────────────────────────────────────
const emit = defineEmits<{
  close: []
  dance: [intensity: DanceIntensity]
}>()

// ── Composables ───────────────────────────────────────────────────
const game = useRhythmGame()

// ── Canvas refs ───────────────────────────────────────────────────
const canvasRef = ref<HTMLCanvasElement | null>(null)
let ctx: CanvasRenderingContext2D | null = null

// Render loop controller — prevents stacking when game is opened/closed repeatedly
let rendering = false

// ── Constants ─────────────────────────────────────────────────────
const LANE_COUNT = 4
const LANE_KEYS: Array<Note['direction']> = ['left', 'down', 'up', 'right']
const LANE_COLORS: Record<Note['direction'], string> = {
  left: '#7EB8DA', down: '#7EDA8E', up: '#FFD93D', right: '#FF6B6B',
}
const LANE_GLOW: Record<Note['direction'], string> = {
  left: 'rgba(126, 184, 218, 0.4)', down: 'rgba(126, 218, 142, 0.4)', up: 'rgba(255, 217, 61, 0.4)', right: 'rgba(255, 107, 107, 0.4)',
}

// Drop speed: ms for a note to travel from top to judgment line
const DROP_DURATION_MS = 1600
const JUDGMENT_LINE_Y_RATIO = 0.75 // 75% from game-area top

// ── Local song selection state ────────────────────────────────────
const selectedEntryIndex = ref(0)
const scrollOffset = ref(0)
const CARD_H = 80
const CARD_GAP = 8

// ── Local rendering state ─────────────────────────────────────────
let canvasW = 0
let canvasH = 0
let laneW = 0
let judgmentY = 0

// Particle system
interface Particle {
  x: number; y: number; vx: number; vy: number
  life: number; maxLife: number; color: string; size: number
}
let particles: Particle[] = []

// Floating judgment text
interface FloatText {
  text: string; color: string
  x: number; y: number; life: number; maxLife: number
}
let floatTexts: FloatText[] = []

// Combo fire level (0–1)
let comboFire = 0
// Screen flash on combo milestones (10, 20, 30…)
let comboFlash = 0
// BPM-synced musical beat phase (0–1)
let beatPhase = 0
let lastBeatMs = 0

// ── Lifecycle ─────────────────────────────────────────────────────

onMounted(() => {
  const c = canvasRef.value
  if (!c) return
  ctx = c.getContext('2d')

  game.startLoop()
  game.setOnPhaseChange(onPhaseChange)
})

onUnmounted(() => {
  game.stopLoop()
  removeListeners()
})

// ── Window resize ─────────────────────────────────────────────────
function resizeCanvas() {
  const c = canvasRef.value
  if (!c) return
  const parent = c.parentElement
  if (!parent) return

  canvasW = parent.clientWidth
  canvasH = parent.clientHeight
  c.width = canvasW * devicePixelRatio
  c.height = canvasH * devicePixelRatio
  c.style.width = `${canvasW}px`
  c.style.height = `${canvasH}px`
  if (ctx) ctx.scale(devicePixelRatio, devicePixelRatio)

  laneW = canvasW / LANE_COUNT
  judgmentY = canvasH * JUDGMENT_LINE_Y_RATIO
}

function scrollToSelected() {
  const startY = 85
  const itemTop = startY + selectedEntryIndex.value * (CARD_H + CARD_GAP) - scrollOffset.value
  const itemBottom = itemTop + CARD_H
  const viewTop = 85
  const viewBottom = canvasH - 24
  if (itemTop < viewTop) {
    scrollOffset.value -= (viewTop - itemTop)
  } else if (itemBottom > viewBottom) {
    scrollOffset.value += (itemBottom - viewBottom)
  }
  scrollOffset.value = Math.max(0, scrollOffset.value)
}

function onWheel(e: WheelEvent) {
  if (game.phase.value !== 'selecting') return
  e.preventDefault()
  const totalH = SELECTABLE_ENTRIES.length * (CARD_H + CARD_GAP) - CARD_GAP
  const viewH = canvasH - 85 - 24
  const maxScroll = Math.max(0, totalH - viewH)
  scrollOffset.value = Math.max(0, Math.min(maxScroll, scrollOffset.value + e.deltaY))
}

// ── Keyboard ──────────────────────────────────────────────────────
const keyMap: Record<string, Note['direction']> = {
  ArrowLeft: 'left', ArrowDown: 'down', ArrowUp: 'up', ArrowRight: 'right',
}

function onKeyDown(e: KeyboardEvent) {
  if (e.repeat) return

  if (e.key === 'Escape') {
    if (game.phase.value === 'playing' || game.phase.value === 'paused') {
      game.exit()
      emit('close')
    } else if (game.phase.value === 'selecting' || game.phase.value === 'ended') {
      emit('close')
    }
    return
  }

  if (e.key === ' ' || e.key === 'Enter') {
    if (game.phase.value === 'playing') {
      if (e.key === ' ') game.togglePause()
    } else if (game.phase.value === 'paused') {
      if (e.key === ' ') game.togglePause()
    } else if (game.phase.value === 'selecting') {
      const entry = SELECTABLE_ENTRIES[selectedEntryIndex.value]
      if (entry) startSong(entry.song, entry.diff)
    }
    return
  }

  // Song selection navigation & scrolling
  if (game.phase.value === 'selecting') {
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedEntryIndex.value = Math.max(0, selectedEntryIndex.value - 1)
      scrollToSelected()
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedEntryIndex.value = Math.min(SELECTABLE_ENTRIES.length - 1, selectedEntryIndex.value + 1)
      scrollToSelected()
      return
    }
  }

  const dir = keyMap[e.key]
  if (dir) {
    e.preventDefault()
    game.pressKey(dir)
  }
}

function onKeyUp(e: KeyboardEvent) {
  const dir = keyMap[e.key]
  if (dir) game.releaseKey(dir)
}

function addListeners() {
  window.addEventListener('keydown', onKeyDown)
  window.addEventListener('keyup', onKeyUp)
  window.addEventListener('resize', resizeCanvas)
  window.addEventListener('wheel', onWheel, { passive: false })
}

function removeListeners() {
  window.removeEventListener('keydown', onKeyDown)
  window.removeEventListener('keyup', onKeyUp)
  window.removeEventListener('resize', resizeCanvas)
  window.removeEventListener('wheel', onWheel)
}

// ── Game phase changes ────────────────────────────────────────────
function onPhaseChange(p: GamePhase) {
  if (p === 'playing' || p === 'selecting') {
    addListeners()
    resizeCanvas()
    if (!rendering) {
      rendering = true
      requestAnimationFrame(renderLoop)
    }
  }
  if (p === 'ended') {
    // Still listen so user can press Escape to close
  }
  if (p === 'idle') {
    removeListeners()
    rendering = false
  }
}

// ── Start song ────────────────────────────────────────────────────
function startSong(s: RhythmSong, diff: SongDifficulty) {
  game.startSong(s, diff, onJudgment)
  resizeCanvas()
}

function onJudgment(j: JudgmentResult, _note: Note) {
  emit('dance', game.danceIntensity.value)

  // Add particle effect at the judgment line
  const li = LANE_KEYS.indexOf(_note.direction)
  for (let i = 0; i < 12; i++) {
    particles.push({
      x: li * laneW + laneW / 2,
      y: judgmentY,
      vx: (Math.random() - 0.5) * 6,
      vy: -Math.random() * 4 - 2,
      life: 1,
      maxLife: 0.5 + Math.random() * 0.3,
      color: j.color,
      size: 3 + Math.random() * 4,
    })
  }

  // Float text
  floatTexts.push({
    text: j.label,
    color: j.color,
    x: li * laneW + laneW / 2,
    y: judgmentY - 40,
    life: 1,
    maxLife: 0.8,
  })

  // Combo fire
  if (j.key !== 'MISS' && game.combo.value >= 5) {
    comboFire = Math.min(1, comboFire + 0.15)
  } else if (j.key === 'MISS') {
    comboFire = 0
  }

  // Screen flash on combo milestones (10, 20, 30…)
  if (j.key !== 'MISS' && game.combo.value > 0 && game.combo.value % 10 === 0) {
    comboFlash = 1
  }

  // Track beat phase for lane pulse
  if (j.key !== 'MISS') {
    const bpm = game.song.value?.bpm ?? 120
    const beatMs = 60000 / bpm
    if (_note.time - lastBeatMs >= beatMs * 0.8) {
      beatPhase = 1
      lastBeatMs = _note.time
    }
  }
}

// ── Draw: Arrow shape ─────────────────────────────────────────────
function drawArrow(c: CanvasRenderingContext2D, x: number, y: number, dir: string, size: number, color: string) {
  c.save()
  c.translate(x, y)

  // Rotate so arrow points in the given direction (default: up)
  let rotation = 0
  if (dir === 'right') rotation = Math.PI / 2
  else if (dir === 'down') rotation = Math.PI
  else if (dir === 'left') rotation = -Math.PI / 2
  c.rotate(rotation)

  const hw = size * 0.38
  const hh = size * 0.55
  const stem = size * 0.11

  c.fillStyle = color
  c.beginPath()
  c.moveTo(0, -hh)
  c.lineTo(-hw, -hh * 0.05)
  c.lineTo(-stem, -hh * 0.05)
  c.lineTo(-stem, hh * 0.4)
  c.lineTo(stem, hh * 0.4)
  c.lineTo(stem, -hh * 0.05)
  c.lineTo(hw, -hh * 0.05)
  c.closePath()
  c.fill()

  // White border for clarity
  c.strokeStyle = 'rgba(255,255,255,0.25)'
  c.lineWidth = 1
  c.stroke()

  c.restore()
}

// ── Render loop ───────────────────────────────────────────────────
function renderLoop() {
  if (!ctx || !rendering) return

  // Clear
  ctx.clearRect(0, 0, canvasW, canvasH)

  switch (game.phase.value) {
    case 'selecting':
      drawSelectScreen()
      break
    case 'playing':
      drawGameplay()
      break
    case 'paused':
      drawGameplay()
      drawPauseOverlay()
      break
    case 'ended':
      drawResultScreen()
      break
  }

  requestAnimationFrame(renderLoop)
}

// ── Draw: Song Select Screen ──────────────────────────────────────
function drawSelectScreen() {
  if (!ctx) return
  const c = ctx
  const now = performance.now()

  // Background — same dance-floor gradient
  const grad = c.createLinearGradient(0, 0, 0, canvasH)
  grad.addColorStop(0, '#0f0c29')
  grad.addColorStop(0.3, '#302b63')
  grad.addColorStop(0.7, '#24243e')
  grad.addColorStop(1, '#1a1a2e')
  c.fillStyle = grad
  c.fillRect(0, 0, canvasW, canvasH)

  // Ambient floating orbs
  const t = now * 0.0005
  for (let i = 0; i < 6; i++) {
    const ox = canvasW * (0.15 + 0.7 * ((i * 0.618 + t) % 1))
    const oy = canvasH * (0.15 + 0.7 * ((i * 0.382 + t * 0.7) % 1))
    const or = 40 + 30 * Math.sin(t + i)
    c.fillStyle = `rgba(126, 218, 142, ${0.03 + 0.02 * Math.sin(t * 2 + i)})`
    c.beginPath()
    c.arc(ox, oy, or, 0, Math.PI * 2)
    c.fill()
  }

  // Title
  c.fillStyle = '#FFF'
  c.font = 'bold 22px system-ui, "Segoe UI", "Noto Sans SC", sans-serif'
  c.textAlign = 'center'
  c.fillText('🎵 选择歌曲', canvasW / 2, 40)

  c.font = '14px system-ui, "Segoe UI", "Noto Sans SC", sans-serif'
  c.fillStyle = 'rgba(255, 255, 255, 0.5)'
  c.fillText('Enter / 空格 开始 · ↑↓ 切换 · 滚轮滚动 · Esc 返回', canvasW / 2, 62)

  // Song cards — scrollable
  const startY = 85
  const x = 20
  const w = canvasW - 40
  const listTop = startY
  const listBottom = canvasH - 24

  for (let i = 0; i < SELECTABLE_ENTRIES.length; i++) {
    const cy = startY + i * (CARD_H + CARD_GAP) - scrollOffset.value
    // Skip items outside visible area
    if (cy + CARD_H < listTop || cy > listBottom) continue

    const entry = SELECTABLE_ENTRIES[i]
    const isSelected = selectedEntryIndex.value === i
    const diffColor = DIFF_COLORS[entry.diff.difficulty] || '#FFF'

    // Card background
    c.fillStyle = isSelected ? 'rgba(255, 107, 107, 0.25)' : 'rgba(255, 255, 255, 0.04)'
    c.beginPath()
    c.roundRect(x, cy, w, CARD_H, 8)
    c.fill()

    if (isSelected) {
      c.strokeStyle = '#FF6B6B'
      c.lineWidth = 2
      c.shadowColor = '#FF6B6B'
      c.shadowBlur = 12
      c.stroke()
      c.shadowBlur = 0
    }

    // Difficulty badge
    c.fillStyle = diffColor + '30'
    c.beginPath()
    c.roundRect(x + 12, cy + 10, 52, 22, 4)
    c.fill()
    c.fillStyle = diffColor
    c.font = 'bold 11px system-ui, sans-serif'
    c.textAlign = 'center'
    c.fillText(entry.diffLabel, x + 38, cy + 25)

    // Song title
    c.fillStyle = '#FFF'
    c.font = 'bold 15px system-ui, "Segoe UI", "Noto Sans SC", sans-serif'
    c.textAlign = 'left'
    c.fillText(entry.label, x + 72, cy + 26)

    // Artist
    c.fillStyle = 'rgba(255, 255, 255, 0.5)'
    c.font = '12px system-ui, "Segoe UI", "Noto Sans SC", sans-serif'
    c.fillText(entry.song.artist, x + 72, cy + 46)

    // BPM / Rating
    c.fillStyle = 'rgba(255, 255, 255, 0.35)'
    c.font = '11px system-ui, sans-serif'
    c.textAlign = 'right'
    c.fillText(`${entry.song.bpm} BPM`, x + w - 14, cy + 26)
    c.fillText(`★${entry.diff.rating}`, x + w - 14, cy + 46)

    // Note count
    c.fillStyle = 'rgba(255, 255, 255, 0.25)'
    c.font = '11px system-ui, sans-serif'
    c.fillText(`${entry.diff.notes.length} notes`, x + w - 14, cy + 64)
  }

  // Scroll indicator
  const totalH = SELECTABLE_ENTRIES.length * (CARD_H + CARD_GAP) - CARD_GAP
  const viewH = listBottom - listTop
  if (totalH > viewH) {
    const barH = viewH * (viewH / totalH)
    const barY = listTop + (scrollOffset.value / (totalH - viewH)) * (viewH - barH)
    c.fillStyle = 'rgba(255, 255, 255, 0.15)'
    c.beginPath()
    c.roundRect(canvasW - 5, barY, 3, barH, 2)
    c.fill()
  }

  // Bottom hint
  c.fillStyle = 'rgba(255, 255, 255, 0.15)'
  c.font = '11px system-ui, sans-serif'
  c.textAlign = 'center'
  c.fillText('↑↓ 切换 · Enter / 空格 开始', canvasW / 2, canvasH - 6)
}

// ── Draw: Gameplay ────────────────────────────────────────────────
function drawGameplay() {
  if (!ctx) return
  const c = ctx
  const now = performance.now()

  // ── Background — vibrant dance-floor gradient ────────────────────
  // Frame state decay
  comboFlash = Math.max(0, comboFlash - 0.03)
  beatPhase  = Math.max(0, beatPhase - 0.04)
  const combo = game.combo.value
  const comboGlow = Math.min(1, combo / 30) // 0 at 0 combo, 1 at 30+ combo

  const grad = c.createLinearGradient(0, 0, 0, canvasH)
  grad.addColorStop(0, '#0f0c29')
  grad.addColorStop(0.3, '#302b63')
  grad.addColorStop(0.7, '#24243e')
  grad.addColorStop(1, '#1a1a2e')
  c.fillStyle = grad
  c.fillRect(0, 0, canvasW, canvasH)

  // ── Conveyor belt lanes ─────────────────────────────────────────
  const stripeOffset = (now * 0.15) % 32

  for (let i = 0; i < LANE_COUNT; i++) {
    const lx = i * laneW
    const color = LANE_COLORS[LANE_KEYS[i]]

    // Alternating lane tint
    c.fillStyle = i % 2 === 0 ? 'rgba(255,255,255,0.015)' : 'rgba(255,255,255,0.025)'
    c.fillRect(lx, 0, laneW, canvasH)

    // Lane color glow — pulses brighter at high combo / on beat
    const beatGlow = beatPhase * 0.5
    const laneGlowAlpha = 0.08 + comboGlow * 0.15 + beatGlow
    c.fillStyle = color + Math.round(laneGlowAlpha * 255).toString(16).padStart(2, '0')
    c.fillRect(lx, 0, laneW, canvasH)

    // Scrolling diagonal stripes (conveyor belt texture)
    c.save()
    c.beginPath()
    c.rect(lx + 2, 0, laneW - 4, canvasH)
    c.clip()
    c.strokeStyle = color + '15'
    c.lineWidth = 1.5
    for (let sy = stripeOffset - 32; sy < canvasH + 32; sy += 18) {
      c.beginPath()
      c.moveTo(lx + (sy * 0.4), sy - 16)
      c.lineTo(lx + laneW + (sy * 0.4), sy + 16)
      c.stroke()
    }
    c.restore()

    // Lane divider
    c.strokeStyle = 'rgba(255,255,255,0.05)'
    c.lineWidth = 1
    c.beginPath()
    c.moveTo(lx + laneW, 0)
    c.lineTo(lx + laneW, canvasH)
    c.stroke()
  }

  // ── Judgment zone ───────────────────────────────────────────────
  const jzH = 44
  const jzY = judgmentY - jzH / 2

  const jzGlowIntensity = 0.18 + comboGlow * 0.25
  const jzGrad = c.createLinearGradient(0, jzY, 0, jzY + jzH)
  jzGrad.addColorStop(0, 'rgba(255,107,107,0)')
  jzGrad.addColorStop(0.2, `rgba(255,107,107,${0.08 + comboGlow * 0.12})`)
  jzGrad.addColorStop(0.5, `rgba(255,107,107,${jzGlowIntensity})`)
  jzGrad.addColorStop(0.8, `rgba(255,107,107,${0.08 + comboGlow * 0.12})`)
  jzGrad.addColorStop(1, 'rgba(255,107,107,0)')
  c.fillStyle = jzGrad
  c.fillRect(0, jzY, canvasW, jzH)

  // Judgment line glow
  c.strokeStyle = '#FF6B6B'
  c.lineWidth = 2 + comboGlow * 1.5
  c.shadowColor = '#FF6B6B'
  c.shadowBlur = 18 + comboGlow * 20
  c.beginPath()
  c.moveTo(0, judgmentY)
  c.lineTo(canvasW, judgmentY)
  c.stroke()
  c.shadowBlur = 0

  // Lane target rings at judgment line
  const pulse = 0.8 + 0.2 * Math.sin(now * 0.003)
  const ringScale = 1 + comboGlow * 0.3
  for (let i = 0; i < LANE_COUNT; i++) {
    const ax = i * laneW + laneW / 2
    const col = LANE_COLORS[LANE_KEYS[i]]

    c.strokeStyle = LANE_GLOW[LANE_KEYS[i]]
    c.lineWidth = 2
    c.beginPath()
    c.arc(ax, judgmentY, laneW * 0.3 * pulse * ringScale, 0, Math.PI * 2)
    c.stroke()

    c.strokeStyle = col + '40'
    c.lineWidth = 1.5
    c.beginPath()
    c.arc(ax, judgmentY, laneW * 0.18, 0, Math.PI * 2)
    c.stroke()
  }

  // ── Falling notes ───────────────────────────────────────────────
  const songNotes = game.currentDifficulty.value?.notes ?? []
  const currentTime = game.elapsedMs.value

  for (const n of songNotes) {
    // Judged → invisible immediately
    if (game.hasJudged(n)) continue
    const elapsed = currentTime - n.time
    // Fallback: past judgment window → hide as well
    if (elapsed > JUDGMENT.MISS.max) continue
    if (elapsed < -DROP_DURATION_MS) continue

    const progress = (elapsed + DROP_DURATION_MS) / DROP_DURATION_MS
    const ny = progress * (judgmentY + laneW / 2) - laneW / 2
    const li = LANE_KEYS.indexOf(n.direction)
    if (li < 0) continue
    const nx = li * laneW + laneW / 2
    const color = LANE_COLORS[n.direction]
    const noteSize = laneW * 0.55

    // Arrow glow
    c.save()
    c.shadowColor = color
    c.shadowBlur = 22

    // Draw arrow-shaped note
    drawArrow(c, nx, ny, n.direction, noteSize, color)
    c.restore()
  }

  // ── Hit particles ───────────────────────────────────────────────
  particles = particles.filter(p => {
    p.life -= 1 / 60 / p.maxLife
    p.x += p.vx
    p.y += p.vy
    p.vy += 0.15
    if (p.life > 0) {
      c.globalAlpha = Math.max(0, p.life)
      c.fillStyle = p.color
      c.beginPath()
      c.arc(p.x, p.y, p.size * p.life, 0, Math.PI * 2)
      c.fill()
      c.globalAlpha = 1
      return true
    }
    return false
  })

  // ── Float texts ─────────────────────────────────────────────────
  floatTexts = floatTexts.filter(ft => {
    ft.life -= 1 / 60 / ft.maxLife
    ft.y -= 1.5
    if (ft.life > 0) {
      c.globalAlpha = Math.max(0, ft.life)
      c.fillStyle = ft.color
      c.font = 'bold 20px system-ui, sans-serif'
      c.textAlign = 'center'
      c.fillText(ft.text, ft.x, ft.y)
      c.globalAlpha = 1
      return true
    }
    return false
  })

  // ── Combo fire glow ─────────────────────────────────────────────
  if (comboFire > 0) {
    comboFire -= 0.005
    const grad2 = c.createRadialGradient(canvasW / 2, judgmentY, 0, canvasW / 2, judgmentY, canvasW * 0.3)
    grad2.addColorStop(0, `rgba(255,200,50,${comboFire * 0.12})`)
    grad2.addColorStop(1, 'rgba(255,200,50,0)')
    c.fillStyle = grad2
    c.fillRect(0, judgmentY - canvasW * 0.3, canvasW, canvasW * 0.6)
  }

  // ── Combo milestone screen flash (10 / 20 / 30 …) ─────────────
  if (comboFlash > 0.01) {
    c.fillStyle = `rgba(255,255,255,${comboFlash * 0.15})`
    c.fillRect(0, 0, canvasW, canvasH)
  }

  // ── HUD ─────────────────────────────────────────────────────────
  // Score
  c.fillStyle = '#FFF'
  c.font = 'bold 22px system-ui, sans-serif'
  c.textAlign = 'left'
  c.fillText(String(game.score.value), 16, 28)

  // Accuracy
  c.fillStyle = 'rgba(255,255,255,0.5)'
  c.font = '13px system-ui, sans-serif'
  c.textAlign = 'right'
  c.fillText(`${game.accuracy.value}%`, canvasW - 16, 22)

  // Progress bar
  if (game.totalNotes.value > 0) {
    const px = canvasW - 16 - 100
    const py = 30
    const pw = 100
    const ph = 3
    c.fillStyle = 'rgba(255,255,255,0.1)'
    c.beginPath()
    c.roundRect(px, py, pw, ph, 2)
    c.fill()
    c.fillStyle = 'rgba(255,200,100,0.5)'
    c.beginPath()
    c.roundRect(px, py, pw * (game.judgedNotes.value / game.totalNotes.value), ph, 2)
    c.fill()
  }

  // Song title
  const songTitle = game.song.value?.title ?? ''
  c.fillStyle = 'rgba(255,255,255,0.25)'
  c.font = '11px system-ui, sans-serif'
  c.textAlign = 'left'
  c.fillText(songTitle, 16, 48)

  // Combo
  if (game.combo.value >= 5) {
    c.save()
    const pulse2 = 1 + 0.06 * Math.sin(now * 0.006)
    c.fillStyle = '#FFD700'
    c.font = `bold ${Math.round(20 * pulse2)}px system-ui, sans-serif`
    c.textAlign = 'right'
    c.fillText(`${game.combo.value} COMBO`, canvasW - 16, 48)
    c.restore()
  }

  // Bottom instruction
  c.fillStyle = 'rgba(255,255,255,0.12)'
  c.font = '10px system-ui, sans-serif'
  c.textAlign = 'center'
  c.fillText('◀ ▼ ▲ ▶  |  空格暂停  Esc退出', canvasW / 2, canvasH - 8)
}

// ── Draw: Pause overlay ───────────────────────────────────────────
function drawPauseOverlay() {
  if (!ctx) return
  const c = ctx
  c.fillStyle = 'rgba(15, 12, 41, 0.65)'
  c.fillRect(0, 0, canvasW, canvasH)

  c.fillStyle = '#FFF'
  c.font = 'bold 28px system-ui, sans-serif'
  c.textAlign = 'center'
  c.fillText('⏸ 已暂停', canvasW / 2, canvasH / 2 - 12)

  c.fillStyle = 'rgba(255, 255, 255, 0.4)'
  c.font = '14px system-ui, sans-serif'
  c.fillText('按空格继续 · Esc 退出', canvasW / 2, canvasH / 2 + 22)
}

// ── Draw: Result screen ───────────────────────────────────────────
function drawResultScreen() {
  if (!ctx) return
  const c = ctx
  const now = performance.now()

  const grad = c.createLinearGradient(0, 0, 0, canvasH)
  grad.addColorStop(0, '#0f0c29')
  grad.addColorStop(0.3, '#302b63')
  grad.addColorStop(0.7, '#24243e')
  grad.addColorStop(1, '#1a1a2e')
  c.fillStyle = grad
  c.fillRect(0, 0, canvasW, canvasH)

  // Ambient particles
  const t = now * 0.0008
  for (let i = 0; i < 8; i++) {
    const px = canvasW * (0.1 + 0.8 * ((i * 0.527 + t) % 1))
    const py = canvasH * (0.1 + 0.8 * ((i * 0.723 + t * 0.6) % 1))
    c.fillStyle = `rgba(255, 107, 107, ${0.04 + 0.03 * Math.sin(t * 2 + i)})`
    c.beginPath()
    c.arc(px, py, 3 + 2 * Math.sin(t + i), 0, Math.PI * 2)
    c.fill()
  }

  // Grade (big)
  const gradeColors: Record<string, string> = {
    SS: '#FFD700', S: '#FFA500', A: '#32CD32',
    B: '#87CEEB', C: '#DDA0DD', D: '#FF6B6B',
  }
  const gc = gradeColors[game.grade.value] ?? '#FFF'
  c.fillStyle = gc
  c.shadowColor = gc
  c.shadowBlur = 30
  c.font = 'bold 72px system-ui, sans-serif'
  c.textAlign = 'center'
  c.fillText(game.grade.value, canvasW / 2, canvasH * 0.25)
  c.shadowBlur = 0

  // Score
  c.fillStyle = '#FFF'
  c.font = 'bold 32px system-ui, sans-serif'
  c.fillText(String(game.score.value), canvasW / 2, canvasH * 0.36)

  // Stats grid
  const stats: Array<[string, string, string]> = [
    ['Perfect', String(game.perfectCount.value), '#FFD700'],
    ['Good',    String(game.goodCount.value), '#32CD32'],
    ['OK',      String(game.okCount.value), '#87CEEB'],
    ['Miss',    String(game.missCount.value), '#FF6B6B'],
    ['Max Combo', `${game.maxCombo.value}`, '#FF69B4'],
    ['Accuracy', `${game.accuracy.value}%`, '#FFF'],
  ]

  const startY = canvasH * 0.42
  const rowH = 24
  c.font = '14px system-ui, sans-serif'
  c.textAlign = 'left'
  stats.forEach(([label, value, color], i) => {
    const y = startY + i * rowH
    const cx = canvasW / 2
    c.fillStyle = 'rgba(255, 255, 255, 0.4)'
    c.textAlign = 'right'
    c.fillText(label, cx - 40, y)
    c.fillStyle = color
    c.textAlign = 'left'
    c.fillText(value, cx + 10, y)
  })

  // Footer
  c.fillStyle = 'rgba(255, 255, 255, 0.2)'
  c.font = '13px system-ui, sans-serif'
  c.textAlign = 'center'
  c.fillText('按 Esc 返回', canvasW / 2, canvasH - 20)
}

// ── Canvas click ──────────────────────────────────────────────────
function onCanvasClick(e: MouseEvent) {
  const rect = canvasRef.value?.getBoundingClientRect()
  if (!rect) return
  const mx = e.clientX - rect.left
  const my = e.clientY - rect.top

  if (game.phase.value === 'selecting') {
    const startY = 85
    const x = 20
    const w = canvasW - 40

    for (let i = 0; i < SELECTABLE_ENTRIES.length; i++) {
      const cy = startY + i * (CARD_H + CARD_GAP) - scrollOffset.value
      if (mx >= x && mx <= x + w && my >= cy && my <= cy + CARD_H) {
        selectedEntryIndex.value = i
        const entry = SELECTABLE_ENTRIES[i]
        startSong(entry.song, entry.diff)
        break
      }
    }
    return
  }

  if (game.phase.value === 'ended') {
    emit('close')
  }
}

// ── Expose ────────────────────────────────────────────────────────
defineExpose({
  open() {
    selectedEntryIndex.value = 0
    scrollOffset.value = 0
    resizeCanvas()
    game.enterSelection()
    addListeners()
  },
  dismiss() {
    rendering = false
    game.exit()
    removeListeners()
  },
})
</script>

<template>
  <div class="rhythm-game-area" :class="{ active: game.phase.value !== 'idle' }" v-show="game.phase.value !== 'idle'">
    <canvas
      ref="canvasRef"
      :class="['rhythm-canvas', { active: game.phase.value !== 'idle' }]"
      @click="onCanvasClick"
    />
  </div>
</template>

<style scoped>
.rhythm-game-area {
  position: absolute;
  bottom: 0;
  left: 0;
  width: 100%;
  height: 65%;
  z-index: 50;
  pointer-events: none;
}
.rhythm-game-area.active {
  pointer-events: auto;
}
.rhythm-canvas {
  width: 100%;
  height: 100%;
  display: block;
  cursor: pointer;
}
</style>