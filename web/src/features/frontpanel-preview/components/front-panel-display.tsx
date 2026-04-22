import { useEffect, useMemo, useRef } from 'react'
import { cn } from '@/lib/utils'
import { drawBitmapText, measureBitmapText } from '../bitmap-font'
import { frontPanelPalette, frontPanelTemperatureColors } from '../design-tokens'
import type { FrontPanelKeyId, FrontPanelScreen, KeyGestureId } from '../types'

const LOGICAL_WIDTH = 160
const LOGICAL_HEIGHT = 50

const palette = frontPanelPalette
const temperatureColors = frontPanelTemperatureColors

type MenuIconId = Extract<FrontPanelScreen, { kind: 'menu' }>['items'][number]['id']

const menuMeta: Record<MenuIconId, { title: string }> = {
  'preset-temp': {
    title: 'TEMP SET',
  },
  'active-cooling': {
    title: 'A-COOL',
  },
  'wifi-info': {
    title: 'WIFI',
  },
  'device-info': {
    title: 'DEVICE',
  },
}

const menuIconBitmaps: Record<MenuIconId, readonly string[]> = {
  'preset-temp': [
    '0000000111000000',
    '0000001001000000',
    '0000001011000000',
    '0000001001000000',
    '0000001001000000',
    '0000001111000000',
    '0000001111000000',
    '0000001111000000',
    '0000001111000000',
    '0000110110110000',
    '0000101111010000',
    '0000101111010000',
    '0000101111010000',
    '0000100111010000',
    '0000010000100000',
    '0000001111000000',
  ],
  'active-cooling': [
    '0000001111100000',
    '0000011111100000',
    '0000011111100000',
    '0000001111000000',
    '0000001110000000',
    '0110001000000110',
    '1111000110011111',
    '1111111111111111',
    '1111111111111111',
    '1111110110000111',
    '0110000001000110',
    '0000000111000000',
    '0000000111100000',
    '0000011111100000',
    '0000011111100000',
    '0000011111100000',
  ],
  'wifi-info': [
    '0000000000000000',
    '0000111111110000',
    '0001111111111000',
    '0111000000001110',
    '1110000000000111',
    '1100011111110011',
    '0001110000111000',
    '0011000000001100',
    '0000001111000000',
    '0000011111100000',
    '0000010000100000',
    '0000000000000000',
    '0000000110000000',
    '0000000110000000',
    '0000000000000000',
    '0000000000000000',
  ],
  'device-info': [
    '0000000000000000',
    '0001001001001000',
    '0001001001001000',
    '0110000000000110',
    '0000111111110000',
    '0000111111110000',
    '0110110000110110',
    '0000110000110000',
    '0000110000110000',
    '0110110000110110',
    '0000111111110000',
    '0000111111110000',
    '0110000000000110',
    '0001001001001000',
    '0001001001001000',
    '0000000000000000',
  ],
}

function fillRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  color: string
) {
  ctx.fillStyle = color
  ctx.fillRect(x, y, width, height)
}

function fillPixelRoundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  color: string
) {
  if (width < 3 || height < 3) {
    fillRect(ctx, x, y, width, height, color)
    return
  }

  fillRect(ctx, x + 1, y, width - 2, 1, color)
  fillRect(ctx, x, y + 1, width, height - 2, color)
  fillRect(ctx, x + 1, y + height - 1, width - 2, 1, color)
}

function temperatureColor(
  value: number,
  thresholds: readonly [number, number, number, number, number, number]
) {
  for (let index = 0; index < thresholds.length - 1; index += 1) {
    if (value < thresholds[index + 1]) {
      return temperatureColors[Math.min(index, temperatureColors.length - 1)]
    }
  }
  return temperatureColors[temperatureColors.length - 1]
}

const celsiusUnitBitmap = [
  '011000000000',
  '100100111111',
  '100101110000',
  '011011000000',
  '000011000000',
  '000011000000',
  '000011000000',
  '000011000000',
  '000011000000',
  '000001111000',
  '000000111111',
] as const

function drawCelsiusUnitBitmap(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  color: string,
  scale = 1
) {
  celsiusUnitBitmap.forEach((row, rowIndex) => {
    for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
      if (row[columnIndex] !== '1') continue
      fillRect(ctx, x + columnIndex * scale, y + rowIndex * scale, scale, scale, color)
    }
  })
}

