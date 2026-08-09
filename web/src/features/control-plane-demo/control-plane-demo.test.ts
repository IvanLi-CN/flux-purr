import { describe, expect, it } from 'vitest'
import { shouldRefreshCalibrationDraft, syncCalibrationDraftText } from './calibration-draft'
import {
  clearStaleWebSerialFailure,
  devicePickerTargets,
  formatRuntimeEventTime,
  shouldShowDeviceControlBlockFeedback,
} from './components/control-plane-demo'

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

describe('Web Serial feedback settlement', () => {
  it('gives live operator events a wall-clock time instead of a demo fixture time', () => {
    expect(formatRuntimeEventTime(new Date(2026, 0, 1, 18, 4, 7))).toBe('18:04:07')
  })

  it('replaces a previous Web Serial failure after the port connects', () => {
    expect(
      clearStaleWebSerialFailure({
        title: 'Web Serial unavailable',
        detail: 'Timed out waiting for a matching USB JSONL response.',
        tone: 'warning',
      })
    ).toEqual({
      title: 'Web Serial connected',
      detail: 'Browser direct USB JSONL control is active.',
      tone: 'success',
    })
  })

  it('does not erase feedback from a different completed action', () => {
    const current = {
      title: 'LAN 设备已连接',
      detail: '设备已取得控制 lease。',
      tone: 'success' as const,
    }

    expect(clearStaleWebSerialFailure(current)).toBe(current)
  })
})

describe('live transport feedback boundary', () => {
  it('keeps remembered Web Serial routes but excludes the no-target placeholder from the picker', () => {
    expect(
      devicePickerTargets([
        { id: 'live-no-target', transport: 'serial' },
        { id: 'web-serial-a0f262f20d6c', transport: 'serial' },
        { id: 'devd-stale', transport: 'devd', connectionAvailable: false },
      ])
    ).toEqual([{ id: 'web-serial-a0f262f20d6c', transport: 'serial' }])
  })

  it('does not surface DEVD bootstrap state as a device-control failure', () => {
    expect(
      shouldShowDeviceControlBlockFeedback({
        transport: 'devd',
        baseUrl: 'devd://unavailable',
        connectionAvailable: false,
      })
    ).toBe(false)
  })

  it('continues to surface a blocked verified device', () => {
    expect(
      shouldShowDeviceControlBlockFeedback({
        transport: 'devd',
        baseUrl: 'http://127.0.0.1:30080',
        connectionAvailable: true,
      })
    ).toBe(true)
  })
})
