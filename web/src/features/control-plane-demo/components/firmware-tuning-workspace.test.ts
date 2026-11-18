import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { ControlPlaneClientError } from '../transport-client'
import {
  buildTuningStartCommand,
  compactIdentifier,
  FirmwareTuningWorkspace,
  messageOf,
  resolveTuningPowerClass,
  shouldPollTuningRead,
  validateCandidate,
  visibleTerminalDisposition,
} from './firmware-tuning-workspace'

describe('firmware tuning workspace', () => {
  it('preserves write lease failures and distinguishes failed writes from reads', () => {
    const lease = new ControlPlaneClientError(
      '设备控制租约正在恢复，请稍后再执行固件热控调优。',
      'devd_lease_unavailable',
      true,
      undefined,
      { category: 'transport_unavailable', operation: 'thermal_tuning_write' }
    )
    expect(messageOf(lease, 'write')).toBe(lease.message)
    const transport = new ControlPlaneClientError(
      'HTTP request failed',
      'lan_transport_unavailable',
      true,
      undefined,
      { category: 'transport_unavailable' }
    )
    expect(messageOf(transport, 'write')).toContain('操作未完成')
    expect(messageOf(transport, 'write')).not.toContain('读取')
    expect(messageOf(transport)).toContain('读取暂时失败')
    expect(
      messageOf(
        new ControlPlaneClientError('Another LAN client owns the lease.', 'lease_busy', false),
        'write'
      )
    ).toContain('设备写入租约暂被占用')
  })
  it('keeps full machine identifiers available while compacting the default row', () => {
    const value = 'sha256:d0c6ccbd8efc209d8febcf9dfeff4f3ba1cfa2702e3711840ebec9ec'
    expect(compactIdentifier(value)).toBe('sha256:d0c6c…0ebec9ec')
    expect(compactIdentifier(value)).not.toBe(value)
    expect(compactIdentifier('short-id')).toBe('short-id')
  })

  it('uses the firmware-resolved PPS bank instead of guessing pps5a', () => {
    expect(resolveTuningPowerClass('pps3a', null)).toBe('pps3a')
    expect(resolveTuningPowerClass('pps5a', 'pps3a')).toBe('pps5a')
    expect(resolveTuningPowerClass(undefined, null)).toBeNull()
  })

  it('does not present firmware compatibility running as an active tuning run', () => {
    expect(
      visibleTerminalDisposition({ armed: false, runId: null, terminalDisposition: 'running' })
    ).toBeUndefined()
    expect(
      visibleTerminalDisposition({ armed: true, runId: 'run-1', terminalDisposition: 'running' })
    ).toBe('running')
  })

  it('keeps unsupported firmware controls disabled', () => {
    const html = renderToStaticMarkup(
      createElement(FirmwareTuningWorkspace, {
        supported: false,
        deviceId: 'fixture',
        transport: 'mock',
        readStatus: async () => {
          throw new Error('unexpected read')
        },
        sendCommand: async () => {
          throw new Error('unexpected command')
        },
      })
    )
    expect(html).toContain('当前设备固件不支持调优事件协议')
    expect(html).toContain('固件热控调优')
    expect(html).toContain('开始调优')
    expect(html).toContain('导出报告')
  })

  it('does not start a second event read while the previous read is in flight', () => {
    const base = {
      supported: true,
      journalReady: true,
      pending: false,
      reading: false,
      gap: false,
      complete: false,
      identityVerified: true,
      failedAttempts: 0,
    }
    expect(shouldPollTuningRead(base)).toBe(true)
    expect(shouldPollTuningRead({ ...base, reading: true })).toBe(false)
  })

  it.each([
    'pps3a',
    'pps5a',
  ] as const)('checks the canonical device profile hash for %s and isolates its start bank', async (powerClass) => {
    const settingsFields = [
      'tempFilterAlphaPermille',
      'warmupReenterCentiC',
      'holdEntryCentiC',
      'holdExitCentiC',
      'holdOnCentiC',
      'holdOffCentiC',
      'overshootCutoffCentiC',
      'approachMaxTicks',
      'approachMinPowerRatioPermille',
      'holdKpPermillePerC',
      'holdKiPermillePerCTick',
      'holdBlendTicks',
      'holdReheatPowerPermille',
      'approachLeadTicks',
      'holdLeadTicks',
      'autoAdjustableWorkingFloorMv',
      'heaterCurrentReserveMa',
    ]
    const pointFields = [
      'brakeDistanceCentiC',
      'warmupPowerPermille',
      'warmupReenterCentiC',
      'approachPowerPermille',
      'approachFloorPowerPermille',
      'approachDampingExponentPermille',
      'approachTailWindowCentiC',
      'holdPowerPermille',
      'holdReheatPowerPermille',
      'holdEntryCentiC',
      'holdExitCentiC',
      'holdOnCentiC',
      'holdOffCentiC',
      'overshootCutoffCentiC',
      'holdKpPermillePerC',
      'holdKiPermillePerCTick',
      'holdBlendTicks',
      'approachLeadTicks',
      'holdLeadTicks',
    ]
    const settings = Object.fromEntries(settingsFields.map((field) => [field, 1]))
    const points = [60, 220, 120, 80, 100, 160, 140, 180].map((targetTempC) => ({
      ...Object.fromEntries(pointFields.map((field) => [field, 1])),
      targetTempC,
    }))
    const value = {
      powerClass,
      candidateHash: 'sha256:896e40c0087904f10eee7db9ddacad5e391cfaf5915923aacf27f83f3ddefe53',
      profile: { bank: powerClass, value: { settings, points } },
    }
    await expect(validateCandidate(value, 'candidate.json')).resolves.toMatchObject({
      name: 'candidate.json',
      powerClass,
      profile: { settings, points },
    })
    const candidate = await validateCandidate(value, 'candidate.json')
    expect(buildTuningStartCommand('run-1', powerClass, candidate)).toMatchObject({
      op: 'start',
      powerClass,
      candidateHash: value.candidateHash,
      candidateProfile: { op: 'preview', bank: powerClass, profile: { settings, points } },
    })
    const otherBank = powerClass === 'pps3a' ? 'pps5a' : 'pps3a'
    expect(() => buildTuningStartCommand('run-1', otherBank, candidate)).toThrow('档位')
    await expect(
      validateCandidate({ ...value, profile: { ...value.profile, bank: otherBank } }, 'bank.json')
    ).rejects.toThrow('档位')
    expect(buildTuningStartCommand('run-2', powerClass, null)).toMatchObject({
      op: 'start',
      powerClass,
      candidateProfile: undefined,
    })
    await expect(validateCandidate({ ...value, runId: 'mock-run' }, 'mock.json')).rejects.toThrow(
      'Mock'
    )
    await expect(
      validateCandidate({ ...value, candidateHash: `sha256:${'a'.repeat(64)}` }, 'wrong.json')
    ).rejects.toThrow('hash')
    await expect(
      validateCandidate({ ...value, profile: { value: { settings, points: [] } } }, 'empty.json')
    ).rejects.toThrow('八目标')
  })
})
