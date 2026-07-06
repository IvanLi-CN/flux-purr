export function resolveCalibrationSliderValue(valueText: string, min: number, max: number) {
  const trimmedValueText = valueText.trim()
  if (trimmedValueText === '') {
    return min
  }

  const numericValue = Number(valueText)
  if (!Number.isFinite(numericValue)) {
    return min
  }

  return Math.min(Math.max(Math.round(numericValue), min), max)
}

export function validateCalibrationSliderText(valueText: string, min: number, max: number) {
  const trimmedValueText = valueText.trim()
  if (trimmedValueText === '') {
    return null
  }

  const numericValue = Number(valueText)
  if (!Number.isFinite(numericValue)) {
    return '请输入有效数值。'
  }
  if (numericValue < min || numericValue > max) {
    return `请输入 ${min}-${max} 范围内的数值。`
  }

  return null
}
