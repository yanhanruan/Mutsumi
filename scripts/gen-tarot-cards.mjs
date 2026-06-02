/**
 * gen-tarot-cards.mjs — generate placeholder art for the full 78-card deck.
 *
 * Emits 78 front SVGs (00.svg … 77.svg) and one card-back.svg into
 * public/assets/tarot/. These are stand-ins so the draw / flip / reveal flow
 * works out of the box; drop real artwork with the same filenames to replace
 * them (delete the files to fall back to these placeholders).
 *
 * Run:  node scripts/gen-tarot-cards.mjs
 *
 * NOTE: ids/names mirror TAROT_DECK in src/config/tarot.ts (Major Arcana 0–21,
 * then Wands, Cups, Swords, Pentacles 22–77).
 */
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const OUT_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '../public/assets/tarot')

const ROMAN = ['0', 'I', 'II', 'III', 'IV', 'V', 'VI', 'VII', 'VIII', 'IX', 'X',
               'XI', 'XII', 'XIII', 'XIV', 'XV', 'XVI', 'XVII', 'XVIII', 'XIX', 'XX', 'XXI']

// ── Major Arcana (0–21) ──────────────────────────────────────────────────
const MAJORS = [
  { name: 'The Fool',           hue: 48,  symbol: '🃏' },
  { name: 'The Magician',       hue: 12,  symbol: '🪄' },
  { name: 'The High Priestess', hue: 250, symbol: '🌙' },
  { name: 'The Empress',        hue: 330, symbol: '👑' },
  { name: 'The Emperor',        hue: 0,   symbol: '🏛️' },
  { name: 'The Hierophant',     hue: 36,  symbol: '🗝️' },
  { name: 'The Lovers',         hue: 345, symbol: '💞' },
  { name: 'The Chariot',        hue: 210, symbol: '🏇' },
  { name: 'Strength',           hue: 30,  symbol: '🦁' },
  { name: 'The Hermit',         hue: 200, symbol: '🏮' },
  { name: 'Wheel of Fortune',   hue: 280, symbol: '🎡' },
  { name: 'Justice',            hue: 190, symbol: '⚖️' },
  { name: 'The Hanged Man',     hue: 170, symbol: '🙃' },
  { name: 'Death',              hue: 270, symbol: '🦋' },
  { name: 'Temperance',         hue: 160, symbol: '🍷' },
  { name: 'The Devil',          hue: 350, symbol: '😈' },
  { name: 'The Tower',          hue: 8,   symbol: '🗼' },
  { name: 'The Star',           hue: 195, symbol: '⭐' },
  { name: 'The Moon',           hue: 235, symbol: '🌚' },
  { name: 'The Sun',            hue: 45,  symbol: '☀️' },
  { name: 'Judgement',          hue: 215, symbol: '🎺' },
  { name: 'The World',          hue: 140, symbol: '🌍' },
]

// ── Minor Arcana (22–77) ─────────────────────────────────────────────────
const SUITS = [
  { name: 'Wands',     symbol: '🔥', hueBase: 18 },
  { name: 'Cups',      symbol: '🍷', hueBase: 200 },
  { name: 'Swords',    symbol: '⚔️', hueBase: 188 },
  { name: 'Pentacles', symbol: '🪙', hueBase: 120 },
]
const RANK_NAMES  = ['Ace', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven',
                     'Eight', 'Nine', 'Ten', 'Page', 'Knight', 'Queen', 'King']
const RANK_LABELS = ['Ace', '2', '3', '4', '5', '6', '7', '8', '9', '10',
                     'Page', 'Knight', 'Queen', 'King']

/** Build the full 78-entry card list with { name, hue, symbol, label }. */
function buildCards() {
  const cards = MAJORS.map((m, i) => ({ ...m, label: ROMAN[i] }))
  for (const suit of SUITS) {
    RANK_NAMES.forEach((rank, r) => {
      cards.push({
        name:   `${rank} of ${suit.name}`,
        hue:    (suit.hueBase + r * 3) % 360,
        symbol: suit.symbol,
        label:  RANK_LABELS[r],
      })
    })
  }
  return cards
}

const W = 300, H = 500, R = 24

/** Corner star flourish at (x, y). */
function star(x, y, fill) {
  return `<text x="${x}" y="${y}" font-size="20" fill="${fill}" text-anchor="middle" dominant-baseline="central">✦</text>`
}

function frontSvg({ name, hue, symbol, label }) {
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
        font-weight="700" fill="#fff" text-anchor="middle">${label}</text>
  <circle cx="${W / 2}" cy="${H / 2 - 18}" r="86" fill="rgba(255,255,255,0.22)"/>
  <text x="${W / 2}" y="${H / 2 - 18}" font-size="120" text-anchor="middle"
        dominant-baseline="central">${symbol}</text>
  <rect x="34" y="${H - 92}" width="${W - 68}" height="50" rx="12" fill="rgba(255,255,255,0.85)"/>
  <text x="${W / 2}" y="${H - 67}" font-family="Georgia, 'Times New Roman', serif" font-size="20"
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

const cards = buildCards()
mkdirSync(OUT_DIR, { recursive: true })
writeFileSync(resolve(OUT_DIR, 'card-back.svg'), backSvg(), 'utf8')
cards.forEach((card, id) => {
  writeFileSync(resolve(OUT_DIR, `${String(id).padStart(2, '0')}.svg`), frontSvg(card), 'utf8')
})
console.log(`Generated ${cards.length} card faces + card-back.svg in ${OUT_DIR}`)
