export type ChartSample = {
  x: number
  y: number
}

export function interpolateLinear(samples: ChartSample[], x: number): number | null {
  if (samples.length === 0 || !Number.isFinite(x)) return null

  for (const sample of samples) {
    if (sample.x === x) return sample.y
  }

  for (let index = 1; index < samples.length; index += 1) {
    const left = samples[index - 1]
    const right = samples[index]
    const minX = Math.min(left.x, right.x)
    const maxX = Math.max(left.x, right.x)
    if (x < minX || x > maxX || left.x === right.x) continue
    const ratio = (x - left.x) / (right.x - left.x)
    return left.y + (right.y - left.y) * ratio
  }

  return null
}

export function chartValueAtPointer(
  event: { clientX: number; clientY: number },
  svg: SVGSVGElement,
  width: number,
  height: number
) {
  const rect = svg.getBoundingClientRect()
  const scaleX = rect.width > 0 ? width / rect.width : 1
  const scaleY = rect.height > 0 ? height / rect.height : 1
  return {
    x: (event.clientX - rect.left) * scaleX,
    y: (event.clientY - rect.top) * scaleY,
  }
}

export function clampChartValue(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value))
}

export function tooltipPlacement(x: number, width: number): 'start' | 'center' | 'end' {
  if (x <= width * 0.28) return 'start'
  if (x >= width * 0.72) return 'end'
  return 'center'
}
