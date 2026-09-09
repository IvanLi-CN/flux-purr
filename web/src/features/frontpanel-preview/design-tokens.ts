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
  '#f7fbff',
  '#427dff',
  '#3aeff7',
  '#52f36b',
  '#c5ef4a',
  '#ffb23a',
  '#ff5542',
  '#ff4da5',
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
    spec: '4×6 small / 5×8 mid bitmap glyphs; FAN CTRL uses 6×10 labels / 8×13 title',
    usage: 'M1~M10, protocol, fan status, menu titles; high-legibility fan policy editor',
  },
  {
    name: 'Temp Unit',
    spec: 'Stacked bitmap ℃ icon',
    usage: 'Temperature unit beside dashboard numerals',
  },
] as const
