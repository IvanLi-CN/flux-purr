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

export const frontPanelTemperatureColors = [
  '#193b72',
  '#2f6cff',
  '#63d8ff',
  '#52e3c2',
  '#9adf61',
  '#ffd166',
  '#ff9a3c',
  '#ff6b57',
] as const

export const frontPanelDefaultThresholdsC = [0, 40, 60, 100, 150, 200, 250, 300] as const

export const frontPanelTypography = [
  {
    name: 'Dashboard Numerals',
    spec: '7-segment digits · 15×26 logical px',
    usage: 'Current temperature and preset temperature',
  },
  {
    name: 'UI Labels',
    spec: '3×5 bitmap glyphs · scale 1 / scale 2',
    usage: 'M1~M10, protocol, fan status, menu titles',
  },
  {
    name: 'Temp Unit',
    spec: 'Stacked bitmap ℃ icon',
    usage: 'Temperature unit beside dashboard numerals',
  },
] as const
