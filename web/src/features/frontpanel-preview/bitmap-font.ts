type BitmapFontName = 'small' | 'mid' | 'control-label' | 'control-title'

type BitmapFont = {
  width: number
  height: number
  glyphs: Readonly<Record<string, readonly number[]>>
}

const FONT_4X6: BitmapFont = {
  width: 4,
  height: 6,
  glyphs: {
    ' ': [0, 0, 0, 0, 0, 0],
    '.': [0, 0, 0, 0, 4, 0],
    '-': [0, 0, 14, 0, 0, 0],
    ':': [0, 4, 0, 0, 4, 0],
    '/': [2, 2, 4, 8, 8, 0],
    '%': [8, 2, 4, 8, 2, 0],
    '+': [4, 4, 14, 4, 4, 0],
    '*': [10, 4, 14, 4, 10, 0],
    '=': [0, 14, 0, 14, 0, 0],
    '!': [4, 4, 4, 0, 4, 0],
    '?': [12, 2, 4, 0, 4, 0],
    '0': [4, 10, 14, 10, 4, 0],
    '1': [4, 12, 4, 4, 14, 0],
    '2': [4, 10, 2, 4, 14, 0],
    '3': [14, 2, 4, 2, 12, 0],
    '4': [10, 10, 14, 2, 2, 0],
    '5': [14, 8, 12, 2, 12, 0],
    '6': [6, 8, 12, 10, 4, 0],
    '7': [14, 2, 4, 8, 8, 0],
    '8': [6, 10, 4, 10, 12, 0],
    '9': [4, 10, 6, 2, 12, 0],
    A: [4, 10, 14, 10, 10, 0],
    B: [12, 10, 12, 10, 12, 0],
    C: [4, 10, 8, 10, 4, 0],
    D: [12, 10, 10, 10, 12, 0],
    E: [14, 8, 12, 8, 14, 0],
    F: [14, 8, 12, 8, 8, 0],
    G: [6, 8, 10, 10, 6, 0],
    H: [10, 10, 14, 10, 10, 0],
    I: [14, 4, 4, 4, 14, 0],
    J: [2, 2, 2, 10, 4, 0],
    K: [10, 10, 12, 10, 10, 0],
    L: [8, 8, 8, 8, 14, 0],
    M: [10, 14, 14, 10, 10, 0],
    N: [2, 10, 14, 10, 8, 0],
    O: [4, 10, 10, 10, 4, 0],
    P: [12, 10, 12, 8, 8, 0],
    Q: [4, 10, 10, 10, 4, 2],
    R: [12, 10, 12, 10, 10, 0],
    S: [6, 8, 4, 2, 12, 0],
    T: [14, 4, 4, 4, 4, 0],
    U: [10, 10, 10, 10, 14, 0],
    V: [10, 10, 10, 14, 4, 0],
    W: [10, 10, 14, 14, 10, 0],
    X: [10, 10, 4, 10, 10, 0],
    Y: [10, 10, 4, 4, 4, 0],
    Z: [14, 2, 4, 8, 14, 0],
  },
}

