import { describe, expect, it } from 'vitest'

import {
  applyLocalCalibrationRequest,
  createDefaultCalibrationState,
} from './components/control-plane-demo'

describe('ADC calibration A/B slots', () => {
  it('updates VIN fitted suggestions from samples without overwriting A/B slots', () => {
    const initial = createDefaultCalibrationState()
    const withSlotA = applyLocalCalibrationRequest(initial, {
      op: 'set_slot_fit',
      channel: 'vin_adc',
      slot: 'a',
      fit: { gain: 0.975, offsetMv: 147.3 },
    })

    const next = applyLocalCalibrationRequest(withSlotA, {
      op: 'capture',
      channel: 'vin_adc',
      observedMv: 873,
      expectedMv: 1000,
      referenceVinMv: 1000,
    })

    expect(next.vinAdc.fittedFit).toEqual({
      gain: 1,
      offsetMv: 127,
      sampleCount: 1,
    })
    expect(next.vinAdc.slots.a).toEqual({ gain: 0.975, offsetMv: 147.3 })
    expect(next.vinAdc.slots.b).toEqual({ gain: 1, offsetMv: 0 })
    expect(next.vinAdc.activeSlot).toBe('a')
  })

  it('switches active VIN slots without changing fitted suggestions or slot values', () => {
    const initial = createDefaultCalibrationState()
    const withSample = applyLocalCalibrationRequest(initial, {
      op: 'capture',
      channel: 'vin_adc',
      observedMv: 900,
      expectedMv: 1200,
      referenceVinMv: 1200,
    })
    const withSlotB = applyLocalCalibrationRequest(withSample, {
      op: 'set_slot_fit',
      channel: 'vin_adc',
      slot: 'b',
      fit: { gain: 1.02, offsetMv: -18.5 },
    })

    const next = applyLocalCalibrationRequest(withSlotB, {
      op: 'set_active_slot',
      channel: 'vin_adc',
      slot: 'b',
    })

    expect(next.vinAdc.activeSlot).toBe('b')
    expect(next.vinAdc.fittedFit).toEqual({ gain: 1, offsetMv: 300, sampleCount: 1 })
    expect(next.vinAdc.slots.b).toEqual({ gain: 1.02, offsetMv: -18.5 })
  })
})
