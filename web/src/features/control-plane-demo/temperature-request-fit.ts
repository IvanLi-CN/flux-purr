export interface TemperatureRequestObservation {
  temperatureC: number
  requestV: number
  currentMa: number
}

export interface TemperatureRequestFit {
  currentMa: number
  degree: number
  rmseV: number
  points: Array<{ temperatureC: number; requestV: number }>
}

// Display-only least squares, evaluated over the complete chart temperature range.
// Contracts are independent. No result is used for power control.
export function fitTemperatureRequestCurves(
  observations: TemperatureRequestObservation[],
  temperatureRange = { minC: 25, maxC: 220 }
): TemperatureRequestFit[] {
  const groups = new Map<number, TemperatureRequestObservation[]>()
  for (const row of observations) {
    if (![row.temperatureC, row.requestV, row.currentMa].every(Number.isFinite)) continue
    const group = groups.get(row.currentMa) ?? []
    group.push(row)
    groups.set(row.currentMa, group)
  }
  return [...groups].flatMap(([currentMa, rows]) => {
    const minT = Math.min(...rows.map((row) => row.temperatureC))
    const maxT = Math.max(...rows.map((row) => row.temperatureC))
    const distinct = new Set(rows.map((row) => row.temperatureC)).size
    if (distinct < 3 || maxT <= minT) return []
    const degree = Math.min(3, distinct - 1)
    const scaled = (temperatureC: number) => (2 * (temperatureC - minT)) / (maxT - minT) - 1
    const matrix = Array.from({ length: degree + 1 }, (_, i) =>
      Array.from({ length: degree + 2 }, (_, j) =>
        rows.reduce(
          (sum, row) =>
            sum +
            (j === degree + 1
              ? row.requestV * scaled(row.temperatureC) ** i
              : scaled(row.temperatureC) ** (i + j)),
          0
        )
      )
    )
    const coefficients = solve(matrix)
    if (!coefficients) return []
    const predict = (temperatureC: number) =>
      coefficients.reduceRight((sum, coefficient) => sum * scaled(temperatureC) + coefficient, 0)
    const rmseV = Math.sqrt(
      rows.reduce((sum, row) => sum + (predict(row.temperatureC) - row.requestV) ** 2, 0) /
        rows.length
    )
    const displayMinT = Math.min(temperatureRange.minC, minT)
    const displayMaxT = Math.max(temperatureRange.maxC, maxT)
    return [
      {
        currentMa,
        degree,
        rmseV,
        points: Array.from({ length: 81 }, (_, index) => {
          const temperatureC = displayMinT + ((displayMaxT - displayMinT) * index) / 80
          return { temperatureC, requestV: predict(temperatureC) }
        }),
      },
    ]
  })
}

function solve(matrix: number[][]): number[] | null {
  const size = matrix.length
  for (let column = 0; column < size; column += 1) {
    let pivot = column
    for (let row = column + 1; row < size; row += 1) {
      if (Math.abs(matrix[row][column]) > Math.abs(matrix[pivot][column])) pivot = row
    }
    if (Math.abs(matrix[pivot][column]) < 1e-10) return null
    ;[matrix[column], matrix[pivot]] = [matrix[pivot], matrix[column]]
    const scale = matrix[column][column]
    for (let j = column; j <= size; j += 1) matrix[column][j] /= scale
    for (let row = 0; row < size; row += 1) {
      if (row === column) continue
      const factor = matrix[row][column]
      for (let j = column; j <= size; j += 1) matrix[row][j] -= factor * matrix[column][j]
    }
  }
  const result = matrix.map((row) => row[size])
  return result.every(Number.isFinite) ? result : null
}
