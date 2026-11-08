const RTD_DIVIDER_NOMINAL_SUPPLY_MV = 3328

export function rtdAdcMvForTemperature(
  tempC: number,
  dividerSupplyMv = RTD_DIVIDER_NOMINAL_SUPPLY_MV
) {
  return Math.round(rtdDividerMv(tempC, dividerSupplyMv))
}

function rtdDividerMv(tempC: number, dividerSupplyMv: number) {
  const resistance =
    tempC >= 0
      ? 1000 * (1 + 3.9083e-3 * tempC - 5.775e-7 * tempC * tempC)
      : 1000 *
        (1 +
          3.9083e-3 * tempC -
          5.775e-7 * tempC * tempC -
          4.183e-12 * (tempC - 100) * tempC * tempC * tempC)
  return (dividerSupplyMv * resistance) / (2490 + resistance)
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

// Raw PT1000 theory, independent of saved user calibration and trace temperatures.
export function theoreticalRtdTemperatureForAdcMv(
  rawMv: number,
  dividerSupplyMv?: number
): number | null {
  if (
    !Number.isFinite(rawMv) ||
    !Number.isFinite(dividerSupplyMv) ||
    !dividerSupplyMv ||
    dividerSupplyMv < 3000 ||
    dividerSupplyMv > 3600 ||
    rawMv <= 0 ||
    rawMv >= dividerSupplyMv ||
    rawMv < rtdDividerMv(-200, dividerSupplyMv) ||
    rawMv > rtdDividerMv(850, dividerSupplyMv)
  )
    return null
  let low = -200
  let high = 850
  for (let index = 0; index < 40; index += 1) {
    const mid = (low + high) / 2
    if (rtdDividerMv(mid, dividerSupplyMv) < rawMv) low = mid
    else high = mid
  }
  return (low + high) / 2
}
