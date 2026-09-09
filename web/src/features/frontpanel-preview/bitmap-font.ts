type BitmapFontName = 'small' | 'mid' | 'control-label' | 'control-title'

type BitmapGlyph = readonly number[] | readonly string[]

type BitmapFont = {
  width: number
  height: number
  scale: number
  legacy: boolean
  glyphs: Readonly<Record<string, BitmapGlyph>>
}

const LEGACY_GLYPHS: Readonly<Record<string, readonly string[]>> = {
  ' ': ['000', '000', '000', '000', '000'],
  '.': ['000', '000', '000', '000', '010'],
  '-': ['000', '000', '111', '000', '000'],
  ':': ['000', '010', '000', '010', '000'],
  '/': ['001', '001', '010', '100', '100'],
  '%': ['101', '001', '010', '100', '101'],
  '+': ['000', '010', '111', '010', '000'],
  '*': ['101', '010', '111', '010', '101'],
  '=': ['000', '111', '000', '111', '000'],
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
}

const LEGACY_FALLBACK = ['111', '001', '011', '000', '010']

const FONT_LEGACY_SMALL: BitmapFont = {
  width: 3,
  height: 5,
  scale: 1,
  legacy: true,
  glyphs: { ...LEGACY_GLYPHS, '?': LEGACY_FALLBACK },
}

const FONT_LEGACY_MID: BitmapFont = {
  width: 3,
  height: 5,
  scale: 2,
  legacy: true,
  glyphs: { ...LEGACY_GLYPHS, '?': LEGACY_FALLBACK },
}

