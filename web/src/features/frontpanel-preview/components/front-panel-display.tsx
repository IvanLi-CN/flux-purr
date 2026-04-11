import { useEffect, useMemo, useRef } from 'react'
import { cn } from '@/lib/utils'
import { drawBitmapText, measureBitmapText } from '../bitmap-font'
import type { FanState, FrontPanelScreen, PowerProtocol } from '../types'

const LOGICAL_WIDTH = 160
const LOGICAL_HEIGHT = 50

const palette = {
  bg: '#08111f',
  panel: '#122036',
  panelStrong: '#1b2a43',
  border: '#2a3d5d',
  text: '#f7fbff',
  muted: '#8ea3c6',
  accent: '#ff9a3c',
  accentSoft: '#4e2e18',
  success: '#40d9a1',
  warning: '#ffd166',
  cyan: '#63d8ff',
} as const

const temperatureColors = ['#63d8ff', '#52e3c2', '#9adf61', '#ffd166', '#ff9a3c'] as const
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

function fanLabel(fanState: FanState) {
  if (fanState === 'auto') return { text: 'AUTO', color: palette.cyan }
  if (fanState === 'off') return { text: 'OFF', color: palette.warning }
  return { text: 'ON', color: palette.success }
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

function drawBorder(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  color: string
) {
  fillRect(ctx, x, y, width, 1, color)
  fillRect(ctx, x, y + height - 1, width, 1, color)
  fillRect(ctx, x, y, 1, height, color)
  fillRect(ctx, x + width - 1, y, 1, height, color)
}

function drawPanel(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  fill: string
) {
  fillRect(ctx, x, y, width, height, fill)
  drawBorder(ctx, x, y, width, height, palette.border)
}

function splitTemperature(tempC: number) {
  const fixed = tempC.toFixed(1)
  const [integerPart, decimalPart] = fixed.split('.')
  return {
    integerPart,
    decimalPart: decimalPart ?? '0',
  }
}

function formatTargetTemperature(tempC: number) {
  return Number.isInteger(tempC) ? String(tempC) : tempC.toFixed(1)
}

function formatVoltageValue(protocol: PowerProtocol, voltage: number) {
  if (protocol === 'PPS') return `${voltage.toFixed(2)}V`
  if (Number.isInteger(voltage)) return `${Math.round(voltage)}V`
  return `${voltage.toFixed(1)}V`
}

function drawTempUnitIcon(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  color: string,
  scale = 1
) {
  drawBitmapText(ctx, '°', x + scale, y, {
    color,
    scale,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, 'C', x, y + 5 * scale, {
    color,
    scale,
    letterSpacing: 1,
  })
}

const sevenSegmentMap: Record<string, Array<'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g'>> = {
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

function temperatureColor(
  currentTempC: number,
  thresholds: readonly [number, number, number, number, number, number]
) {
  for (let index = 0; index < thresholds.length - 1; index += 1) {
    if (currentTempC < thresholds[index + 1]) {
      return temperatureColors[Math.min(index, temperatureColors.length - 1)]
    }
  }
  return temperatureColors[temperatureColors.length - 1]
}

function drawRightInfoLine(
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

function drawHomeScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'home' }>
) {
  const fan = fanLabel(screen.fanState)
  const pwmWidth = Math.max(18, Math.round((screen.pwmPercent / 100) * 148))
  const currentTempColor = temperatureColor(screen.currentTempC, screen.temperatureThresholdsC)
  const currentTemperature = splitTemperature(screen.currentTempC)

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  drawPanel(ctx, 4, 4, 72, 36, palette.panelStrong)
  drawSevenSegmentNumber(ctx, currentTemperature.integerPart, 8, 8, currentTempColor)
  drawBitmapText(ctx, currentTemperature.decimalPart, 65, 8, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawTempUnitIcon(ctx, 66, 21, palette.text, 1)

  drawPanel(ctx, 78, 4, 78, 36, palette.panel)
  drawRightInfoLine(ctx, 7, palette.warning, 'SET', formatTargetTemperature(screen.targetTempC))
  drawRightInfoLine(
    ctx,
    18,
    palette.cyan,
    screen.protocol,
    formatVoltageValue(screen.protocol, screen.voltage)
  )
  drawRightInfoLine(ctx, 29, fan.color, 'FAN', fan.text)

  drawPanel(ctx, 4, 42, 152, 5, palette.panel)
  fillRect(ctx, 6, 43, pwmWidth, 3, palette.accent)
}

function drawMenuScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'menu' }>
) {
  const selectedIndex = Math.max(
    0,
    screen.items.findIndex((item) => item.id === screen.selectedItem)
  )
  const selectedItem = screen.items[selectedIndex] ?? screen.items[0]
  const selectedMeta = menuMeta[selectedItem.id]
  const titleWidth = measureBitmapText(selectedMeta.title, 2, 1)

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  drawPanel(ctx, 4, 4, 152, 24, palette.panelStrong)
  screen.items.forEach((item, index) => {
    const x = 6 + index * 38
    const active = item.id === screen.selectedItem
    if (index > 0) fillRect(ctx, x - 2, 8, 1, 16, palette.border)
    if (active) fillRect(ctx, x + 4, 8, 26, 16, palette.accent)
    drawMenuIcon(ctx, item.id, x + 9, 8, active ? palette.bg : palette.text)
  })

  drawPanel(ctx, 4, 30, 152, 16, palette.panel)
  drawMenuIcon(ctx, selectedItem.id, 8, 30, palette.warning)
  drawBitmapText(ctx, selectedMeta.title, Math.round((160 - titleWidth) / 2), 34, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
}

function drawPresetTempScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'preset-temp' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  drawBitmapText(ctx, 'SET TEMP', 8, 6, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `STEP ${screen.stepC}C`, 152, 6, {
    color: palette.muted,
    scale: 2,
    align: 'right',
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `${screen.presetTempC}C`, LOGICAL_WIDTH / 2, 19, {
    color: palette.warning,
    scale: 5,
    align: 'center',
    letterSpacing: 1,
  })
}

function drawActiveCoolingScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'active-cooling' }>
) {
  const fan = fanLabel(screen.fanState)
  const stateText = screen.enabled ? 'ON' : 'OFF'
  const stateColor = screen.enabled ? palette.success : palette.warning

  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  drawBitmapText(ctx, 'A-COOL', 8, 6, {
    color: palette.text,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, stateText, 152, 6, {
    color: stateColor,
    scale: 2,
    align: 'right',
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `MODE ${screen.mode.toUpperCase()}`, 8, 20, {
    color: palette.cyan,
    scale: 2,
    letterSpacing: 1,
  })
  drawBitmapText(ctx, `FAN ${fan.text}`, 8, 33, {
    color: fan.color,
    scale: 2,
    letterSpacing: 1,
  })
}

function drawWifiInfoScreen(
  ctx: CanvasRenderingContext2D,
  screen: Extract<FrontPanelScreen, { kind: 'wifi-info' }>
) {
  fillRect(ctx, 0, 0, LOGICAL_WIDTH, LOGICAL_HEIGHT, palette.bg)

  drawBitmapText(ctx, `SSID ${screen.ssid}`, 8, 6, {
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
    case 'home':
      drawHomeScreen(ctx, screen)
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
    case 'home':
      return `front panel home screen ${screen.currentTempC} degrees`
    case 'menu':
      return `front panel preferences menu`
    case 'preset-temp':
      return `front panel preset temperature ${screen.presetTempC} degrees`
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
}

export function FrontPanelDisplay({
  screen,
  scale = 6,
  className,
  frameClassName,
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
          'rounded-[28px] border border-slate-700/80 bg-slate-950 p-4 shadow-[0_30px_80px_rgba(2,6,23,0.55)]',
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
      <div className="space-y-1 text-center">
        <p className="text-sm font-semibold text-slate-100">{screen.title}</p>
        {screen.subtitle ? <p className="text-xs text-slate-400">{screen.subtitle}</p> : null}
      </div>
    </div>
  )
}