function drawTempUnitIcon(ctx: CanvasRenderingContext2D, x: number, y: number, color: string) {
  drawCelsiusUnitBitmap(ctx, x, y, color, 1)
}

const sevenSegmentMap: Record<string, Array<'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g'>> = {
  '-': ['g'],
  '0': ['a', 'b', 'c', 'd', 'e', 'f'],
  '1': ['b', 'c'],
  '2': ['a', 'b', 'g', 'e', 'd'],
  '3': ['a', 'b', 'g', 'c', 'd'],
  '4': ['f', 'g', 'b', 'c'],
  '5': ['a', 'f', 'g', 'c', 'd'],
  '6': ['a', 'f', 'g', 'e', 'c', 'd'],
  '7': ['a', 'b', 'c'],
  '8': ['a', 'b', 'c', 'd', 'e', 'f', 'g'],
  '9': ['a', 'b', 'c', 'd', 'f', 'g'],
}

function drawSevenSegmentDigit(
  ctx: CanvasRenderingContext2D,
  digit: string,
  x: number,
  y: number,
  color: string
) {
  const thickness = 3
  const width = 15
  const height = 26
  const midY = y + Math.floor((height - thickness) / 2)
  const segments = sevenSegmentMap[digit]
  if (!segments) return

  const segmentShapes = {
    a: [x + 2, y, width - 4, thickness],
    b: [x + width - thickness, y + 2, thickness, 9],
    c: [x + width - thickness, y + 15, thickness, 9],
    d: [x + 2, y + height - thickness, width - 4, thickness],
    e: [x, y + 15, thickness, 9],
    f: [x, y + 2, thickness, 9],
    g: [x + 2, midY, width - 4, thickness],
  } as const

  for (const segment of segments) {
    const [sx, sy, sw, sh] = segmentShapes[segment]
    fillRect(ctx, sx, sy, sw, sh, color)
  }
}

function drawSevenSegmentNumber(
  ctx: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  color: string
) {
  let cursorX = x
  for (const digit of text) {
    drawSevenSegmentDigit(ctx, digit, cursorX, y, color)
    cursorX += 17
  }
}

function measureSevenSegmentNumber(text: string) {
  if (!text.length) return 0
  return text.length * 17 - 2
}

const keyMaskColors: Record<KeyGestureId, string> = {
  short: palette.success,
  double: palette.accent,
  long: palette.cyan,
}

function activeKeyColor(
  activeKey: FrontPanelKeyId | null,
  target: FrontPanelKeyId,
  gesture: KeyGestureId | null
) {
  if (activeKey !== target || !gesture) return palette.text
  return keyMaskColors[gesture]
}

function drawKeyShape(ctx: CanvasRenderingContext2D, key: FrontPanelKeyId, color: string) {
  ctx.fillStyle = color
  ctx.beginPath()

  if (key === 'up') {
    ctx.moveTo(40, 7)
    ctx.lineTo(30, 19)
    ctx.lineTo(50, 19)
  } else if (key === 'down') {
    ctx.moveTo(40, 41)
    ctx.lineTo(30, 29)
    ctx.lineTo(50, 29)
  } else if (key === 'left') {
    ctx.moveTo(11, 24)
    ctx.lineTo(23, 14)
    ctx.lineTo(23, 34)
  } else if (key === 'right') {
    ctx.moveTo(69, 24)
    ctx.lineTo(57, 14)
    ctx.lineTo(57, 34)
  } else {
    ctx.arc(40, 24, 10, 0, Math.PI * 2)
  }

  ctx.closePath()
  ctx.fill()
}

function drawKeyTestScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'key-test' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  fillRect(ctx, 4, 4, 152, 42, palette.panelStrong)
  drawBitmapText(ctx, 'KEY TEST', 8, 7, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, 'SHORT=SUCCESS', 84, 8, {
    color: palette.success,
    scale: 1,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, 'DOUBLE=ACCENT', 84, 15, {
    color: palette.accent,
    scale: 1,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, 'LONG=INFO', 84, 22, {
    color: palette.cyan,
    scale: 1,
    letterSpacing: 1,
  })

  ;(['up', 'left', 'center', 'right', 'down'] as const).forEach((key) => {
    drawKeyShape(ctx, key, activeKeyColor(screen.activeKey, key, screen.activeGesture))
  })

  drawBitmapText(ctx, 'U', 39, 15, { color: palette.bg, scale: 1, letterSpacing: 1 })
  drawBitmapText(ctx, 'D', 39, 34, { color: palette.bg, scale: 1, letterSpacing: 1 })
  drawBitmapText(ctx, 'L', 17, 24, { color: palette.bg, scale: 1, letterSpacing: 1 })
  drawBitmapText(ctx, 'R', 61, 24, { color: palette.bg, scale: 1, letterSpacing: 1 })
  drawBitmapText(ctx, 'OK', 34, 24, { color: palette.bg, scale: 1, letterSpacing: 1 })

  drawBitmapText(ctx, screen.rawKeyLabel.replace('RAW ', ''), 84, 32, {
    color: palette.warning,
    scale: 1,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, screen.logicalKeyLabel.replace('LOGICAL ', '').replace('LOG ', ''), 112, 32, {
    color: palette.text,
    scale: 1,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, screen.gestureLabel, 134, 32, {
    color: screen.activeGesture ? keyMaskColors[screen.activeGesture] : palette.text,
    scale: 1,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, screen.rawMaskLabel, 84, 41, {
    color: palette.muted,
    scale: 1,
    letterSpacing: 1,
  })
}

function drawMenuIcon(
  ctx: CanvasRenderingContext2D,
  itemId: MenuIconId,
  x: number,
  y: number,
  color: string
) {
  const bitmap = menuIconBitmaps[itemId]
  for (let rowIndex = 0; rowIndex < bitmap.length; rowIndex += 1) {
    const row = bitmap[rowIndex]
    for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
      if (row[columnIndex] !== '1') continue
      fillRect(ctx, x + columnIndex, y + rowIndex, 1, 1, color)
    }
  }
}

function drawStatusLine(
  ctx: CanvasRenderingContext2D,
  y: number,
  color: string,
  label: string,
  value: string
) {
  drawBitmapText(ctx, label, 80, y, {
    color,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, value, 154, y, {
    color,
    scale: 2,
    letterSpacing: 1,
    align: 'right',
  })
}

function drawDashboardScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'dashboard' }>
) {
  const valueColor = temperatureColor(screen.targetTempC, screen.temperatureThresholdsC)
  const valueText = String(Math.round(screen.targetTempC))

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  fillRect(ctx, 4, 4, 72, 36, palette.panelStrong)
  drawSevenSegmentNumber(ctx, valueText, 8, 8, valueColor)
  drawTempUnitIcon(ctx, 60, 24, palette.text)

  fillRect(ctx, 78, 4, 78, 36, palette.panel)
  drawStatusLine(
    ctx,
    7,
    screen.heaterEnabled ? palette.success : palette.warning,
    'HEAT',
    screen.heaterEnabled ? 'ON' : 'OFF'
  )
  drawStatusLine(
    ctx,
    18,
    screen.fanEnabled ? palette.success : palette.disabled,
    'FAN',
    screen.fanEnabled ? 'RUN' : 'STOP'
  )
  drawStatusLine(ctx, 29, palette.cyan, 'MODE', 'APP')

  fillRect(ctx, 4, 42, 152, 5, palette.panel)
  fillRect(
    ctx,
    6,
    43,
    screen.heaterEnabled ? 116 : 42,
    3,
    screen.heaterEnabled ? palette.accent : palette.disabled
  )
}

function drawMenuScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'menu' }>
) {
  const selectedItem =
    screen.items.find((item) => item.id === screen.selectedItem) ?? screen.items[0]
  const selectedMeta = menuMeta[selectedItem.id]
  const titleWidth = measureBitmapText(selectedMeta.title, 2, 1)

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  fillRect(ctx, 4, 4, 152, 24, palette.panelStrong)
  screen.items.forEach((item, index) => {
    const x = 6 + index * 38
    const active = item.id === screen.selectedItem

    if (index > 0) fillRect(ctx, x - 2, 8, 1, 16, palette.border)
    if (active) fillPixelRoundedRect(ctx, x + 4, 6, 26, 20, palette.accent)
    drawMenuIcon(ctx, item.id, x + 9, 8, active ? palette.bg : palette.text)
  })

  fillRect(ctx, 4, 30, 152, 16, palette.panel)
  drawMenuIcon(ctx, selectedItem.id, 8, 30, palette.warning)
  drawBitmapText(ctx, selectedMeta.title, Math.round((160 - titleWidth) / 2), 34, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
}

const presetMarkerGlyph = ['10001', '11011', '10101', '10001', '10001'] as const

function drawPresetSlotLabel(
  ctx: CanvasRenderingContext2D,
  index: number,
  x: number,
  y: number,
  color: string
) {
  presetMarkerGlyph.forEach((row, rowIndex) => {
    for (let columnIndex = 0; columnIndex < row.length; columnIndex += 1) {
      if (row[columnIndex] !== '1') continue
      fillRect(ctx, x + columnIndex, y + rowIndex, 1, 1, color)
    }
  })

  drawBitmapText(ctx, String(index + 1), x + 6, y, {
    color,
    scale: 1,
    letterSpacing: 1,
  })
}

function drawPresetTempScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'preset-temp' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  const selectedPreset = screen.presetsC[screen.selectedPresetIndex] ?? null
  const displayText = selectedPreset == null ? '---' : String(Math.round(selectedPreset))
  const valueColor =
    selectedPreset == null
      ? palette.disabled
      : temperatureColor(selectedPreset, screen.temperatureThresholdsC)
  const unitColor = selectedPreset == null ? palette.disabled : palette.text

  for (let index = 0; index < Math.min(screen.presetsC.length, 10); index += 1) {
    const isSelected = index === screen.selectedPresetIndex
    const isEnabled = screen.presetsC[index] != null
    const color = isSelected ? palette.accent : isEnabled ? palette.text : palette.disabled
    drawPresetSlotLabel(ctx, index, 2 + index * 16, 1, color)
  }

  const digitsWidth = measureSevenSegmentNumber(displayText)
  const digitsY = 18
  const digitHeight = 26
  const unitGap = 3
  const unitWidth = celsiusUnitBitmap[0].length
  const unitHeight = 11
  const blockWidth = digitsWidth + unitGap + unitWidth
  const digitsX = Math.round((LOGICAL_WIDTH - blockWidth) / 2)
  const unitX = digitsX + digitsWidth + unitGap
  const unitY = digitsY + digitHeight - unitHeight

  drawSevenSegmentNumber(ctx, displayText, digitsX, digitsY, valueColor)
  drawCelsiusUnitBitmap(ctx, unitX, unitY, unitColor, 1)
}

function drawActiveCoolingScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'active-cooling' }>
) {
  const fanText =
    !screen.enabled || screen.mode === 'off' ? 'OFF' : screen.mode === 'smart' ? 'AUTO' : 'ON'
  const fanColor =
    fanText === 'AUTO' ? palette.cyan : fanText === 'ON' ? palette.success : palette.warning

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  drawBitmapText(ctx, 'A-COOL', 8, 6, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, screen.enabled ? 'ON' : 'OFF', 152, 6, {
    color: screen.enabled ? palette.success : palette.warning,
    scale: 2,
    align: 'right',
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `MODE ${screen.mode.toUpperCase()}`, 8, 20, {
    color: palette.cyan,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `FAN ${fanText}`, 8, 33, {
    color: fanColor,
    scale: 2,
    letterSpacing: 1,
  })
}

function fitBitmapText(
  text: string,
  maxWidth: number,
  scale = 1,
  letterSpacing = 1,
  trailing = '...'
) {
  if (measureBitmapText(text, scale, letterSpacing) <= maxWidth) return text

  let truncated = text
  while (truncated.length > 0) {
    const next = `${truncated.slice(0, -1)}${trailing}`
    if (measureBitmapText(next, scale, letterSpacing) <= maxWidth) return next
    truncated = truncated.slice(0, -1)
  }

  return trailing
}

function drawWifiInfoScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'wifi-info' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  drawBitmapText(ctx, fitBitmapText(`SSID ${screen.ssid}`, 144, 2, 1), 8, 6, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `RSSI ${screen.rssiDbm}DBM`, 8, 19, {
    color: palette.cyan,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `IP ${screen.ipAddress}`, 8, 32, {
    color: palette.warning,
    scale: 2,
    letterSpacing: 1,
  })
}

function drawDeviceInfoScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'device-info' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)
  drawBitmapText(ctx, `BOARD ${screen.board}`, 8, 6, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `FW ${screen.firmwareVersion}`, 8, 19, {
    color: palette.warning,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `ID ${screen.serial}`, 8, 32, {
    color: palette.cyan,
    scale: 2,
    letterSpacing: 1,
  })
}

function drawFrontPanel(ctx: CanvasRenderingContext2D, screen: FrontPanelScreen) {
  ctx.clearRect(0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT)
  ctx.imageSmoothingEnabled = false

  switch (screen.kind) {
    case 'key-test':
      drawKeyTestScreen(ctx, screen)
      return
    case 'dashboard':
      drawDashboardScreen(ctx, screen)
      return
    case 'menu':
      drawMenuScreen(ctx, screen)
      return
    case 'preset-temp':
      drawPresetTempScreen(ctx, screen)
      return
    case 'active-cooling':
      drawActiveCoolingScreen(ctx, screen)
      return
    case 'wifi-info':
      drawWifiInfoScreen(ctx, screen)
      return
    case 'device-info':
      drawDeviceInfoScreen(ctx, screen)
      return
  }
}

function ariaLabel(screen: FrontPanelScreen) {
  switch (screen.kind) {
    case 'key-test':
      return `front panel key test ${screen.gestureLabel}`
    case 'dashboard':
      return `front panel dashboard ${screen.targetTempC} degrees`
    case 'menu':
      return 'front panel menu'
    case 'preset-temp': {
      const preset = screen.presetsC[screen.selectedPresetIndex]
      return preset == null
        ? `front panel preset ${screen.selectedPresetIndex + 1} disabled`
        : `front panel preset ${screen.selectedPresetIndex + 1} ${preset} degrees`
    }
    case 'active-cooling':
      return `front panel active cooling ${screen.enabled ? 'enabled' : 'disabled'}`
    case 'wifi-info':
      return `front panel wifi info ${screen.ssid}`
    case 'device-info':
      return `front panel device info ${screen.board}`
  }
}

export interface FrontPanelDisplayProps {
  screen: FrontPanelScreen
  scale?: number
  className?: string
  frameClassName?: string
  showFrame?: boolean
  showMeta?: boolean
}

export function FrontPanelDisplay({
  screen,
  scale = 6,
  className,
  frameClassName,
  showFrame = true,
  showMeta = true,
}: FrontPanelDisplayProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const label = useMemo(() => ariaLabel(screen), [screen])
  const renderScale = Math.max(1, Math.floor(scale))

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    canvas.width = LOGICAL_WIDTH
    canvas.height = LOGICAL_HEIGHT
    const context = canvas.getContext('2d')
    if (!context) return
    context.imageSmoothingEnabled = false
    drawFrontPanel(context, screen)
  }, [screen])

  return (
    <div data-testid="front-panel-display" className={cn('inline-flex flex-col gap-3', className)}>
      <div
        className={cn(
          showFrame
            ? 'rounded-[28px] border border-slate-700/80 bg-slate-950 p-4 shadow-[0_30px_80px_rgba(2,6,23,0.55)]'
            : 'p-0',
          frameClassName
        )}
      >
        <canvas
          ref={canvasRef}
          width={LOGICAL_WIDTH}
          height={LOGICAL_HEIGHT}
          role="img"
          aria-label={label}
          data-screen-kind={screen.kind}
          className="block bg-[#08111f]"
          style={{
            width: `${LOGICAL_WIDTH * renderScale}px`,
            height: `${LOGICAL_HEIGHT * renderScale}px`,
            imageRendering: 'pixelated',
          }}
        />
      </div>
      {showMeta ? (
        <div className="space-y-1 text-center">
          <p className="text-sm font-semibold text-slate-100">{screen.title}</p>
          {screen.subtitle ? <p className="text-xs text-slate-400">{screen.subtitle}</p> : null}
        </div>
      ) : null}
    </div>
  )
}
