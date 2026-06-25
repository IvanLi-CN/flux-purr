import { describe, expect, it } from 'vitest'
import { shouldRefreshCalibrationDraft, syncCalibrationDraftText } from './calibration-draft'

describe('calibration draft synchronization', () => {
  it('initializes the draft from the first live runtime value', () => {
    const previousValueRef = { current: null as number | null }

    const shouldRefresh = shouldRefreshCalibrationDraft('', 913, previousValueRef)

    expect(shouldRefresh).toBe(true)
    expect(previousValueRef.current).toBe(913)
  })

  it('preserves a user-edited draft while live polling repeats the same value', () => {
    const previousValueRef = { current: 913 as number | null }

    const shouldRefresh = shouldRefreshCalibrationDraft('950', 913, previousValueRef)

    expect(shouldRefresh).toBe(false)
    expect(previousValueRef.current).toBe(913)
  })

  it('refreshes the draft when firmware acknowledges a new live target value', () => {
    const previousValueRef = { current: 913 as number | null }

    const shouldRefresh = shouldRefreshCalibrationDraft('950', 950, previousValueRef)

    expect(shouldRefresh).toBe(true)
    expect(previousValueRef.current).toBe(950)
  })

  it('seeds an empty draft from live raw ADC before a target is acknowledged', () => {
    const previousValueRef = { current: null as number | null }

    const nextDraft = syncCalibrationDraftText('', null, 913, previousValueRef)

    expect(nextDraft).toBe('913')
    expect(previousValueRef.current).toBeNull()
  })

  it('preserves a user-edited draft while raw ADC jitters without an acknowledged target', () => {
    const previousValueRef = { current: null as number | null }

    const nextDraft = syncCalibrationDraftText('950', null, 915, previousValueRef)

    expect(nextDraft).toBe('950')
    expect(previousValueRef.current).toBeNull()
  })
})
