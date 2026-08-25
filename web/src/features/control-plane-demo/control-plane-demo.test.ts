import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { shouldRefreshCalibrationDraft, syncCalibrationDraftText } from './calibration-draft'
import {
  ControlPlaneDemo,
  clearStaleWebSerialFailure,
  deviceChoiceMatchesRouteId,
  devicePickerTargets,
  formatRuntimeEventTime,
  nextFirmwareActivitySequence,
  shouldShowDeviceControlBlockFeedback,
  vinAutoCalibrationActionDisabled,
} from './components/control-plane-demo'
import { liveControlPlaneScenario } from './live-scenario'
import { controlPlaneScenario } from './mock-data'

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
  it('keeps firmware activity keys unique after Fast Refresh preserves prior entries', () => {
    expect(
      nextFirmwareActivitySequence(
        [
          { id: 'firmware-activity-idle' },
          { id: 'firmware-activity-4' },
          { id: 'firmware-activity-8' },
        ],
        1
      )
    ).toBe(9)
  })

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
  it('keeps a pre-flash native serial route valid after runtime identity becomes available', () => {
    const choice = {
      identityId: 'd0cf1308a148',
      connections: [{ target: { id: 'serial-303a-1001-D0:CF:13:08:A1:48' } }],
    }

    expect(deviceChoiceMatchesRouteId(choice, '303a-1001-D0:CF:13:08:A1:48')).toBe(true)
    expect(deviceChoiceMatchesRouteId(choice, '303a-1001-AA:BB:CC:DD:EE:FF')).toBe(false)
  })

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

describe('firmware workspace hierarchy', () => {
  it('keeps the device status strip ordered and removes transport and lease from its normal summary', () => {
    const markup = renderToStaticMarkup(
      createElement(ControlPlaneDemo, {
        scenario: controlPlaneScenario,
        allowDemoControls: true,
        devd: { enabled: false },
        webSerial: { enabled: false },
      })
    )

    const pickerIndex = markup.indexOf('aria-label="目标设备"')
    const hotplateIndex = markup.indexOf('热板')
    const pdIndex = markup.indexOf('PD')
    const workspaceIndex = markup.indexOf('class="industrial-workspace-switch"')

    expect(pickerIndex).toBeGreaterThan(-1)
    expect(hotplateIndex).toBeGreaterThan(pickerIndex)
    expect(pdIndex).toBeGreaterThan(hotplateIndex)
    expect(workspaceIndex).toBeGreaterThan(pdIndex)
    expect(markup).not.toContain('>传输<')
    expect(markup).not.toContain('>租约<')
  })

  it('treats firmware maintenance as an independent workspace without device navigation', () => {
    const markup = renderToStaticMarkup(
      createElement(ControlPlaneDemo, {
        scenario: controlPlaneScenario,
        initialView: 'update',
        allowDemoControls: true,
        devd: { enabled: false },
        webSerial: { enabled: false },
      })
    )

    expect(markup).toContain('aria-label="固件工作区"')
    expect(markup).toContain('独立烧录任务')
    expect(markup).not.toContain('aria-label="当前目标"')
    expect(markup).not.toContain('aria-label="设备工作区"')
    expect(markup).not.toContain('运行时追踪')
    expect(markup).toContain('aria-label="固件事务日志"')
    expect(markup).toContain('等待任务')
  })

  it('keeps the live firmware workspace available when its routed control device is unavailable', () => {
    const markup = renderToStaticMarkup(
      createElement(ControlPlaneDemo, {
        scenario: liveControlPlaneScenario,
        allowDemoControls: false,
        devd: { enabled: false },
        webSerial: { enabled: false },
        navigation: {
          state: {
            kind: 'device',
            deviceId: 'serial-303a-1001-D0:CF:13:08:A1:48',
            view: 'update',
          },
          variant: 'live',
          search: { demo: false },
          navigate: async () => undefined,
          blockedNavigation: null,
          onCalibrationGuardChange: () => undefined,
        },
      })
    )

    expect(markup).toContain('aria-label="固件工作区"')
    expect(markup).not.toContain('目标设备暂不可用')
  })

  it('keeps the independent firmware workspace rendered when live discovery has no device', () => {
    const markup = renderToStaticMarkup(
      createElement(ControlPlaneDemo, {
        scenario: { ...liveControlPlaneScenario, devices: [], selectedDeviceId: 'missing-device' },
        allowDemoControls: false,
        devd: { enabled: false },
        webSerial: { enabled: false },
        navigation: {
          state: {
            kind: 'device',
            deviceId: 'serial-303a-1001-D0:CF:13:08:A1:48',
            view: 'update',
          },
          variant: 'live',
          search: { demo: false },
          navigate: async () => undefined,
          blockedNavigation: null,
          onCalibrationGuardChange: () => undefined,
        },
      })
    )

    expect(markup).toContain('aria-label="固件工作区"')
    expect(markup).toContain('浏览器直接连接 ROM')
  })

  it('withholds device navigation until a control device is selected', () => {
    const markup = renderToStaticMarkup(
      createElement(ControlPlaneDemo, {
        scenario: liveControlPlaneScenario,
        allowDemoControls: false,
        devd: { enabled: false },
        webSerial: { enabled: false },
      })
    )

    expect(markup).toContain('Choose target')
    expect(markup).not.toContain('aria-label="设备工作区"')
  })
})

describe('voltage auto-calibration power gate', () => {
  const readyAction = {
    controlsBlocked: false,
    calibrationActionPending: false,
    jobRunning: false,
    modeArmed: true,
    validPpsInput: true,
  }

  it('holds automatic voltage calibration until FUSB302B confirms a performance PPS contract', () => {
    expect(
      vinAutoCalibrationActionDisabled(
        {
          pdController: 'fusb302b',
          pdContractKind: 'fixed',
          pdPerformanceGuaranteed: true,
        },
        readyAction
      )
    ).toBe(true)
    expect(
      vinAutoCalibrationActionDisabled(
        {
          pdController: 'fusb302b',
          pdContractKind: 'pps',
          pdPerformanceGuaranteed: false,
        },
        readyAction
      )
    ).toBe(true)
    expect(
      vinAutoCalibrationActionDisabled(
        {
          pdController: 'fusb302b',
          pdContractKind: 'pps',
          pdPerformanceGuaranteed: true,
        },
        readyAction
      )
    ).toBe(false)
  })

  it('preserves the established CH224Q voltage-calibration path', () => {
    expect(
      vinAutoCalibrationActionDisabled(
        { pdController: 'ch224q', pdContractKind: null, pdPerformanceGuaranteed: null },
        readyAction
      )
    ).toBe(false)
  })
})
