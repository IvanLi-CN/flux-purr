export function rtdAdcMvForTemperature(tempC: number) {
  const resistance =
    tempC >= 0
      ? 1000 * (1 + 3.9083e-3 * tempC - 5.775e-7 * tempC * tempC)
      : 1000 *
        (1 +
          3.9083e-3 * tempC -
          5.775e-7 * tempC * tempC -
          4.183e-12 * (tempC - 100) * tempC * tempC * tempC)
  return Math.round((3000 * resistance) / (2490 + resistance))
}

export function rtdTemperatureForAdcMv(targetMv: number) {
  let low = -200
  let high = 850

  for (let index = 0; index < 30; index += 1) {
    const mid = (low + high) / 2
    const midMv = rtdAdcMvForTemperature(mid)
    if (midMv < targetMv) {
      low = mid
    } else {
      high = mid
    }
  }

  return (low + high) / 2
}