const FONT_CONTROL_LABEL: BitmapFont = {
  width: 6,
  height: 10,
  scale: 1,
  legacy: false,
  glyphs: {
    ' ': [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    '?': [0, 28, 34, 4, 8, 8, 0, 8, 0, 0],
    A: [0, 8, 20, 34, 34, 62, 34, 34, 0, 0],
    B: [0, 60, 18, 18, 28, 18, 18, 60, 0, 0],
    C: [0, 28, 34, 32, 32, 32, 34, 28, 0, 0],
    D: [0, 60, 18, 18, 18, 18, 18, 60, 0, 0],
    E: [0, 62, 32, 32, 60, 32, 32, 62, 0, 0],
    F: [0, 62, 32, 32, 60, 32, 32, 32, 0, 0],
    G: [0, 28, 34, 32, 32, 38, 34, 28, 0, 0],
    H: [0, 34, 34, 34, 62, 34, 34, 34, 0, 0],
    I: [0, 28, 8, 8, 8, 8, 8, 28, 0, 0],
    J: [0, 14, 4, 4, 4, 4, 36, 24, 0, 0],
    K: [0, 34, 36, 40, 48, 40, 36, 34, 0, 0],
    L: [0, 32, 32, 32, 32, 32, 32, 62, 0, 0],
    M: [0, 34, 34, 54, 42, 34, 34, 34, 0, 0],
    N: [0, 34, 34, 50, 42, 38, 34, 34, 0, 0],
    O: [0, 28, 34, 34, 34, 34, 34, 28, 0, 0],
    P: [0, 60, 34, 34, 60, 32, 32, 32, 0, 0],
    Q: [0, 28, 34, 34, 34, 34, 42, 28, 2, 0],
    R: [0, 60, 34, 34, 60, 40, 36, 34, 0, 0],
    S: [0, 28, 34, 32, 28, 2, 34, 28, 0, 0],
    T: [0, 62, 8, 8, 8, 8, 8, 8, 0, 0],
    U: [0, 34, 34, 34, 34, 34, 34, 28, 0, 0],
    V: [0, 34, 34, 34, 20, 20, 20, 8, 0, 0],
    W: [0, 34, 34, 34, 42, 42, 54, 34, 0, 0],
    X: [0, 34, 34, 20, 8, 20, 34, 34, 0, 0],
    Y: [0, 34, 34, 20, 8, 8, 8, 8, 0, 0],
    Z: [0, 62, 2, 4, 8, 16, 32, 62, 0, 0],
  },
}

const FONT_CONTROL_TITLE: BitmapFont = {
  width: 8,
  height: 13,
  scale: 1,
  legacy: false,
  glyphs: {
    ' ': [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    '?': [0, 60, 66, 66, 2, 4, 8, 8, 0, 8, 0, 0, 0],
    A: [0, 24, 36, 66, 66, 66, 126, 66, 66, 66, 0, 0, 0],
    B: [0, 120, 68, 66, 68, 120, 68, 66, 68, 120, 0, 0, 0],
    C: [0, 60, 66, 64, 64, 64, 64, 64, 66, 60, 0, 0, 0],
    D: [0, 120, 68, 66, 66, 66, 66, 66, 68, 120, 0, 0, 0],
    E: [0, 126, 64, 64, 64, 120, 64, 64, 64, 126, 0, 0, 0],
    F: [0, 126, 64, 64, 64, 120, 64, 64, 64, 64, 0, 0, 0],
    G: [0, 60, 66, 64, 64, 78, 66, 70, 58, 0, 0, 0, 0],
    H: [0, 66, 66, 66, 66, 126, 66, 66, 66, 66, 0, 0, 0],
    I: [0, 124, 16, 16, 16, 16, 16, 16, 16, 124, 0, 0, 0],
    J: [0, 31, 4, 4, 4, 4, 4, 4, 68, 56, 0, 0, 0],
    K: [0, 66, 68, 72, 80, 96, 80, 72, 68, 66, 0, 0, 0],
    L: [0, 64, 64, 64, 64, 64, 64, 64, 64, 126, 0, 0, 0],
    M: [0, 130, 130, 198, 170, 146, 146, 130, 130, 130, 0, 0, 0],
    N: [0, 66, 66, 98, 82, 74, 70, 66, 66, 66, 0, 0, 0],
    O: [0, 60, 66, 66, 66, 66, 66, 66, 66, 60, 0, 0, 0],
    P: [0, 124, 66, 66, 66, 124, 64, 64, 64, 64, 0, 0, 0],
    Q: [0, 60, 66, 66, 66, 66, 66, 82, 74, 60, 2, 0, 0],
    R: [0, 124, 66, 66, 66, 124, 80, 72, 68, 66, 0, 0, 0],
    S: [0, 60, 66, 64, 64, 60, 2, 2, 66, 60, 0, 0, 0],
    T: [0, 254, 16, 16, 16, 16, 16, 16, 16, 16, 0, 0, 0],
    U: [0, 66, 66, 66, 66, 66, 66, 66, 66, 60, 0, 0, 0],
    V: [0, 130, 130, 68, 68, 68, 40, 40, 40, 16, 0, 0, 0],
    W: [0, 130, 130, 130, 130, 146, 146, 146, 170, 68, 0, 0, 0],
    X: [0, 130, 130, 68, 40, 16, 40, 68, 130, 130, 0, 0, 0],
    Y: [0, 130, 130, 68, 40, 16, 16, 16, 16, 16, 0, 0, 0],
    Z: [0, 126, 2, 4, 8, 16, 32, 64, 64, 126, 0, 0, 0],
  },
}

function resolveFont(fontOrScale: BitmapFontName | number): BitmapFont {
  if (typeof fontOrScale === 'number') return fontOrScale >= 2 ? FONT_LEGACY_MID : FONT_LEGACY_SMALL
  if (fontOrScale === 'control-label') return FONT_CONTROL_LABEL
  if (fontOrScale === 'control-title') return FONT_CONTROL_TITLE
  return fontOrScale === 'mid' ? FONT_LEGACY_MID : FONT_LEGACY_SMALL
}

function normalizeText(text: string) {
  return text.toUpperCase()
}

function glyphFor(font: BitmapFont, char: string): BitmapGlyph {
  return font.glyphs[char] ?? font.glyphs['?']
}

export function measureBitmapText(
  text: string,
  fontOrScale: BitmapFontName | number = 'small',
  letterSpacing = 1
) {
  const normalized = normalizeText(text)
  if (!normalized.length) return 0
  const font = resolveFont(fontOrScale)
  return (normalized.length * font.width + (normalized.length - 1) * letterSpacing) * font.scale
}

export interface BitmapTextOptions {
  color: string
  font?: BitmapFontName
  scale?: number
  align?: 'left' | 'center' | 'right'
  letterSpacing?: number
}

export function drawBitmapText(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  options: BitmapTextOptions
) {
  const fontSelector = options.font ?? options.scale ?? 1
  const font = resolveFont(fontSelector)
  const letterSpacing = options.letterSpacing ?? 1
  const normalized = normalizeText(text)
  const totalWidth = measureBitmapText(normalized, fontSelector, letterSpacing)

  let cursorX = x
  if (options.align === 'center') cursorX = Math.round(x - totalWidth / 2)
  if (options.align === 'right') cursorX = x - totalWidth

  ctx.fillStyle = options.color
  for (const char of normalized) {
    const glyph = glyphFor(font, char)
    if (font.legacy) {
      for (const [rowIndex, row] of (glyph as readonly string[]).entries()) {
        for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
          if (row[columnIndex] !== '1') continue
          ctx.fillRect(
            cursorX + columnIndex * font.scale,
            y + rowIndex * font.scale,
            font.scale,
            font.scale
          )
        }
      }
    } else {
      for (const [rowIndex, row] of (glyph as readonly number[]).entries()) {
        for (let columnIndex = 0; columnIndex < font.width; columnIndex += 1) {
          if ((row & (1 << (font.width - 1 - columnIndex))) === 0) continue
          ctx.fillRect(cursorX + columnIndex, y + rowIndex, 1, 1)
        }
      }
    }
    cursorX += (font.width + letterSpacing) * font.scale
  }
}

export function bitmapTextHeight(fontOrScale: BitmapFontName | number = 'small') {
  const font = resolveFont(fontOrScale)
  return font.height * font.scale
}
