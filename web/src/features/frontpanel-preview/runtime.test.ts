import { describe, expect, it } from 'vitest'

import { createFrontPanelRuntimeState, tickFrontPanelRuntime } from './runtime'

describe('front panel fan policy preview', () => {
  it('gates heating fan pulses on live heater output', () => {
    const state = createFrontPanelRuntimeState()

    const armedButIdle = tickFrontPanelRuntime(
      {
        ...state,
        currentTempC: 110,
        heaterEnabled: true,
        heaterOutputPercent: 0,
      },
      0
    )

    expect(armedButIdle.fanRuntimeEnabled).toBe(false)
    expect(armedButIdle.fanDisplayState).toBe('auto')
  })

  it('doubles the heating pulse window without changing cooling-disabled pulses', () => {
    const state = createFrontPanelRuntimeState()

    const heatingOn = tickFrontPanelRuntime(
      {
        ...state,
        elapsedMs: 199,
        currentTempC: 110,
        heaterEnabled: true,
        heaterOutputPercent: 18,
      },
      0
    )
    const heatingOff = tickFrontPanelRuntime(
      {
        ...state,
        elapsedMs: 200,
        currentTempC: 110,
        heaterEnabled: true,
        heaterOutputPercent: 18,
      },
      0
    )
    const coolingDisabledOff = tickFrontPanelRuntime(
      {
        ...state,
        activeCoolingEnabled: false,
        elapsedMs: 100,
        currentTempC: 110,
      },
      0
    )

    expect(heatingOn.fanRuntimeEnabled).toBe(true)
    expect(heatingOff.fanRuntimeEnabled).toBe(false)
    expect(coolingDisabledOff.fanRuntimeEnabled).toBe(false)
  })
})