const FONT_5X8: BitmapFont = {
  width: 5,
  height: 8,
  glyphs: {
    ' ': [0, 0, 0, 0, 0, 0, 0, 0],
    '.': [0, 0, 0, 0, 0, 4, 14, 4],
    '-': [0, 0, 0, 0, 30, 0, 0, 0],
    ':': [0, 0, 12, 12, 0, 12, 12, 0],
    '/': [0, 2, 2, 4, 8, 16, 16, 0],
    '%': [0, 8, 10, 4, 10, 2, 0, 0],
    '+': [0, 0, 4, 4, 31, 4, 4, 0],
    '*': [0, 0, 18, 12, 30, 12, 18, 0],
    '=': [0, 0, 0, 30, 0, 30, 0, 0],
    '!': [0, 4, 4, 4, 4, 0, 4, 0],
    '?': [0, 4, 10, 2, 4, 0, 4, 0],
    '0': [0, 4, 10, 10, 10, 10, 4, 0],
    '1': [0, 4, 12, 4, 4, 4, 14, 0],
    '2': [0, 12, 18, 2, 12, 16, 30, 0],
    '3': [0, 30, 4, 12, 2, 18, 12, 0],
    '4': [0, 4, 12, 20, 30, 4, 4, 0],
    '5': [0, 30, 16, 28, 2, 18, 12, 0],
    '6': [0, 12, 16, 28, 18, 18, 12, 0],
    '7': [0, 30, 2, 4, 4, 8, 8, 0],
    '8': [0, 12, 18, 12, 18, 18, 12, 0],
    '9': [0, 12, 18, 18, 14, 2, 12, 0],
    A: [0, 12, 18, 18, 30, 18, 18, 0],
    B: [0, 28, 18, 28, 18, 18, 28, 0],
    C: [0, 12, 18, 16, 16, 18, 12, 0],
    D: [0, 28, 18, 18, 18, 18, 28, 0],
    E: [0, 30, 16, 28, 16, 16, 30, 0],
    F: [0, 30, 16, 28, 16, 16, 16, 0],
    G: [0, 12, 18, 16, 22, 18, 12, 0],
    H: [0, 18, 18, 30, 18, 18, 18, 0],
    I: [0, 14, 4, 4, 4, 4, 14, 0],
    J: [0, 14, 4, 4, 4, 20, 8, 0],
    K: [0, 18, 20, 24, 20, 20, 18, 0],
    L: [0, 16, 16, 16, 16, 16, 30, 0],
    M: [0, 18, 30, 30, 18, 18, 18, 0],
    N: [0, 18, 26, 30, 22, 22, 18, 0],
    O: [0, 12, 18, 18, 18, 18, 12, 0],
    P: [0, 28, 18, 18, 28, 16, 16, 0],
    Q: [0, 12, 18, 18, 26, 22, 12, 2],
    R: [0, 28, 18, 18, 28, 18, 18, 0],
    S: [0, 12, 18, 8, 4, 18, 12, 0],
    T: [0, 14, 4, 4, 4, 4, 4, 0],
    U: [0, 18, 18, 18, 18, 18, 12, 0],
    V: [0, 18, 18, 18, 18, 12, 12, 0],
    W: [0, 18, 18, 18, 30, 30, 18, 0],
    X: [0, 18, 18, 12, 12, 18, 18, 0],
    Y: [0, 17, 17, 10, 4, 4, 4, 0],
    Z: [0, 30, 2, 4, 8, 16, 30, 0],
  },
}

const FONT_CONTROL_LABEL: BitmapFont = {
  width: 6,
  height: 10,
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
  if (typeof fontOrScale === 'number') return fontOrScale >= 2 ? FONT_5X8 : FONT_4X6
  if (fontOrScale === 'control-label') return FONT_CONTROL_LABEL
  if (fontOrScale === 'control-title') return FONT_CONTROL_TITLE
  return fontOrScale === 'mid' ? FONT_5X8 : FONT_4X6
}

function normalizeText(text: string) {
  return text.toUpperCase()
}

function glyphFor(font: BitmapFont, char: string) {
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
  return normalized.length * font.width + (normalized.length - 1) * letterSpacing
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
  const font = resolveFont(options.font ?? options.scale ?? 1)
  const letterSpacing = options.letterSpacing ?? 1
  const normalized = normalizeText(text)
  const totalWidth = measureBitmapText(
    normalized,
    font === FONT_5X8
      ? 'mid'
      : font === FONT_CONTROL_LABEL
        ? 'control-label'
        : font === FONT_CONTROL_TITLE
          ? 'control-title'
          : 'small',
    letterSpacing
  )

  let cursorX = x
  if (options.align === 'center') cursorX = Math.round(x - totalWidth / 2)
  if (options.align === 'right') cursorX = x - totalWidth

  ctx.fillStyle = options.color
  for (const char of normalized) {
    const glyph = glyphFor(font, char)
    glyph.forEach((row, rowIndex) => {
      for (let columnIndex = 0; columnIndex < font.width; columnIndex += 1) {
        if ((row & (1 << (font.width - 1 - columnIndex))) === 0) continue
        ctx.fillRect(cursorX + columnIndex, y + rowIndex, 1, 1)
      }
    })
    cursorX += font.width + letterSpacing
  }
}

export function bitmapTextHeight(fontOrScale: BitmapFontName | number = 'small') {
  return resolveFont(fontOrScale).height
}
