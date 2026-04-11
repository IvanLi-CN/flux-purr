const GLYPHS: Record<string, string[]> = {
  ' ': ['000', '000', '000', '000', '000'],
  '.': ['000', '000', '000', '000', '010'],
  '-': ['000', '000', '111', '000', '000'],
  ':': ['000', '010', '000', '010', '000'],
  '/': ['001', '001', '010', '100', '100'],
  '%': ['101', '001', '010', '100', '101'],
  '+': ['000', '010', '111', '010', '000'],
  '°': ['010', '101', '010', '000', '000'],
  '0': ['111', '101', '101', '101', '111'],
  '1': ['010', '110', '010', '010', '111'],
  '2': ['111', '001', '111', '100', '111'],
  '3': ['111', '001', '111', '001', '111'],
  '4': ['101', '101', '111', '001', '001'],
  '5': ['111', '100', '111', '001', '111'],
  '6': ['111', '100', '111', '101', '111'],
  '7': ['111', '001', '001', '001', '001'],
  '8': ['111', '101', '111', '101', '111'],
  '9': ['111', '101', '111', '001', '111'],
  A: ['111', '101', '111', '101', '101'],
  B: ['110', '101', '110', '101', '110'],
  C: ['111', '100', '100', '100', '111'],
  D: ['110', '101', '101', '101', '110'],
  E: ['111', '100', '110', '100', '111'],
  F: ['111', '100', '110', '100', '100'],
  G: ['111', '100', '101', '101', '111'],
  H: ['101', '101', '111', '101', '101'],
  I: ['111', '010', '010', '010', '111'],
  J: ['001', '001', '001', '101', '111'],
  K: ['101', '101', '110', '101', '101'],
  L: ['100', '100', '100', '100', '111'],
  M: ['101', '111', '111', '101', '101'],
  N: ['101', '111', '111', '111', '101'],
  O: ['111', '101', '101', '101', '111'],
  P: ['110', '101', '110', '100', '100'],
  Q: ['111', '101', '101', '111', '001'],
  R: ['110', '101', '110', '101', '101'],
  S: ['111', '100', '111', '001', '111'],
  T: ['111', '010', '010', '010', '010'],
  U: ['101', '101', '101', '101', '111'],
  V: ['101', '101', '101', '101', '010'],
  W: ['101', '101', '111', '111', '101'],
  X: ['101', '101', '010', '101', '101'],
  Y: ['101', '101', '010', '010', '010'],
  Z: ['111', '001', '010', '100', '111'],
} as const

const FALLBACK_GLYPH = ['111', '001', '011', '000', '010']
const FONT_WIDTH = 3
const FONT_HEIGHT = 5

export interface BitmapTextOptions {
  color: string
  scale?: number
  align?: 'left' | 'center' | 'right'
  letterSpacing?: number
}

function normalizeText(text: string) {
  return text.toUpperCase()
}

function glyphFor(char: string) {
  return GLYPHS[char] ?? FALLBACK_GLYPH
}

export function measureBitmapText(text: string, scale = 1, letterSpacing = 1) {
  const normalized = normalizeText(text)
  if (!normalized.length) return 0
  const rawWidth = normalized.length * FONT_WIDTH + (normalized.length - 1) * letterSpacing
  return rawWidth * scale
}

export function drawBitmapText(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  options: BitmapTextOptions
) {
  const scale = options.scale ?? 1
  const letterSpacing = options.letterSpacing ?? 1
  const normalized = normalizeText(text)
  const totalWidth = measureBitmapText(normalized, scale, letterSpacing)

  let cursorX = x
  if (options.align === 'center') cursorX = Math.round(x - totalWidth / 2)
  if (options.align === 'right') cursorX = x - totalWidth

  ctx.fillStyle = options.color
  for (const char of normalized) {
    const glyph = glyphFor(char)
    glyph.forEach((row, rowIndex) => {
      for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
        if (row[columnIndex] !== '1') continue
        ctx.fillRect(cursorX + columnIndex * scale, y + rowIndex * scale, scale, scale)
      }
    })
    cursorX += (FONT_WIDTH + letterSpacing) * scale
  }
}

export function bitmapTextHeight(scale = 1) {
  return FONT_HEIGHT * scale
}
