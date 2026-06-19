/**
 * `v-tip` — a theme-styled tooltip rendered at `document.body`.
 *
 * A CSS `::after` tooltip is part of the element's box, so it is clipped by any
 * ancestor with `overflow:hidden` (the composer card) or a scroll container (the
 * emoji grid — where a wide nowrap pseudo-element even forces a scrollbar), and
 * it runs off-screen for edge-anchored controls (jump-to-latest). This directive
 * sidesteps all of that by teleporting one shared tooltip element to `<body>` and
 * positioning it with the hovered element's viewport rect — so it never clips and
 * never affects layout.
 *
 * Usage:  <button v-tip="'Send'">…</button>
 */
import type { Directive } from 'vue'

interface TipTarget extends HTMLElement {
  _tipText?: string
}

let tipEl: HTMLDivElement | null = null
let active: TipTarget | null = null
let frame = 0

/** Lazily create the singleton tooltip node + its styles on first use. */
function init(): HTMLDivElement {
  if (tipEl) return tipEl
  const style = document.createElement('style')
  style.textContent = `
.app-tip{position:fixed;left:0;top:0;z-index:9999;pointer-events:none;
  max-width:220px;padding:4px 9px;border-radius:8px;text-align:center;
  font-family:system-ui,"Segoe UI","Noto Sans SC","Noto Sans JP",sans-serif;
  font-size:11px;font-weight:500;line-height:1.3;color:#2a4a2a;
  background:rgba(245,250,245,0.97);border:1px solid rgba(148,185,148,0.45);
  box-shadow:0 2px 8px rgba(40,70,40,0.14);
  opacity:0;transition:opacity 120ms ease;}
.app-tip.show{opacity:1;}`
  document.head.appendChild(style)
  tipEl = document.createElement('div')
  tipEl.className = 'app-tip'
  document.body.appendChild(tipEl)
  // Any scroll/resize invalidates the anchored position — just hide.
  window.addEventListener('scroll', hide, true)
  window.addEventListener('resize', hide, true)
  return tipEl
}

/** Anchor below the target, flipping above when there is no room; clamp to viewport. */
function place(target: HTMLElement) {
  const el = tipEl!
  const r = target.getBoundingClientRect()
  const w = el.offsetWidth
  const h = el.offsetHeight
  const gap = 8
  let top = r.bottom + gap
  if (top + h > window.innerHeight - 4) top = r.top - gap - h
  let left = r.left + r.width / 2 - w / 2
  left = Math.max(4, Math.min(left, window.innerWidth - w - 4))
  el.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`
}

function show(target: TipTarget) {
  const text = target._tipText
  if (!text) return
  const el = init()
  active = target
  el.textContent = text
  place(target)           // measure + position with the text in place
  cancelAnimationFrame(frame)
  frame = requestAnimationFrame(() => el.classList.add('show'))
}

function hide() {
  active = null
  tipEl?.classList.remove('show')
}

const onEnter = (e: Event) => show(e.currentTarget as TipTarget)
const onLeave = () => hide()

export const vTip: Directive<TipTarget, string> = {
  mounted(el, binding) {
    el._tipText = binding.value
    el.addEventListener('mouseenter', onEnter)
    el.addEventListener('mouseleave', onLeave)
    el.addEventListener('mousedown', onLeave)   // dismiss on click
  },
  updated(el, binding) {
    el._tipText = binding.value
    if (active === el && tipEl) {
      tipEl.textContent = binding.value ?? ''
      place(el)
    }
  },
  beforeUnmount(el) {
    el.removeEventListener('mouseenter', onEnter)
    el.removeEventListener('mouseleave', onLeave)
    el.removeEventListener('mousedown', onLeave)
    if (active === el) hide()
  },
}
