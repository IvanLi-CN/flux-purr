import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import { FirmwareTuningWorkspace, validateCandidate } from './firmware-tuning-workspace'

describe('firmware tuning workspace', () => {
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

  it('checks the canonical device profile hash and rejects mock candidates', async () => {
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
    const points = [60, 240, 140, 100, 80, 120, 180, 160, 220].map((targetTempC) => ({
      ...Object.fromEntries(pointFields.map((field) => [field, 1])),
      targetTempC,
    }))
    const value = {
      powerClass: 'pps5a',
      candidateHash: 'sha256:841b20a8fec5278e37558f158496b234c89a2aca65f4393eb1a40fb4f6e40141',
      profile: { bank: 'pps5a', value: { settings, points } },
    }
    await expect(validateCandidate(value, 'candidate.json')).resolves.toMatchObject({
      name: 'candidate.json',
      profile: { settings, points },
    })
    await expect(validateCandidate({ ...value, runId: 'mock-run' }, 'mock.json')).rejects.toThrow(
      'Mock'
    )
    await expect(
      validateCandidate({ ...value, candidateHash: `sha256:${'a'.repeat(64)}` }, 'wrong.json')
    ).rejects.toThrow('hash')
    await expect(
      validateCandidate({ ...value, profile: { value: { settings, points: [] } } }, 'empty.json')
    ).rejects.toThrow('九目标')
  })
})
