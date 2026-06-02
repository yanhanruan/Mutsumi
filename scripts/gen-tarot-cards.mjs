/**
 * gen-tarot-cards.mjs — generate placeholder Major Arcana card art.
 *
 * Emits 22 front SVGs (00.svg … 21.svg) and one card-back.svg into
 * public/assets/tarot/. These are stand-ins so the entrance / flip / reveal
 * animations and audio can be demonstrated now; replace the files later with
 * real artwork (keep the same filenames and the component needs no changes).
 *
 * Run:  node scripts/gen-tarot-cards.mjs
 *
 * NOTE: the id/name/hue values below mirror MAJOR_ARCANA in
 * src/config/tarot.ts. If you change the table there, re-run this script.
 */
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const OUT_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../public/assets/tarot')

const ROMAN = ['0', 'I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X',
               'XI', 'XII', 'XIII', 'XIV', 'XV', 'XVI', 'XVII', 'XVIII', 'XIX', 'XX', 'XXI']

const CARDS = [
  { id: 0,  name: 'The Fool',           hue: 48,  symbol: '🃏' },
  { id: 1,  name: 'The Magician',       hue: 12,  symbol: '🪄' },
  { id: 2,  name: 'The High Priestess', hue: 250, symbol: '🌙' },
  { id: 3,  name: 'The Empress',        hue: 330, symbol: '👑' },
  { id: 4,  name: 'The Emperor',        hue: 0,   symbol: '🏛️' },
  { id: 5,  name: 'The Hierophant',     hue: 36,  symbol: '🗝️' },
  { id: 6,  name: 'The Lovers',         hue: 345, symbol: '💞' },
  { id: 7,  name: 'The Chariot',        hue: 210, symbol: '🏇' },
  { id: 8,  name: 'Strength',           hue: 30,  symbol: '🦁' },
  { id: 9,  name: 'The Hermit',         hue: 200, symbol: '🏮' },
  { id: 10, name: 'Wheel of Fortune',   hue: 280, symbol: '🎡' },
  { id: 11, name: 'Justice',            hue: 190, symbol: '⚖️' },
  { id: 12, name: 'The Hanged Man',     hue: 170, symbol: '🙃' },
  { id: 13, name: 'Death',              hue: 270, symbol: '🦋' },
  { id: 14, name: 'Temperance',         hue: 160, symbol: '🍷' },
  { id: 15, name: 'The Devil',          hue: 350, symbol: '😈' },
  { id: 16, name: 'The Tower',          hue: 8,   symbol: '🗼' },
  { id: 17, name: 'The Star',           hue: 195, symbol: '⭐' },
  { id: 18, name: 'The Moon',           hue: 235, symbol: '🌚' },
  { id: 19, name: 'The Sun',            hue: 45,  symbol: '☀️' },
  { id: 20, name: 'Judgement',          hue: 215, symbol: '🎺' },
  { id: 21, name: 'The World',          hue: 140, symbol: '🌍' },
]

const W = 300, H = 500, R = 24

/** Corner star flourish at (x, y). */
function star(x, y, fill) {
  return `<text x="${x}" y="${y}" font-size="20" fill="${fill}" text-anchor="middle" dominant-baseline="central">✦</text>`
}

function frontSvg({ name, hue, symbol }, roman) {
  const c1 = `hsl(${hue}, 72%, 64%)`
  const c2 = `hsl(${(hue + 40) % 360}, 60%, 40%)`
  const ink = `hsl(${hue}, 40%, 18%)`
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0.4" y2="1">
      <stop offset="0" stop-color="${c1}"/>
      <stop offset="1" stop-color="${c2}"/>
    </linearGradient>
  </defs>
  <rect x="3" y="3" width="${W - 6}" height="${H - 6}" rx="${R}" fill="url(#bg)"/>
  <rect x="14" y="14" width="${W - 28}" height="${H - 28}" rx="${R - 8}"
        fill="none" stroke="rgba(255,255,255,0.65)" stroke-width="2"/>
  ${star(30, 32, 'rgba(255,255,255,0.7)')}
  ${star(W - 30, 32, 'rgba(255,255,255,0.7)')}
  ${star(30, H - 30, 'rgba(255,255,255,0.7)')}
  ${star(W - 30, H - 30, 'rgba(255,255,255,0.7)')}
  <text x="${W / 2}" y="62" font-family="Georgia, 'Times New Roman', serif" font-size="30"
        font-weight="700" fill="#fff" text-anchor="middle">${roman}</text>
  <circle cx="${W / 2}" cy="${H / 2 - 18}" r="86" fill="rgba(255,255,255,0.22)"/>
  <text x="${W / 2}" y="${H / 2 - 18}" font-size="120" text-anchor="middle"
        dominant-baseline="central">${symbol}</text>
  <rect x="34" y="${H - 92}" width="${W - 68}" height="50" rx="12" fill="rgba(255,255,255,0.85)"/>
  <text x="${W / 2}" y="${H - 67}" font-family="Georgia, 'Times New Roman', serif" font-size="21"
        font-weight="700" fill="${ink}" text-anchor="middle" dominant-baseline="central">${name}</text>
</svg>`
}

function backSvg() {
  // App-consistent frosted-green back.
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">
  <defs>
    <linearGradient id="gb" x1="0" y1="0" x2="0.3" y2="1">
      <stop offset="0" stop-color="#dff0df"/>
      <stop offset="1" stop-color="#a9cda9"/>
    </linearGradient>
    <pattern id="lattice" width="34" height="34" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <path d="M0 17 H34 M17 0 V34" stroke="rgba(86,122,86,0.28)" stroke-width="1.5"/>
    </pattern>
  </defs>
  <rect x="3" y="3" width="${W - 6}" height="${H - 6}" rx="${R}" fill="url(#gb)"/>
  <rect x="3" y="3" width="${W - 6}" height="${H - 6}" rx="${R}" fill="url(#lattice)"/>
  <rect x="16" y="16" width="${W - 32}" height="${H - 32}" rx="${R - 8}"
        fill="none" stroke="rgba(86,122,86,0.55)" stroke-width="2.5"/>
  <circle cx="${W / 2}" cy="${H / 2}" r="70" fill="rgba(245,250,245,0.85)"
          stroke="rgba(86,122,86,0.5)" stroke-width="2"/>
  <text x="${W / 2}" y="${H / 2}" font-size="64" text-anchor="middle"
        dominant-baseline="central" fill="rgba(70,108,70,0.85)">✦</text>
</svg>`
}

mkdirSync(OUT_DIR, { recursive: true })
writeFileSync(resolve(OUT_DIR, 'card-back.svg'), backSvg(), 'utf8')
for (const card of CARDS) {
  const file = `${String(card.id).padStart(2, '0')}.svg`
  writeFileSync(resolve(OUT_DIR, file), frontSvg(card, ROMAN[card.id]), 'utf8')
}
console.log(`Generated ${CARDS.length} card faces + card-back.svg in ${OUT_DIR}`)
