export type FrontPanelTheme = 'light' | 'dark'

export type FrontPanelPalette = {
  bg: string
  dashboardBg: string
  panel: string
  panelStrong: string
  border: string
  text: string
  muted: string
  disabled: string
  accent: string
  accentSoft: string
  success: string
  warning: string
  cyan: string
}

function rgb565ToCss(red: number, green: number, blue: number) {
  const expand = (value: number, max: number) => Math.round((value * 255) / max)
  return `#${[expand(red, 31), expand(green, 63), expand(blue, 31)]
    .map((value) => value.toString(16).padStart(2, '0'))
    .join('')}`
}

export const frontPanelPalette = {
  bg: '#08111f',
  panel: '#122036',
  panelStrong: '#1b2a43',
  border: '#2a3d5d',
  text: '#f7fbff',
  muted: '#8ea3c6',
  disabled: '#5b6c88',
  accent: '#ff9a3c',
  accentSoft: '#4e2e18',
  success: '#40d9a1',
  warning: '#ffd166',
  cyan: '#63d8ff',
} as const

const lightFrontPanelPalette: FrontPanelPalette = {
  bg: rgb565ToCss(29, 59, 29),
  dashboardBg: '#ffffff',
  panel: '#ffffff',
  panelStrong: '#ffffff',
  border: rgb565ToCss(17, 35, 17),
  text: rgb565ToCss(2, 8, 11),
  muted: rgb565ToCss(8, 20, 18),
  disabled: rgb565ToCss(14, 30, 19),
  accent: rgb565ToCss(20, 12, 0),
  accentSoft: rgb565ToCss(26, 44, 24),
  success: rgb565ToCss(0, 24, 8),
  warning: rgb565ToCss(20, 7, 2),
  cyan: rgb565ToCss(0, 18, 23),
}

export const frontPanelThemePalettes: Record<FrontPanelTheme, FrontPanelPalette> = {
  dark: { ...frontPanelPalette, dashboardBg: frontPanelPalette.bg },
  light: lightFrontPanelPalette,
}

// These values mirror the firmware's dashboard palettes after RGB565 expansion.
export const frontPanelTemperatureColors = [
  rgb565ToCss(28, 59, 30),
  rgb565ToCss(18, 49, 31),
  rgb565ToCss(12, 57, 31),
  rgb565ToCss(16, 59, 21),
  rgb565ToCss(26, 61, 19),
  rgb565ToCss(31, 52, 9),
  rgb565ToCss(31, 36, 7),
  rgb565ToCss(30, 28, 22),
] as const

export const frontPanelLightTemperatureColors = [
  rgb565ToCss(4, 19, 17),
  rgb565ToCss(4, 23, 20),
  rgb565ToCss(0, 30, 17),
  rgb565ToCss(2, 30, 9),
  rgb565ToCss(11, 27, 0),
  rgb565ToCss(19, 23, 0),
  rgb565ToCss(22, 20, 1),
  rgb565ToCss(15, 16, 19),
] as const

export const frontPanelTemperatureColorsByTheme: Record<FrontPanelTheme, readonly string[]> = {
  dark: frontPanelTemperatureColors,
  light: frontPanelLightTemperatureColors,
}

export function darkenRgb565Color(color: string) {
  const channels = color.match(/^#([0-9a-f]{6})$/i)
  if (!channels) return color

  const [red, green, blue] = [0, 2, 4].map((offset) =>
    Number.parseInt(channels[1].slice(offset, offset + 2), 16)
  )
  const quantize = (value: number, max: number) => Math.round((value * max) / 255)
  const red565 = Math.max(0, quantize(red, 31) - 4)
  const green565 = Math.max(0, quantize(green, 63) - 4)
  const blue565 = Math.max(0, quantize(blue, 31) - 4)
  return rgb565ToCss(red565, green565, blue565)
}

export const frontPanelDefaultThresholdsC = [0, 40, 60, 100, 150, 200, 250, 300] as const

export const frontPanelTypography = [
  {
    name: 'Dashboard Numerals',
    spec: '7-segment digits · 15×26 logical px',
    usage: 'Current temperature and preset temperature',
  },
  {
    name: 'UI Labels',
    spec: 'Existing screens retain their current bitmap glyphs; FAN CTRL uses 6×10 labels / 8×13 title',
    usage: 'M1~M10, protocol, fan status, menu titles; high-legibility fan policy editor',
  },
  {
    name: 'Temp Unit',
    spec: 'Stacked bitmap ℃ icon',
    usage: 'Temperature unit beside dashboard numerals',
  },
] as const
