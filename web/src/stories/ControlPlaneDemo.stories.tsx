import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fireEvent, userEvent, waitFor, within } from 'storybook/test'
import {
  ControlPlaneDemo,
  type LanRuntimeDependencies,
} from '@/features/control-plane-demo/components/control-plane-demo'
import type {
  CalibrationRuntimeState,
  CalibrationState,
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
  Identity,
  NetworkSummary,
} from '@/features/control-plane-demo/contracts'
import { knownWebSerialDeviceToTarget } from '@/features/control-plane-demo/known-web-serial-devices'
import type {
  LanDeviceSession,
  LanLease,
  LanProbe,
  LanPublicInfo,
} from '@/features/control-plane-demo/lan-client'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'
import {
  ControlPlaneClientError,
  type ControlPlaneHttpClient,
} from '@/features/control-plane-demo/transport-client'
import type { ControlPlaneScenario } from '@/features/control-plane-demo/types'
import type { WebSerialControlPlaneClient } from '@/features/control-plane-demo/web-serial'

const meta = {
  title: 'App/ControlPlaneDemo',
  component: ControlPlaneDemo,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
  },
  args: {
    scenario: liveControlPlaneScenario,
    initialView: 'dashboard',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: true,
      persistKnownDevices: false,
      clientFactory: () => new FakeWebSerialClient() as unknown as WebSerialControlPlaneClient,
    },
  },
} satisfies Meta<typeof ControlPlaneDemo>

export default meta
type Story = StoryObj<typeof meta>
const webSerialRuntimeWrites: DirectRuntimeConfigRequest[] = []
let webSerialConnectCalls = 0
let webSerialDisconnectCalls = 0
const heaterCurveStoryPackage = {
  points: [
    { tempCentiC: 2120, resistanceMilliohms: 4251 },
    { tempCentiC: 5180, resistanceMilliohms: 4732 },
    { tempCentiC: 7560, resistanceMilliohms: 5144 },
    { tempCentiC: 10600, resistanceMilliohms: 5555 },
    { tempCentiC: 14150, resistanceMilliohms: 6053 },
    { tempCentiC: 17675, resistanceMilliohms: 6469 },
    { tempCentiC: 21010, resistanceMilliohms: 6831 },
    { tempCentiC: 24340, resistanceMilliohms: 7124 },
  ],
} satisfies HeaterCurvePackage

const idleCalibrationRuntime = {
  mode: 'off',
  ppsEnabled: false,
  ppsMv: null,
  ppsMa: null,
  heaterEnabled: false,
  targetAdcMv: null,
  stable: false,
  stabilityErrorMv: null,
  error: null,
  job: {
    kind: null,
    status: 'idle',
    progressPercent: 0,
    samplesCollected: 0,
    nextRequestMv: null,
    message: null,
  },
} satisfies CalibrationRuntimeState

const legacyWifiStateScenario = {
  ...controlPlaneScenario,
  selectedDeviceId: 'fp-lab-01',
  devices: controlPlaneScenario.devices.map((device) =>
    device.id === 'fp-lab-01'
      ? {
          ...device,
          transport: 'devd' as const,
          leaseState: 'active' as const,
          leaseId: 'legacy-wifi-state-lease',
        }
      : device
  ),
} satisfies ControlPlaneScenario

export const DemoManualPpsPanel: Story = {
  name: 'Demo / Manual PPS panel',
  args: {
    scenario: {
      ...controlPlaneScenario,
      selectedDeviceId: 'fp-kit-02',
    },
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: /Advanced PPS/ }))
    await expect(await canvas.findByRole('slider', { name: 'Manual PPS voltage' })).toBeVisible()
  },
}

export const LegacyWifiStateProtocol: Story = {
  name: 'Settings / Legacy WiFi state protocol',
  args: {
    scenario: legacyWifiStateScenario,
    initialView: 'settings',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step(
      'WiFi configuration remains visible but cannot submit without wifi_state_v2',
      async () => {
        await expect(await canvas.findByRole('heading', { name: 'WiFi' })).toBeVisible()
        await expect(
          await canvas.findByText('当前设备固件需要 WiFi 状态协议更新后才能提交设置。')
        ).toBeVisible()
        await expect(await canvas.findByRole('textbox', { name: 'WiFi 名称' })).toBeDisabled()
        await expect(await canvas.findByRole('button', { name: '保存并连接' })).toBeDisabled()
      }
    )
  },
}

export const DemoCalibrationIdle: Story = {
  name: 'Demo / Calibration idle',
  args: {
    scenario: {
      ...controlPlaneScenario,
      devices: controlPlaneScenario.devices.map((device) =>
        device.id === controlPlaneScenario.selectedDeviceId
          ? { ...device, currentTempC: 183.6, targetTempC: 183.6, heaterOutputPercent: 0 }
          : { ...device, heaterOutputPercent: 0 }
      ),
    },
    initialView: 'calibration',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
}

export const DemoCalibrationTab: Story = {
  name: 'Demo / Calibration workbench',
  args: {
    scenario: controlPlaneScenario,
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('calibration workbench shows owner-facing modes', async () => {
      const calibrationWorkbench = canvasElement.querySelector('.industrial-calibration-workbench')
      expect(calibrationWorkbench).not.toBeNull()
      await expect(await canvas.findByRole('tab', { name: '加热曲线标定' })).toBeVisible()
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await expect(await canvas.findByRole('tab', { name: '电压读数标定' })).toBeVisible()
      await expect(await canvas.findByRole('table', { name: '加热曲线点表' })).toBeVisible()
      const statusCard = await canvas.findByRole('heading', { name: '状态' })
      const statusCardRoot = statusCard.closest('.industrial-calibration-live-card') as HTMLElement
      expect(statusCardRoot).not.toBeNull()
      await expect(within(statusCardRoot).findByText('目标温度')).resolves.toBeVisible()
      await expect(within(statusCardRoot).findByText('预览')).resolves.toBeVisible()
      await expect(await canvas.findByRole('heading', { name: '运行时追踪' })).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ 帧/)).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '导入预览' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeDisabled()
      const heaterCurveTable = await canvas.findByRole('table', { name: '加热曲线点表' })
      expect(heaterCurveTable.scrollWidth).toBeLessThanOrEqual(heaterCurveTable.clientWidth + 1)
    })

    await step('scrolling calibration content keeps the tab strip fixed', async () => {
      const tabList = canvasElement.querySelector(
        '.industrial-calibration-tabs__list'
      ) as HTMLElement | null
      const activeTabPanel = canvasElement.querySelector('[role="tabpanel"]') as HTMLElement | null
      expect(tabList).not.toBeNull()
      expect(activeTabPanel).not.toBeNull()
      if (!tabList || !activeTabPanel) {
        throw new Error('Expected calibration tabs and active tab panel to exist')
      }

      expect(activeTabPanel.scrollHeight).toBeGreaterThan(activeTabPanel.clientHeight)
      const tabListTop = Math.round(tabList.getBoundingClientRect().top)
      activeTabPanel.scrollTop = Math.min(
        240,
        activeTabPanel.scrollHeight - activeTabPanel.clientHeight
      )
      activeTabPanel.dispatchEvent(new Event('scroll'))

      await waitFor(() => {
        expect(activeTabPanel.scrollTop).toBeGreaterThan(0)
      })
      expect(Math.round(tabList.getBoundingClientRect().top)).toBe(tabListTop)
    })

    await step(
      'temperature and voltage modes keep technical details as secondary panels',
      async () => {
        await expect(
          await canvas.findByRole('slider', { name: '加热曲线标定目标温度滑块' })
        ).toBeVisible()
        await expect(
          await canvas.findByRole('spinbutton', { name: '加热曲线标定目标温度输入' })
        ).toBeVisible()
        await expect(await canvas.findByRole('heading', { name: '校准控制' })).toBeVisible()
        expect(canvas.queryByText('PPS 电流能力')).not.toBeInTheDocument()
        let actionButtons = Array.from(
          (
            canvasElement.querySelector(
              '.industrial-calibration-inline-actions--single-row'
            ) as HTMLElement | null
          )?.querySelectorAll('.industrial-button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual(['自动校准'])
        await expect(await canvas.findByRole('switch', { name: '加热开关' })).toBeVisible()
        await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))
        await expect(await canvas.findByRole('slider', { name: '目标 ADC 滑块' })).toBeVisible()
        await expect(await canvas.findByRole('spinbutton', { name: '目标 ADC 输入' })).toBeVisible()
        actionButtons = Array.from(
          (
            canvasElement.querySelector(
              '.industrial-calibration-inline-actions--single-row'
            ) as HTMLElement | null
          )?.querySelectorAll('.industrial-button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual([])
        await expect(await canvas.findByRole('switch', { name: '加热开关' })).toBeVisible()
        await expect(await canvas.findByRole('heading', { name: '温度 ADC' })).toBeVisible()
        const targetAdcInput = await canvas.findByRole('spinbutton', { name: '目标 ADC 输入' })
        const referenceTempInput = await canvas.findByRole('spinbutton', { name: '标定温度' })
        await userEvent.clear(targetAdcInput)
        await userEvent.type(targetAdcInput, '970')
        await userEvent.clear(referenceTempInput)
        await userEvent.type(referenceTempInput, '21.6')
        await userEvent.click((await canvas.findAllByRole('button', { name: '采集样本' }))[0])
        await waitFor(() => {
          expect(canvas.getAllByText(/已采集 .* 样本|captured .* sample/i).length).toBeGreaterThan(
            0
          )
        })
        await expect(await canvas.findByText(/1\/8 个样本/i)).toBeVisible()
        const rtdSampleTable = await canvas.findByRole('table', { name: '温度 ADC 样本' })
        expect(within(rtdSampleTable).getAllByText('ADC 电压').length).toBeGreaterThanOrEqual(1)
        expect(within(rtdSampleTable).getAllByText('温度').length).toBeGreaterThanOrEqual(1)
        await expect(within(rtdSampleTable).getByText('21.6℃')).toBeVisible()
        await expect(within(rtdSampleTable).getByText('970mV')).toBeVisible()
        await userEvent.click((await canvas.findAllByRole('button', { name: '采集样本' }))[0])
        await waitFor(() => {
          expect(within(rtdSampleTable).getAllByText('970mV').length).toBeGreaterThanOrEqual(2)
        })
        await expect(within(rtdSampleTable).getAllByText('21.6℃').length).toBeGreaterThanOrEqual(2)
        await userEvent.clear(targetAdcInput)
        await userEvent.type(targetAdcInput, '1000')
        await userEvent.clear(referenceTempInput)
        await userEvent.type(referenceTempInput, '58')
        await userEvent.click((await canvas.findAllByRole('button', { name: '采集样本' }))[0])
        await waitFor(() => {
          expect(within(rtdSampleTable).getByText('1000mV')).toBeVisible()
        })
        await expect(within(rtdSampleTable).getByText('58.0℃')).toBeVisible()
        await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
        await expect(await canvas.findByRole('heading', { name: '电压 ADC' })).toBeVisible()
        await expect(await canvas.findByRole('slider', { name: 'PPS 电压滑块' })).toBeVisible()
        await expect(await canvas.findByRole('spinbutton', { name: 'PPS 电压输入' })).toBeVisible()
        expect(canvas.queryByText('当前电流')).not.toBeInTheDocument()
        const vinStatusSummary = await canvas.findByLabelText('当前 ADC 标定状态摘要')
        await expect(within(vinStatusSummary).getByText('槽位 A')).toBeVisible()
        await expect(within(vinStatusSummary).getByText('槽位 B')).toBeVisible()
        actionButtons = Array.from(
          (
            canvasElement.querySelector(
              '.industrial-calibration-inline-actions--single-row'
            ) as HTMLElement | null
          )?.querySelectorAll('.industrial-button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual(['自动扫点'])
        expect(canvas.queryByRole('switch', { name: '加热开关' })).not.toBeInTheDocument()
        await expect(await canvas.findByLabelText('当前 ADC 标定状态摘要')).toBeVisible()
        await expect(await canvas.findByLabelText('电压 ADC 拟合建议 拟合建议')).toBeVisible()
        await expect(await canvas.findByRole('spinbutton', { name: '参考电压' })).toBeVisible()
        const vinSampleTable = await canvas.findByRole('table', { name: '电压 ADC 样本' })
        expect(within(vinSampleTable).getAllByText('ADC 电压').length).toBeGreaterThanOrEqual(1)
        expect(within(vinSampleTable).getAllByText('参考电压').length).toBeGreaterThanOrEqual(1)
        expect(canvas.queryByRole('button', { name: '+1V' })).not.toBeInTheDocument()
        expect(canvas.queryByText(/Range 5V/i)).not.toBeInTheDocument()
      }
    )

    await step('power capability hint moves to the title area tooltip', async () => {
      const titleMain = canvasElement.querySelector(
        '.industrial-calibration-live-card__title-main'
      ) as HTMLElement | null
      expect(titleMain).not.toBeNull()
      expect(titleMain?.querySelector('button[aria-label="查看电源能力说明"]')).not.toBeNull()
    })

    await step('voltage mode switch toggles on in demo runtime', async () => {
      const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
      await expect(modeToggle).toHaveAttribute('aria-checked', 'false')
      await userEvent.click(modeToggle)
      await waitFor(() => {
        expect(modeToggle).toHaveAttribute('aria-checked', 'true')
      })
      await waitFor(() => {
        expect(canvas.getByRole('button', { name: '自动扫点' })).toBeVisible()
      })
    })

    await step(
      'arming calibration mode auto-enables PPS runtime without a separate button',
      async () => {
        const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
        await waitFor(() => {
          expect(modeToggle).toHaveAttribute('aria-checked', 'true')
        })
        expect(canvas.queryByRole('button', { name: '申请 PPS' })).not.toBeInTheDocument()
        expect(canvas.queryByRole('button', { name: '关闭 PPS' })).not.toBeInTheDocument()
      }
    )

    await step('voltage mode action buttons stay on one row', async () => {
      const actionRow = canvasElement.querySelector(
        '.industrial-calibration-inline-actions--single-row'
      ) as HTMLElement | null
      expect(actionRow).not.toBeNull()
      if (!actionRow) {
        throw new Error('Expected calibration action row to exist')
      }
      expect(
        actionRow.classList.contains('industrial-calibration-inline-actions--single-row')
      ).toBe(true)
      expect(actionRow.scrollWidth).toBeLessThanOrEqual(actionRow.clientWidth + 2)
    })

    await step('voltage mode toggle actions block rapid repeat clicks', async () => {
      const modeToggle = canvasElement.querySelector('[role="switch"]') as HTMLElement | null
      expect(modeToggle).not.toBeNull()
      if (!modeToggle) {
        throw new Error('Expected calibration mode toggle to exist')
      }
      await userEvent.click(modeToggle)
      const startAutoButton = await canvas.findByRole('button', { name: '自动扫点' })
      await userEvent.click(startAutoButton)
      await waitFor(() => {
        expect(startAutoButton).toBeDisabled()
      })
    })

    await step(
      'armed calibration mode blocks page-internal tab switching until closed',
      async () => {
        const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
        const portalCanvas = within(canvasElement.ownerDocument.body)
        if (modeToggle.getAttribute('aria-checked') !== 'true') {
          await userEvent.click(modeToggle)
        }
        await waitFor(() => {
          expect(modeToggle).toHaveAttribute('aria-checked', 'true')
        })

        await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

        await waitFor(() => {
          const leaveGuard = canvasElement.ownerDocument.body.querySelector(
            '.industrial-calibration-leave-guard'
          ) as HTMLElement | null
          expect(leaveGuard).not.toBeNull()
          expect(leaveGuard).toBeVisible()
        })
        const leaveGuard = canvasElement.ownerDocument.body.querySelector(
          '.industrial-calibration-leave-guard'
        ) as HTMLElement | null
        const modeToggleAnchor = canvasElement.querySelector(
          '#calibration-mode-toggle-anchor'
        ) as HTMLElement | null
        if (!leaveGuard || !modeToggleAnchor) {
          throw new Error('Expected calibration leave guard and mode toggle to exist')
        }
        await expect(
          await portalCanvas.findByText('校准控制仍开着，先关闭后再切到“温度标定”。')
        ).toBeVisible()
        await expect(await canvas.findByRole('tab', { name: '电压读数标定' })).toHaveAttribute(
          'data-state',
          'active'
        )

        const leaveGuardRect = leaveGuard.getBoundingClientRect()
        const modeToggleRect = modeToggleAnchor.getBoundingClientRect()
        expect(leaveGuardRect.top).toBeGreaterThanOrEqual(modeToggleRect.bottom - 72)
        expect(leaveGuardRect.top).toBeLessThan(modeToggleRect.bottom + 88)
        expect(leaveGuardRect.left).toBeLessThan(modeToggleRect.right + 40)
        expect(leaveGuardRect.right).toBeGreaterThan(modeToggleRect.left - 40)

        await userEvent.click(await portalCanvas.findByRole('button', { name: '关闭并继续' }))

        await waitFor(() => {
          expect(modeToggle).toHaveAttribute('aria-checked', 'false')
        })
        await expect(await canvas.findByRole('tab', { name: '温度标定' })).toHaveAttribute(
          'data-state',
          'active'
        )
      }
    )
  },
}

export const DemoCalibrationLeaveGuard: Story = {
  name: 'Demo / Calibration leave guard',
  args: {
    scenario: controlPlaneScenario,
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
    const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
    await userEvent.click(modeToggle)
    await waitFor(() => {
      expect(modeToggle).toHaveAttribute('aria-checked', 'true')
    })
    await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))
    await waitFor(() => {
      const leaveGuard = canvasElement.ownerDocument.body.querySelector(
        '.industrial-calibration-leave-guard'
      ) as HTMLElement | null
      expect(leaveGuard).not.toBeNull()
      expect(leaveGuard).toBeVisible()
    })
  },
}

export const DemoCalibrationHeaterCurvePreview: Story = {
  name: 'Demo / 加热曲线标定 preview',
  args: {
    scenario: {
      ...controlPlaneScenario,
      devices: controlPlaneScenario.devices.map((device) =>
        device.id === controlPlaneScenario.selectedDeviceId
          ? {
              ...device,
              heaterCurve: {
                active: {
                  points: [
                    { tempCentiC: 2120, resistanceMilliohms: 4251 },
                    { tempCentiC: 5180, resistanceMilliohms: 4732 },
                    { tempCentiC: 7560, resistanceMilliohms: 5144 },
                    { tempCentiC: 10600, resistanceMilliohms: 5555 },
                    { tempCentiC: 14150, resistanceMilliohms: 6053 },
                    { tempCentiC: 17675, resistanceMilliohms: 6469 },
                    { tempCentiC: 21010, resistanceMilliohms: 6831 },
                    { tempCentiC: 24340, resistanceMilliohms: 7124 },
                  ],
                },
                preview: {
                  points: [
                    { tempCentiC: 2120, resistanceMilliohms: 4270 },
                    { tempCentiC: 5180, resistanceMilliohms: 4750 },
                    { tempCentiC: 7560, resistanceMilliohms: 5160 },
                    { tempCentiC: 10600, resistanceMilliohms: 5572 },
                    { tempCentiC: 14150, resistanceMilliohms: 6073 },
                    { tempCentiC: 17675, resistanceMilliohms: 6488 },
                    { tempCentiC: 21010, resistanceMilliohms: 6850 },
                    { tempCentiC: 24340, resistanceMilliohms: 7142 },
                  ],
                },
              },
            }
          : device
      ),
    },
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('shows a previewed heater curve', async () => {
      await expect(await canvas.findByRole('table', { name: '加热曲线点表' })).toBeVisible()
      const statusCard = await canvas.findByRole('heading', { name: '状态' })
      const statusCardRoot = statusCard.closest('.industrial-calibration-live-card') as HTMLElement
      expect(statusCardRoot).not.toBeNull()
      await waitFor(() => {
        expect(within(statusCardRoot).getByText('目标温度')).toBeVisible()
      })
      await expect(await canvas.findByRole('columnheader', { name: '预览温度' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeEnabled()
    })

    await step('save promotes preview to active curve', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '保存曲线' }))
      const statusCard = await canvas.findByRole('heading', { name: '状态' })
      const statusCardRoot = statusCard.closest('.industrial-calibration-live-card') as HTMLElement
      expect(statusCardRoot).not.toBeNull()
      await waitFor(() => {
        expect(within(statusCardRoot).getByText('目标温度')).toBeVisible()
      })
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeDisabled()
      await expect(canvas.getByRole('table', { name: '加热曲线点表' })).toBeVisible()
    })
  },
}

export const DemoCalibrationSlotEditor: Story = {
  name: 'Demo / Calibration slot editor',
  args: {
    scenario: {
      ...controlPlaneScenario,
      devices: controlPlaneScenario.devices.map((device) =>
        device.id === controlPlaneScenario.selectedDeviceId
          ? { ...device, heaterEnabled: true, heaterOutputPercent: 0 }
          : device
      ),
    },
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const portalCanvas = within(canvasElement.ownerDocument.body)

    await step('slot editor writes explicit A/B fits', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

      const statusSummary = await canvas.findByLabelText('当前 ADC 标定状态摘要')
      await userEvent.click(within(statusSummary).getAllByRole('button', { name: '编辑' })[0])

      await expect(await portalCanvas.findByRole('dialog')).toBeVisible()
      await expect(await portalCanvas.findByText('温度 ADC 槽位 A')).toBeVisible()
      await userEvent.clear(portalCanvas.getByRole('spinbutton', { name: '增益' }))
      await userEvent.type(portalCanvas.getByRole('spinbutton', { name: '增益' }), '1.01234')
      await userEvent.clear(portalCanvas.getByRole('spinbutton', { name: '偏移' }))
      await userEvent.type(portalCanvas.getByRole('spinbutton', { name: '偏移' }), '12.3')
      await userEvent.click(await portalCanvas.findByRole('button', { name: '保存' }))

      await waitFor(() => {
        expect(within(statusSummary).getByText('1.01234x')).toBeVisible()
      })
      await expect(within(statusSummary).getByText('12.3mV')).toBeVisible()
    })
  },
}

export const DemoTemperatureCalibrationHeatingFeedback: Story = {
  name: 'Demo / 温度标定 heating feedback',
  args: {
    scenario: {
      ...controlPlaneScenario,
      selectedDeviceId: 'fp-lab-01',
      devices: controlPlaneScenario.devices.map((device) =>
        device.id === 'fp-lab-01'
          ? {
              ...device,
              transport: 'devd',
              baseUrl: 'devd://fp-lab-01',
              severity: 'nominal',
              leaseState: 'active',
              leaseId: 'story-lease',
              rtdRawAdcMv: 1120,
              heaterEnabled: false,
              heaterOutputPercent: 0,
              calibration: {
                ...idleCalibrationRuntime,
                mode: 'rtd_adc',
                ppsEnabled: true,
                ppsMv: 16000,
                targetAdcMv: 970,
              },
            }
          : device
      ),
    },
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step(
      'heater toggle stays available and status card follows hardware output',
      async () => {
        await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
        await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

        const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
        await expect(modeToggle).toHaveAttribute('aria-checked', 'true')

        const heaterToggle = await canvas.findByRole('switch', { name: '加热开关' })
        await expect(heaterToggle).toBeEnabled()
        await expect(await canvas.findByRole('meter', { name: '加热强度' })).toHaveAttribute(
          'value',
          '0'
        )

        await userEvent.click(heaterToggle)

        await waitFor(() => {
          expect(canvas.getByRole('switch', { name: '加热开关' })).toBeEnabled()
        })
        await expect(await canvas.findByRole('meter', { name: '加热强度' })).toHaveAttribute(
          'value',
          '0'
        )
      }
    )
  },
}

export const DemoCalibrationManualFit: Story = {
  name: 'Demo / ADC slot fits',
  args: {
    scenario: {
      ...controlPlaneScenario,
      devices: controlPlaneScenario.devices.map((device) =>
        device.id === controlPlaneScenario.selectedDeviceId
          ? { ...device, heaterOutputPercent: 0 }
          : device
      ),
    },
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const portalCanvas = within(canvasElement.ownerDocument.body)

    await step('slot summaries update after explicit A/B fit edits', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

      let statusSummary = await canvas.findByLabelText('当前 ADC 标定状态摘要')
      await userEvent.click(within(statusSummary).getAllByRole('button', { name: '编辑' })[0])
      let gainInput = await portalCanvas.findByRole('spinbutton', { name: '增益' })
      let offsetInput = await portalCanvas.findByRole('spinbutton', { name: '偏移' })

      await userEvent.clear(gainInput)
      await userEvent.type(gainInput, '1.01234')
      await userEvent.clear(offsetInput)
      await userEvent.type(offsetInput, '12.3')
      await userEvent.click(await portalCanvas.findByRole('button', { name: '保存' }))

      await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
      statusSummary = await canvas.findByLabelText('当前 ADC 标定状态摘要')
      await userEvent.click(within(statusSummary).getAllByRole('button', { name: '编辑' })[1])
      gainInput = await portalCanvas.findByRole('spinbutton', { name: '增益' })
      offsetInput = await portalCanvas.findByRole('spinbutton', { name: '偏移' })

      await userEvent.clear(gainInput)
      await userEvent.type(gainInput, '0.98047')
      await userEvent.clear(offsetInput)
      await userEvent.type(offsetInput, '149.8')
      await userEvent.click(await portalCanvas.findByRole('button', { name: '保存' }))

      await waitFor(() => {
        expect(within(statusSummary).getByText('0.98047x')).toBeVisible()
      })
      await expect(within(statusSummary).getByText('149.8mV')).toBeVisible()
    })
  },
}

export const DemoCalibrationDenseLists: Story = {
  name: 'Demo / ADC sample lists',
  args: {
    scenario: createCalibrationDenseScenario(),
    initialView: 'calibration',
    allowDemoControls: true,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('fills both calibration sample lists to their scroll boundary', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

      await waitFor(() => {
        expect(canvas.getByText('8/8 个样本')).toBeVisible()
      })

      const rtdList = await canvas.findByRole('region', { name: '温度 ADC 样本列表' })
      rtdList.scrollTop = rtdList.scrollHeight
      fireEvent.scroll(rtdList)

      await expect(await canvas.findByRole('heading', { name: '运行时追踪' })).toBeVisible()

      await expect(
        within(rtdList).getByRole('button', { name: '删除 温度 ADC 样本 8' })
      ).toBeVisible()

      await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
      await waitFor(() => {
        expect(canvas.getByText('8/8 个样本')).toBeVisible()
      })
      const vinList = await canvas.findByRole('region', { name: '电压 ADC 样本列表' })
      vinList.scrollTop = vinList.scrollHeight
      fireEvent.scroll(vinList)
      await expect(
        within(vinList).getByRole('button', { name: '删除 电压 ADC 样本 8' })
      ).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ 帧/)).toBeVisible()
    })
  },
}

export const DemoCalibrationIncompleteRtdSingleSample: Story = {
  name: 'Demo / Incomplete RTD single sample',
  args: {
    scenario: createIncompleteRtdSingleSampleScenario(),
    initialView: 'calibration',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('does not render legacy RTD fit samples as temperature samples', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

      const rtdList = await canvas.findByRole('region', { name: '温度 ADC 样本列表' })
      await expect(within(rtdList).getByText('0/8 个样本')).toBeVisible()
      expect(within(rtdList).queryByRole('button', { name: /删除 温度 ADC 样本/ })).toBeNull()
      expect(within(rtdList).queryByText('1/8 个样本')).toBeNull()
    })
  },
}

export const LiveWebSerialAddDevice: Story = {
  name: 'Live / Web Serial Add Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const documentRoot = within(canvasElement.ownerDocument.body)
    webSerialRuntimeWrites.length = 0
    webSerialConnectCalls = 0
    webSerialDisconnectCalls = 0

    await step('no live target starts on the device chooser', async () => {
      await expect(await canvas.findByRole('heading', { name: 'Choose target' })).toBeVisible()
      await expect(await canvas.findByText('No known devices')).toBeVisible()
      await expect(await canvas.findByRole('separator')).toBeVisible()
      const addDeviceButtons = ['WiFi', 'Web Serial', '桥接'].map((name) =>
        canvas.getByRole('button', { name: new RegExp(name) })
      )
      const addDeviceRows = new Set(
        addDeviceButtons.map((button) => Math.round(button.getBoundingClientRect().top))
      )
      expect(addDeviceButtons).toHaveLength(3)
      expect(addDeviceRows.size).toBe(1)
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
      await expect(canvas.queryByText('1000 frames')).not.toBeInTheDocument()
    })

    await step(
      'successful Web Serial connect returns to Dashboard with real log entries',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))

        await waitFor(() => {
          expect(canvas.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
        })
        await expect(canvas.getByRole('button', { name: '目标设备' })).toHaveTextContent(
          'flux-purr-s3-001'
        )
        await expect(canvas.getByRole('button', { name: '目标设备' })).toHaveTextContent('串口')
        await expect(await canvas.findByText('Web Serial connected')).toBeVisible()
        await expect(
          await canvas.findByText(
            'flux-purr-s3-001 USB JSONL probe accepted: get_identity / get_network / get_status'
          )
        ).toBeVisible()
        await expect(canvas.queryByText('1000 frames')).not.toBeInTheDocument()
      }
    )

    await step('Dashboard target stepper advances immediately across rapid clicks', async () => {
      const increase = await canvas.findByRole('button', { name: 'Increase target temperature' })
      await userEvent.click(increase)
      await userEvent.click(increase)
      await userEvent.click(increase)

      await waitFor(() => {
        expect(
          canvas.getByRole('spinbutton', { name: 'Dashboard target temperature' })
        ).toHaveValue(45)
      })
      await waitFor(() => {
        expect(
          webSerialRuntimeWrites.filter((request) => request.targetTempC != null)
        ).toHaveLength(1)
      })
      expect(webSerialRuntimeWrites.at(-1)?.targetTempC).toBe(45)
    })

    await step('Dashboard advanced PPS override writes through Web Serial', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /Advanced PPS/ }))
      const slider = await canvas.findByRole('slider', { name: 'Manual PPS voltage' })
      fireEvent.input(slider, { target: { value: '10400' } })
      await userEvent.click(await canvas.findByRole('button', { name: 'Apply PPS' }))

      await waitFor(() => {
        expect(webSerialRuntimeWrites.at(-1)?.manualPpsEnabled).toBe(true)
      })
      expect(webSerialRuntimeWrites.at(-1)?.manualPpsMv).toBe(10_400)
      await expect(await canvas.findByText(/Manual 10.4V/)).toBeVisible()
      await userEvent.click(await canvas.findByRole('button', { name: 'Clear' }))
      await waitFor(() => {
        expect(webSerialRuntimeWrites.at(-1)?.manualPpsEnabled).toBe(false)
      })
    })

    await step('global log remains expanded after switching to settings', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /设置/ }))

      await expect(await canvas.findByRole('heading', { name: 'Heat policy' })).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: '运行时追踪' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '全部' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '完成' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('button', { name: '完成' }))
      await expect(await canvas.findByRole('button', { name: '完成' })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
      await userEvent.click(await canvas.findByRole('button', { name: '全部' }))
      await expect(
        await canvas.findByText(
          'flux-purr-s3-001 USB JSONL probe accepted: get_identity / get_network / get_status'
        )
      ).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ 帧/)).toBeVisible()
    })

    await step(
      'Settings preset edits write through Web Serial and re-render from status',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: /M5 180℃ enabled/ }))

        await waitFor(() => {
          expect(canvas.getByRole('button', { name: /M5 180℃ enabled/ })).toHaveAttribute(
            'aria-pressed',
            'true'
          )
        })
        await userEvent.click(await canvas.findByRole('switch', { name: 'Preset M5' }))

        await waitFor(() => {
          expect(canvas.getByRole('button', { name: /M5 --- disabled/ })).toBeVisible()
        })
      }
    )

    await step('Settings fan policy keeps the acknowledged operator selection', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: 'OFF' }))

      await waitFor(() => {
        expect(canvas.getByRole('button', { name: 'OFF' })).toHaveAttribute('aria-pressed', 'true')
      })
      await expect(await canvas.findByText('flux-purr-s3-001 fan policy is now OFF.')).toBeVisible()
    })

    await step('Add device can choose another Web Serial port after one is connected', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '目标设备' }))
      await userEvent.click(await documentRoot.findByRole('button', { name: '添加设备' }))

      const webSerialButton = await canvas.findByRole('button', { name: /Web Serial/ })
      await expect(webSerialButton).toBeEnabled()
      await userEvent.click(webSerialButton)

      await waitFor(() => expect(webSerialConnectCalls).toBe(2))
      expect(webSerialDisconnectCalls).toBe(1)
    })
  },
}

export const LiveKnownWebSerialReconnect: Story = {
  name: 'Live / Known Web Serial reconnect',
  args: {
    scenario: {
      ...liveControlPlaneScenario,
      selectedDeviceId: 'web-serial-a0f262f20d6c',
      devices: [
        knownWebSerialDeviceToTarget({
          deviceId: 'a0f262f20d6c',
          hostname: 'flux-purr-a0f262f20d6c',
          firmwareVersion: '0.1.0',
          buildId: 'build-1',
        }),
      ],
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const documentRoot = within(canvasElement.ownerDocument.body)
    webSerialConnectCalls = 0

    await step(
      'known device channel reuses browser authorization and verifies identity',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: '目标设备' }))
        await userEvent.click(
          await documentRoot.findByRole('button', {
            name: 'Web Serial · flux-purr-a0f262f20d6c',
          })
        )

        await waitFor(() => expect(webSerialConnectCalls).toBe(1))
        await expect(await canvas.findByText('Web Serial connected')).toBeVisible()
        await expect(await canvas.findByRole('button', { name: '目标设备' })).toHaveTextContent(
          'flux-purr-s3-001'
        )
        await userEvent.click(await canvas.findByRole('button', { name: '目标设备' }))
        await expect(await documentRoot.findByText('设备 ID · a0f262f20d6c')).toBeVisible()
        await expect(await documentRoot.findByText('设备 ID · flux-purr-s3-001')).toBeVisible()
      }
    )
  },
}

export const LiveLanAddDeviceSelection: Story = {
  name: 'Live / LAN add device selection',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const documentRoot = within(canvasElement.ownerDocument.body)

    await step('WiFi is selected by default and shows its LAN address entry', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '目标设备' }))
      await userEvent.click(await documentRoot.findByRole('button', { name: '添加设备' }))

      await expect(await canvas.findByRole('heading', { name: 'Choose connection' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: /WiFi/ })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
      await expect(await canvas.findByLabelText('WiFi LAN pairing')).toBeVisible()
      await expect(await canvas.findByLabelText('设备地址')).toBeVisible()
    })
  },
}

export const LiveLanConnectionChoice: Story = {
  name: 'Live / LAN connection choice',
  args: {
    initialView: 'add-device',
  },
}

export const LiveKnownDeviceSelection: Story = {
  name: 'Live / Known Device Selection',
  args: {
    scenario: createKnownDeviceSelectionScenario(),
    initialView: 'dashboard',
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('known devices are shown while browser-only serial targets are hidden', async () => {
      await expect(await canvas.findByRole('heading', { name: 'Choose target' })).toBeVisible()
      await expect(
        await canvas.findByRole('button', { name: /Authorized USB target/ })
      ).toBeVisible()
      await expect(canvas.queryByRole('button', { name: /Browser Direct/ })).not.toBeInTheDocument()
      await expect(await canvas.findByRole('separator')).toBeVisible()
      await expect(await canvas.findByRole('button', { name: /WiFi/ })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: /Web Serial/ })).toBeVisible()
      await expect(
        await canvas.findByRole('button', { name: /准备本机 devd 桥接目标/ })
      ).toBeVisible()
      const addDeviceOptions = Array.from(
        canvasElement.querySelectorAll<HTMLButtonElement>('.industrial-add-device-option')
      )
      expect(addDeviceOptions).toHaveLength(3)
      const addDeviceRows = new Set(
        addDeviceOptions.map((button) => Math.round(button.getBoundingClientRect().top))
      )
      expect(addDeviceRows.size).toBe(1)
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
    })

    await step('selecting a known device opens its runtime surface', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /Authorized USB target/ }))
      await expect(await canvas.findByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
      await waitFor(() => {
        expect(canvas.getAllByText('Authorized USB target selected').length).toBeGreaterThan(0)
      })
    })
  },
}

export const LiveQuickAddDevice: Story = {
  name: 'Live / Quick Add Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('quick add WiFi switches into the LAN connection form', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /WiFi/ }))

      await expect(await canvas.findByRole('heading', { name: 'Choose connection' })).toBeVisible()
      await expect(await canvas.findByLabelText('WiFi LAN pairing')).toBeVisible()
      await expect(await canvas.findByLabelText('设备地址')).toBeVisible()
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
    })
  },
}

export const LiveQuickAddBridgeDevice: Story = {
  name: 'Live / Quick Add Bridge Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('quick add Bridge requires route and target selection before binding', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /桥接/ }))

      await expect(await canvas.findByRole('heading', { name: 'Choose connection' })).toBeVisible()
      await expect(await canvas.findByRole('region', { name: 'DEVD 桥接目标' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: 'USB' })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
      await expect(await canvas.findByRole('button', { name: 'WiFi / LAN' })).toHaveAttribute(
        'aria-pressed',
        'false'
      )
      await expect(await canvas.findByRole('button', { name: '连接所选设备' })).toBeDisabled()
      await expect(canvas.queryByText('Native bridge added')).not.toBeInTheDocument()

      await userEvent.click(await canvas.findByRole('button', { name: 'WiFi / LAN' }))
      await expect(await canvas.findByRole('button', { name: 'WiFi / LAN' })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
      await expect(await canvas.findByRole('button', { name: '选择候选设备' })).toBeDisabled()
      await expect(await canvas.findByRole('button', { name: '刷新服务' })).toBeEnabled()
      await expect(await canvas.findByLabelText('CIDR 网段')).toBeVisible()
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
    })

    await step(
      'connecting Web Serial from the pending Bridge flow selects the hardware target',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))

        await waitFor(() => {
          expect(canvas.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
        })
        await expect(canvas.getByRole('button', { name: '目标设备' })).toHaveTextContent(
          'flux-purr-s3-001'
        )
        await expect(canvas.getByRole('button', { name: '目标设备' })).toHaveTextContent('串口')
        await expect(
          canvas.queryByText(/Native bridge \/ BRIDGE|本机桥接 \/ 桥接/)
        ).not.toBeInTheDocument()
        await expect(await canvas.findByText('Web Serial connected')).toBeVisible()
      }
    )
  },
}

export const LiveWebSerialConnectionTimeout: Story = {
  name: 'Live / Web Serial connection timeout feedback',
  args: {
    webSerial: {
      enabled: true,
      connectTimeoutMs: 2_000,
      clientFactory: () => new HangingWebSerialClient() as unknown as WebSerialControlPlaneClient,
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    await expect(await canvas.findByRole('heading', { name: 'Choose target' })).toBeVisible()
    await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))

    await expect(await canvas.findByText(/正在等待浏览器选择串口/)).toBeVisible()
    await expect(
      await canvas.findByText('Web Serial 连接超时，请重新选择设备。', {}, { timeout: 4_000 })
    ).toBeVisible()
    await expect(await canvas.findByText('Web Serial unavailable')).toBeVisible()
  },
}

export const LiveWebSerialPortSelectionCancelled: Story = {
  name: 'Live / Web Serial port selection cancelled',
  args: {
    webSerial: {
      enabled: true,
      clientFactory: () => new CancelledWebSerialClient() as unknown as WebSerialControlPlaneClient,
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    await expect(await canvas.findByRole('heading', { name: 'Choose target' })).toBeVisible()
    await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))

    await expect(await canvas.findByText('Web Serial unavailable')).toBeVisible()
    await expect(
      await canvas.findByText('浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。')
    ).toBeVisible()
    await expect(await canvas.findByRole('button', { name: /Web Serial/ })).toBeEnabled()
  },
}

const bridgeDiscoveryRequests = { list: 0, mdns: 0, cidr: 0 }
const bridgeDiscoveryClient = {
  async listDevdLanDevices() {
    bridgeDiscoveryRequests.list += 1
    return []
  },
  async refreshDevdLanMdns() {
    bridgeDiscoveryRequests.mdns += 1
    return [
      {
        id: 'lan-flux-purr-a0f262f20d6c',
        baseUrl: 'http://192.168.31.189',
        hostname: 'flux-purr-a0f262f20d6c',
        lastIpv4: '192.168.31.189',
        paired: true,
      },
    ]
  },
  async scanDevdLanCidr(_baseUrl: string, cidr: string) {
    bridgeDiscoveryRequests.cidr += 1
    expect(cidr).toBe('192.168.31.0/24')
    return [
      {
        id: 'lan-flux-purr-001122334455',
        baseUrl: 'http://192.168.31.118',
        hostname: 'flux-purr-001122334455',
        lastIpv4: '192.168.31.118',
        paired: false,
      },
    ]
  },
} as unknown as ControlPlaneHttpClient

export const LiveBridgeLanServiceDiscovery: Story = {
  name: 'Live / Bridge LAN service discovery',
  args: {
    initialView: 'add-device',
    devd: {
      enabled: false,
      devdBaseUrl: 'http://127.0.0.1:4170',
      httpClient: bridgeDiscoveryClient,
    },
    webSerial: { enabled: false },
  },
  play: async ({ canvasElement, step }) => {
    bridgeDiscoveryRequests.list = 0
    bridgeDiscoveryRequests.mdns = 0
    bridgeDiscoveryRequests.cidr = 0
    const canvas = within(canvasElement)

    await step('LAN bridge loads registry without scanning the network', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /桥接/ }))
      await userEvent.click(await canvas.findByRole('button', { name: 'WiFi / LAN' }))
      await waitFor(() => expect(bridgeDiscoveryRequests.list).toBe(1))
      expect(bridgeDiscoveryRequests.mdns).toBe(0)
      expect(bridgeDiscoveryRequests.cidr).toBe(0)
      await expect(await canvas.findByRole('button', { name: '刷新服务' })).toBeEnabled()
      await expect(await canvas.findByLabelText('CIDR 网段')).toBeVisible()
    })

    await step('explicit mDNS refresh adds the hostname candidate', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '刷新服务' }))
      await expect(await canvas.findByText('flux-purr-a0f262f20d6c')).toBeVisible()
      expect(bridgeDiscoveryRequests.mdns).toBe(1)
    })

    await step('CIDR enter submits once and retains the mDNS candidate', async () => {
      const input = await canvas.findByLabelText('CIDR 网段')
      await userEvent.clear(input)
      await userEvent.type(input, '192.168.31.0/24{enter}')
      await expect(await canvas.findByText('flux-purr-001122334455')).toBeVisible()
      await expect(await canvas.findByText('flux-purr-a0f262f20d6c')).toBeVisible()
      expect(bridgeDiscoveryRequests.cidr).toBe(1)
    })

    await step('a discovered candidate cannot masquerade as a connected target', async () => {
      const target = await canvas.findByRole('button', { name: /flux-purr-a0f262f20d6c/ })
      await userEvent.click(target)
      await userEvent.click(await canvas.findByRole('button', { name: '选择候选设备' }))
      await expect(await canvas.findByText('已选择 LAN 候选设备')).toBeVisible()
      await expect(canvas.getByRole('button', { name: '目标设备' })).not.toHaveTextContent(
        'flux-purr-a0f262f20d6c'
      )
    })
  },
}

export const LiveBridgeUsbTargetSelection: Story = {
  name: 'Live / Bridge USB target selection',
  args: {
    scenario: createKnownDeviceSelectionScenario(),
    initialView: 'add-device',
    webSerial: { enabled: false },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('a concrete DEVD USB target must be selected before connecting', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /桥接/ }))
      const target = await canvas.findByRole('button', { name: /Authorized USB target/ })
      const connect = await canvas.findByRole('button', { name: '连接所选设备' })

      await expect(connect).toBeDisabled()
      await userEvent.click(target)
      await expect(target).toHaveAttribute('aria-pressed', 'true')
      await expect(connect).toBeEnabled()

      await userEvent.click(await canvas.findByRole('button', { name: 'WiFi / LAN' }))
      await expect(connect).toBeDisabled()
      await expect(await canvas.findByRole('button', { name: '刷新服务' })).toBeEnabled()
      await expect(await canvas.findByLabelText('CIDR 网段')).toBeVisible()

      await userEvent.click(await canvas.findByRole('button', { name: 'USB' }))
      const restoredTarget = await canvas.findByRole('button', { name: /Authorized USB target/ })
      await expect(restoredTarget).toHaveAttribute('aria-pressed', 'false')
      await expect(connect).toBeDisabled()

      await userEvent.click(restoredTarget)
      await userEvent.click(connect)

      await expect(await canvas.findByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
      await expect(canvas.getByRole('button', { name: '目标设备' })).toHaveTextContent(
        'Authorized USB target'
      )
      await expect(
        canvas.getByText('DEVD', { selector: '.industrial-status-datum strong' })
      ).toBeVisible()
    })
  },
}

export const LiveBridgeTargetChooser: Story = {
  name: 'Live / Bridge target chooser',
  args: {
    scenario: createKnownDeviceSelectionScenario(),
    initialView: 'add-device',
    webSerial: { enabled: false },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('bridge opens with no implicit target binding', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /桥接/ }))

      const bridgeOption = await canvas.findByRole('button', { name: /桥接/ })
      const target = await canvas.findByRole('button', { name: /Authorized USB target/ })
      await expect(target).toHaveAttribute('aria-pressed', 'false')
      await expect(await canvas.findByRole('button', { name: '连接所选设备' })).toBeDisabled()
      await expect(canvasElement.ownerDocument.activeElement).not.toBe(bridgeOption)
      await expect(canvas.getByRole('button', { name: '目标设备' })).not.toHaveTextContent(
        'Native bridge'
      )
    })
  },
}

export const LiveBridgeKeyboardFocus: Story = {
  name: 'Live / Bridge keyboard focus',
  args: {
    scenario: createKnownDeviceSelectionScenario(),
    initialView: 'add-device',
    webSerial: { enabled: false },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('keyboard activation preserves the visible focus indicator', async () => {
      const bridgeOption = await canvas.findByRole('button', { name: /桥接/ })
      bridgeOption.focus()
      await userEvent.keyboard('{Enter}')

      await expect(bridgeOption).toHaveAttribute('aria-pressed', 'true')
      await expect(canvasElement.ownerDocument.activeElement).toBe(bridgeOption)
    })
  },
}

export const LiveWebSerialTemperatureCalibrationTargetHolds: Story = {
  name: 'Live / Temperature calibration target holds while live polling',
  args: {
    scenario: liveControlPlaneScenario,
    initialView: 'calibration',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: true,
      clientFactory: () =>
        new FakeWebSerialClient(
          {
            calibration: {
              ...idleCalibrationRuntime,
              mode: 'rtd_adc',
              targetAdcMv: null,
            },
            rtdRawAdcMv: 913,
            targetTempC: 260,
          },
          {
            mutateOnProbe: (currentStatus) => ({
              ...currentStatus,
              rtdRawAdcMv: (currentStatus.rtdRawAdcMv ?? 913) + 1,
            }),
          }
        ) as unknown as WebSerialControlPlaneClient,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    webSerialRuntimeWrites.length = 0

    await step('connects the live Web Serial target from calibration flow', async () => {
      await expect(await canvas.findByRole('heading', { name: 'Choose target' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))
      await waitFor(() => {
        expect(canvas.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
      })
      await userEvent.click(await canvas.findByRole('button', { name: '校准' }))
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))
    })

    await step('keeps the drafted target ADC across live polling', async () => {
      const targetAdcInput = await canvas.findByRole('spinbutton', { name: '目标 ADC 输入' })
      await expect(targetAdcInput).toHaveValue(913)

      await userEvent.clear(targetAdcInput)
      await userEvent.type(targetAdcInput, '950')
      await verifyStoryDelay(1_300)

      await waitFor(() => {
        expect(canvas.getByRole('spinbutton', { name: '目标 ADC 输入' })).toHaveValue(950)
      })
    })
  },
}

export const LiveHeaterSafetyLockFeedback: Story = {
  name: 'Live / Heater safety lock feedback',
  args: {
    scenario: {
      ...liveControlPlaneScenario,
      selectedDeviceId: 'serial-heater-lock',
      devices: [
        {
          id: 'serial-heater-lock',
          alias: 'Authorized USB target',
          location: '/dev/cu.usbmodem21231401',
          transport: 'devd',
          severity: 'nominal',
          baseUrl: 'devd://serial-heater-lock',
          firmware: '0.1.0',
          buildId: 'story-devd',
          uptime: '00:09:12',
          boardTempC: 92.4,
          currentTempC: 214.8,
          targetTempC: 220,
          rtdRawAdcMv: 1498,
          vinRawAdcMv: 2760,
          voltageMv: 20_100,
          currentMa: 840,
          pdRequestMv: 20_000,
          pdContractMv: 20_000,
          pdState: 'ready',
          manualPpsEnabled: false,
          manualPpsMv: null,
          manualPpsMa: null,
          ppsCapabilityMinMv: 5_000,
          ppsCapabilityMaxMv: 21_000,
          ppsCapabilityMaxMa: 3_000,
          manualPpsError: null,
          heaterLockReason: 'cooling-disabled-overtemp',
          calibration: idleCalibrationRuntime,
          heaterEnabled: false,
          heaterOutputPercent: 0,
          activeCoolingEnabled: false,
          fanState: 'OFF',
          wifiRssi: null,
          capabilities: ['identity', 'status', 'monitor'],
          networkState: 'idle',
          leaseState: 'active',
          leaseId: 'story-lease-lock',
        },
      ],
    },
    initialView: 'dashboard',
    allowDemoControls: false,
    devd: {
      enabled: false,
    },
    webSerial: {
      enabled: false,
    },
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step(
      'shows a concrete heater safety lock reason instead of a generic disconnect',
      async () => {
        await expect(await canvas.findByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
        await expect(await canvas.findByText('加热安全锁已触发')).toBeVisible()
        await expect(await canvas.findByText('locked')).toBeVisible()
        await expect(
          await canvas.findAllByText('热板温度过高且主动散热已关闭，安全锁已关闭加热。')
        ).toHaveLength(3)
        await expect(canvas.queryByText('硬件连接受阻')).not.toBeInTheDocument()
      }
    )
  },
}

function createCalibrationDenseScenario(): ControlPlaneScenario {
  const longTraceDetail =
    'calibration_config response payload includes shared samples, fitted suggestions, A/B slot fits, active slot, raw observed millivolts, reference targets, and operator feedback metadata for the current lease'
  const denseCalibration = {
    rtdAdc: {
      samples: Array.from({ length: 8 }, (_, index) => {
        const targetAdcMv = 940 + index * 18
        const referenceTempC = 20 + index * 14
        return {
          observedMv: targetAdcMv + 3,
          expectedMv: targetAdcMv,
          referenceTempC,
          targetAdcMv,
        }
      }),
      fittedFit: { gain: 0.998, offsetMv: -3, sampleCount: 8 },
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 0.998, offsetMv: -3 },
      },
      activeSlot: 'a',
    },
    vinAdc: {
      samples: Array.from({ length: 8 }, (_, index) => {
        const expectedMv = 5_000 + index * 2_000
        const observedMv = Math.round((expectedMv * 5100) / (56_000 + 5100))
        return {
          observedMv,
          expectedMv,
          referenceVinMv: expectedMv,
        }
      }),
      fittedFit: { gain: 11.98039, offsetMv: 0, sampleCount: 8 },
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 11.98039, offsetMv: 0 },
      },
      activeSlot: 'a',
    },
  } satisfies CalibrationState

  return {
    ...controlPlaneScenario,
    devices: controlPlaneScenario.devices.map((device) =>
      device.id === controlPlaneScenario.selectedDeviceId
        ? {
            ...device,
            heaterOutputPercent: 0,
            currentTempC: 183.6,
            voltageMv: 20_010,
            storedCalibration: denseCalibration,
          }
        : { ...device, heaterOutputPercent: 0 }
    ),
    events: controlPlaneScenario.events.map((event, index) => ({
      ...event,
      detail: index % 2 === 0 ? longTraceDetail : event.detail,
      message:
        index % 3 === 0
          ? `${event.message}; calibration samples and event stream remained bounded after dense operator sampling`
          : event.message,
    })),
  }
}

function createIncompleteRtdSingleSampleScenario(): ControlPlaneScenario {
  const incompleteCalibration = {
    rtdAdc: {
      samples: [{ observedMv: 1_019, expectedMv: 1_000 }, null, null, null, null, null, null, null],
      fittedFit: { gain: 1, offsetMv: -19, sampleCount: 1 },
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 1, offsetMv: 0 },
      },
      activeSlot: 'a',
    },
    vinAdc: {
      samples: [null, null, null, null, null, null, null, null],
      fittedFit: { gain: 1, offsetMv: 0, sampleCount: 0 },
      slots: {
        a: { gain: 1, offsetMv: 0 },
        b: { gain: 1, offsetMv: 0 },
      },
      activeSlot: 'a',
    },
  } satisfies CalibrationState

  return {
    ...controlPlaneScenario,
    devices: controlPlaneScenario.devices.map((device) =>
      device.id === controlPlaneScenario.selectedDeviceId
        ? {
            ...device,
            currentTempC: 72.6,
            rtdRawAdcMv: 1_019,
            storedCalibration: incompleteCalibration,
            calibration: {
              ...device.calibration,
              mode: 'rtd_adc',
              ppsEnabled: true,
              ppsMv: 15_500,
              heaterEnabled: true,
              targetAdcMv: 1_000,
            },
          }
        : device
    ),
  }
}

type FakeWebSerialClientOptions = {
  mutateOnProbe?: (currentStatus: ControlPlaneStatus) => ControlPlaneStatus
}

class HangingWebSerialClient {
  connect(): Promise<never> {
    return new Promise(() => undefined)
  }

  disconnect(): Promise<void> {
    return Promise.resolve()
  }
}

class CancelledWebSerialClient {
  connect(): Promise<never> {
    return Promise.reject(
      new Error('浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。')
    )
  }

  disconnect(): Promise<void> {
    return Promise.resolve()
  }
}

class FakeWebSerialClient {
  private currentStatus: ControlPlaneStatus
  private heaterCurve: HeaterCurveState = {
    active: heaterCurveStoryPackage,
    preview: null,
  }
  private readonly options: FakeWebSerialClientOptions

  constructor(
    initialStatus: Partial<ControlPlaneStatus> = {},
    options: FakeWebSerialClientOptions = {}
  ) {
    this.options = options
    this.currentStatus = {
      ...status,
      ...initialStatus,
      calibration: {
        ...status.calibration,
        ...initialStatus.calibration,
        job: {
          ...status.calibration.job,
          ...initialStatus.calibration?.job,
        },
      },
      network: {
        ...status.network,
        ...initialStatus.network,
      },
    }
  }

  connect() {
    webSerialConnectCalls += 1
    return Promise.resolve({ ...webSerialProbe, status: this.currentStatus })
  }

  probe() {
    if (this.options.mutateOnProbe) {
      this.currentStatus = this.options.mutateOnProbe(this.currentStatus)
    }
    return Promise.resolve({ ...webSerialProbe, status: this.currentStatus })
  }

  configureRuntime(request: DirectRuntimeConfigRequest) {
    webSerialRuntimeWrites.push(request)
    this.currentStatus = {
      ...this.currentStatus,
      ...request,
      calibration: request.calibration
        ? {
            ...this.currentStatus.calibration,
            ...request.calibration,
          }
        : this.currentStatus.calibration,
      targetTempC:
        request.targetTempC ??
        request.presetsC?.[
          request.selectedPresetSlot ?? this.currentStatus.selectedPresetSlot ?? 0
        ] ??
        this.currentStatus.targetTempC,
      heaterOutputPercent:
        request.heaterEnabled === false ? 0 : this.currentStatus.heaterOutputPercent,
      fanDisplayState:
        request.activeCoolingEnabled === false ? 'OFF' : this.currentStatus.fanDisplayState,
      manualPpsEnabled: request.manualPpsEnabled ?? this.currentStatus.manualPpsEnabled ?? false,
      manualPpsMv:
        request.manualPpsEnabled === false
          ? null
          : (request.manualPpsMv ?? this.currentStatus.manualPpsMv ?? null),
      manualPpsMa:
        request.manualPpsEnabled === false
          ? null
          : (request.manualPpsMa ?? this.currentStatus.manualPpsMa ?? null),
      pdRequestMv:
        request.manualPpsEnabled === true && request.manualPpsMv
          ? request.manualPpsMv
          : this.currentStatus.pdRequestMv,
      pdContractMv:
        request.manualPpsEnabled === true && request.manualPpsMv
          ? request.manualPpsMv
          : this.currentStatus.pdContractMv,
    }
    return Promise.resolve(this.currentStatus satisfies ControlPlaneStatus)
  }

  getHeaterCurve() {
    return Promise.resolve(this.heaterCurve)
  }

  previewHeaterCurve(heaterCurve: HeaterCurvePackage) {
    this.heaterCurve = {
      ...this.heaterCurve,
      preview: heaterCurve,
    }
    return Promise.resolve(this.heaterCurve)
  }

  clearHeaterCurvePreview() {
    this.heaterCurve = {
      ...this.heaterCurve,
      preview: null,
    }
    return Promise.resolve(this.heaterCurve)
  }

  saveHeaterCurve() {
    if (this.heaterCurve.preview) {
      this.heaterCurve = {
        active: this.heaterCurve.preview,
        preview: null,
      }
    }
    return Promise.resolve(this.heaterCurve)
  }

  disconnect() {
    webSerialDisconnectCalls += 1
    return Promise.resolve()
  }
}

const identity = {
  deviceId: 'flux-purr-s3-001',
  firmwareVersion: '0.1.0',
  buildId: 'story-build',
  gitSha: 'story',
  board: 'esp32-s3',
  apiVersion: '2026-05-29',
  protocolVersion: 'flux-purr.usb.v1',
  hostname: 'flux-purr-s3-001',
  capabilities: ['identity', 'status', 'network', 'usb_jsonl', 'monitor'],
} satisfies Identity

const network = {
  state: 'idle',
  ssid: null,
  ip: null,
  gateway: null,
  dns: [],
  wifiRssi: null,
  lastError: null,
} satisfies NetworkSummary

const status = {
  mode: 'sampling',
  uptimeSeconds: 44,
  currentTempC: 20.3,
  targetTempC: 30,
  selectedPresetSlot: 3,
  presetsC: [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
  heaterEnabled: false,
  heaterOutputPercent: 0,
  activeCoolingEnabled: true,
  fanDisplayState: 'AUTO',
  fanEnabled: false,
  fanPwmPermille: 0,
  rtdRawAdcMv: 1120,
  vinRawAdcMv: 1670,
  voltageMv: 12_000,
  currentMa: 0,
  boardTempCenti: 2860,
  pdRequestMv: 20_000,
  pdContractMv: 12_000,
  pdState: 'ready',
  manualPpsEnabled: false,
  manualPpsMv: null,
  manualPpsMa: null,
  ppsCapabilityMinMv: 5_000,
  ppsCapabilityMaxMv: 21_000,
  ppsCapabilityMaxMa: 3_000,
  manualPpsError: null,
  heaterLockReason: null,
  calibration: idleCalibrationRuntime,
  frontpanelKey: null,
  network,
} satisfies ControlPlaneStatus

const webSerialProbe = {
  identity,
  network,
  status,
}

const lanIdentity = {
  ...identity,
  deviceId: '001122334455',
  hostname: 'flux-purr-001122334455',
} satisfies Identity

const lanPublicInfo = {
  api: 'v1',
  deviceId: lanIdentity.deviceId,
  hostname: lanIdentity.hostname,
  firmwareVersion: identity.firmwareVersion,
  pairing: { mode: 'required', active: true, attemptsRemaining: 5 },
} satisfies LanPublicInfo

const lanSession = {
  baseUrl: 'http://192.168.1.42',
  token: 'a'.repeat(64),
  hostname: lanIdentity.hostname,
} satisfies LanDeviceSession

const lanProbe = {
  identity: lanIdentity,
  network: {
    ...network,
    state: 'connected',
    ip: '192.168.1.42',
    wifiRssi: -48,
  },
  status: {
    ...status,
    network: { state: 'connected' },
  },
} satisfies LanProbe

const lanStaleWriteProbe = {
  ...lanProbe,
  status: {
    ...lanProbe.status,
    targetTempC: 150,
  },
} satisfies LanProbe

const lanRuntimeFixture: LanRuntimeDependencies = {
  createLease: async () => ({ leaseId: 'story-lan-lease', ttlMs: 30_000 }) satisfies LanLease,
  releaseLease: async () => undefined,
  startLeaseHeartbeat: () => () => undefined,
  streamEvents: async function* () {
    yield* [] as Array<Record<string, unknown>>
  },
}

let lanHeartbeatSubscriptionCount = 0
let failFirstLanHeartbeat = true
const lanHeartbeatExpiryFixture: LanRuntimeDependencies = {
  ...lanRuntimeFixture,
  startLeaseHeartbeat: (_session, _lease, onFailure) => {
    lanHeartbeatSubscriptionCount += 1
    if (failFirstLanHeartbeat) {
      failFirstLanHeartbeat = false
      onFailure(
        new ControlPlaneClientError('The LAN control lease expired.', 'lease_expired', false)
      )
    }
    return () => undefined
  },
}

let lanStaleWriteAttempts = 0
let lanStaleWriteProbeCount = 0
const lanStaleWriteFixture: LanRuntimeDependencies = {
  ...lanRuntimeFixture,
  probeDevice: async (session) => {
    lanStaleWriteProbeCount += 1
    session.controlRevision = 9
    return lanStaleWriteProbe
  },
  writeRuntime: async () => {
    lanStaleWriteAttempts += 1
    throw new ControlPlaneClientError(
      'The control state changed after this client last read it.',
      'stale_write',
      false
    )
  },
}

const lanPairingFixture = {
  initialAddress: lanSession.baseUrl,
  supported: true,
  connectDevice: async () => lanPublicInfo,
  pairDevice: async () => ({ session: lanSession, probe: lanProbe }),
  scanDevices: async () => [{ baseUrl: lanSession.baseUrl, info: lanPublicInfo }],
}

export const LiveLanScanSelection: Story = {
  name: 'Live / LAN scan connection waits for pairing',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('connecting a scan result starts the same direct LAN flow', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '开始扫描' }))
      await expect(await canvas.findByText(lanIdentity.hostname)).toBeVisible()
      await userEvent.click(
        await canvas.findByRole('button', { name: `连接 ${lanIdentity.hostname}` })
      )
      await expect(await canvas.findByLabelText('设备地址')).toHaveValue(lanSession.baseUrl)
      await expect(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' })).toBeVisible()
      await expect(
        await canvas.findByRole('button', { name: `连接 ${lanIdentity.hostname}` })
      ).toHaveAttribute('aria-pressed', 'true')
      const results = canvas.getByRole('list', { name: '发现的设备' })
      expect(getComputedStyle(results).gap).not.toBe('0px')
      await expect(canvas.getByRole('button', { name: '目标设备' })).not.toHaveTextContent(
        lanIdentity.hostname
      )
    })
  },
}

export const LiveLanScanSelectionThenConnect: Story = {
  name: 'Live / LAN scan connection completes and selects device',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
    lanRuntime: lanRuntimeFixture,
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('selecting a candidate and completing connection selects the hostname', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '开始扫描' }))
      await userEvent.click(
        await canvas.findByRole('button', { name: `连接 ${lanIdentity.hostname}` })
      )
      const dialog = within(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' }))
      await userEvent.type(await dialog.findByLabelText('四位配对码'), '4827')
      await userEvent.click(await dialog.findByRole('button', { name: '配对设备' }))

      await expect(await canvas.findByRole('button', { name: '目标设备' })).toHaveTextContent(
        lanIdentity.hostname
      )
      await expect(await canvas.findByText('LAN 设备已连接')).toBeVisible()
    })
  },
}

export const LiveLanPairingRegistersDeviceName: Story = {
  name: 'Live / LAN pairing registers device name after lease',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
    lanRuntime: lanRuntimeFixture,
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('successful pairing and lease select the hostname target', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '连接设备' }))
      const dialog = within(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' }))
      await userEvent.type(await dialog.findByLabelText('四位配对码'), '4827')
      await userEvent.click(await dialog.findByRole('button', { name: '配对设备' }))

      await expect(await canvas.findByRole('button', { name: '目标设备' })).toHaveTextContent(
        lanIdentity.hostname
      )
      await expect(await canvas.findByText('LAN 设备已连接')).toBeVisible()
    })
  },
}

export const LiveLanLeaseFailureDoesNotBindTarget: Story = {
  name: 'Live / LAN lease failure keeps current target',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
    lanRuntime: {
      ...lanRuntimeFixture,
      createLease: async () => {
        throw new Error('LAN lease conflict')
      },
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: '连接设备' }))
    const dialog = within(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(await dialog.findByLabelText('四位配对码'), '4827')
    await userEvent.click(await dialog.findByRole('button', { name: '配对设备' }))

    await expect(await canvas.findByText('LAN 租约获取失败')).toBeVisible()
    await expect(canvas.getByRole('button', { name: '目标设备' })).not.toHaveTextContent(
      lanIdentity.hostname
    )
  },
}

export const LiveLanHeartbeatExpiryRequiresExplicitReselection: Story = {
  name: 'Live / LAN heartbeat expiry requires explicit reselection',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
    lanRuntime: lanHeartbeatExpiryFixture,
  },
  play: async ({ canvasElement, step }) => {
    lanHeartbeatSubscriptionCount = 0
    failFirstLanHeartbeat = true
    const canvas = within(canvasElement)

    await step('heartbeat expiry leaves direct LAN controls read-only', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '连接设备' }))
      const dialog = within(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' }))
      await userEvent.type(await dialog.findByLabelText('四位配对码'), '4827')
      await userEvent.click(await dialog.findByRole('button', { name: '配对设备' }))

      await waitFor(() => expect(lanHeartbeatSubscriptionCount).toBe(1))
      await expect(await canvas.findByText('硬件连接受阻')).toBeVisible()
      const heartbeatFailureDetails = await canvas.findAllByText(
        'LAN lease 心跳失败：The LAN control lease expired.'
      )
      expect(heartbeatFailureDetails.some((detail) => detail.checkVisibility())).toBe(true)
      await expect(await canvas.findByLabelText('Dashboard target temperature')).toBeDisabled()
    })

    await step('only an explicit direct-LAN reselection acquires a new lease', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '目标设备' }))
      const picker = within(await canvas.findByRole('dialog', { name: '设备与连接方式' }))
      await userEvent.click(
        await picker.findByRole('button', { name: `WiFi / LAN · ${lanIdentity.hostname}` })
      )
      await expect(await canvas.findByText('LAN 设备已连接')).toBeVisible()
      await expect(await canvas.findByLabelText('Dashboard target temperature')).toBeEnabled()
    })
  },
}

export const LiveLanStaleWriteRefreshesWithoutReplay: Story = {
  name: 'Live / LAN stale write refreshes without replay',
  args: {
    initialView: 'add-device',
    lanPairing: lanPairingFixture,
    lanRuntime: lanStaleWriteFixture,
  },
  play: async ({ canvasElement, step }) => {
    lanStaleWriteAttempts = 0
    lanStaleWriteProbeCount = 0
    const canvas = within(canvasElement)

    await step(
      'a stale direct-LAN write refreshes device state without replaying the request',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: '连接设备' }))
        const dialog = within(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' }))
        await userEvent.type(await dialog.findByLabelText('四位配对码'), '4827')
        await userEvent.click(await dialog.findByRole('button', { name: '配对设备' }))
        await expect(await canvas.findByText('LAN 设备已连接')).toBeVisible()

        fireEvent.input(await canvas.findByLabelText('Dashboard target temperature'), {
          target: { value: '155' },
        })

        await expect(await canvas.findByText('LAN runtime update failed')).toBeVisible()
        await expect(
          await canvas.findByText('设备控制状态已变化，已读取最新状态；请确认后重新提交。')
        ).toBeVisible()
        await expect(await canvas.findByLabelText('Dashboard target temperature')).toHaveValue(150)
        expect(lanStaleWriteAttempts).toBe(1)
        expect(lanStaleWriteProbeCount).toBe(2)
        expect(canvas.queryByText('Target updated')).not.toBeInTheDocument()
      }
    )
  },
}

function createKnownDeviceSelectionScenario() {
  return {
    ...liveControlPlaneScenario,
    selectedDeviceId: 'live-no-target',
    devices: [
      liveControlPlaneScenario.devices[0],
      {
        id: 'serial-authorized-usb',
        alias: 'Authorized USB target',
        location: '/dev/cu.usbmodem21221401',
        transport: 'devd',
        bridgeTransport: 'usb',
        severity: 'nominal',
        baseUrl: 'devd://serial-authorized-usb',
        firmware: '0.1.0',
        buildId: 'story-devd',
        uptime: '00:00:44',
        boardTempC: 28.6,
        currentTempC: 20.3,
        targetTempC: 30,
        rtdRawAdcMv: 1120,
        vinRawAdcMv: 1670,
        voltageMv: 12_000,
        currentMa: 0,
        pdRequestMv: 20_000,
        pdContractMv: 12_000,
        pdState: 'ready',
        manualPpsEnabled: false,
        manualPpsMv: null,
        manualPpsMa: null,
        ppsCapabilityMinMv: 5_000,
        ppsCapabilityMaxMv: 21_000,
        ppsCapabilityMaxMa: 3_000,
        manualPpsError: null,
        heaterLockReason: null,
        calibration: idleCalibrationRuntime,
        heaterEnabled: false,
        heaterOutputPercent: 0,
        activeCoolingEnabled: true,
        fanState: 'AUTO',
        wifiRssi: null,
        capabilities: ['identity', 'status', 'monitor'],
        networkState: 'idle',
        leaseState: 'active',
        leaseId: 'story-lease',
      },
      {
        id: 'web-serial-browser-direct',
        alias: 'Browser Direct',
        location: 'Browser Web Serial',
        transport: 'serial',
        severity: 'nominal',
        baseUrl: 'webserial://selected',
        firmware: '0.1.0',
        buildId: 'story-serial',
        uptime: '00:00:44',
        boardTempC: 28.6,
        currentTempC: 20.3,
        targetTempC: 30,
        rtdRawAdcMv: 1120,
        vinRawAdcMv: 1670,
        voltageMv: 12_000,
        currentMa: 0,
        pdRequestMv: 20_000,
        pdContractMv: 12_000,
        pdState: 'ready',
        manualPpsEnabled: false,
        manualPpsMv: null,
        manualPpsMa: null,
        ppsCapabilityMinMv: 5_000,
        ppsCapabilityMaxMv: 21_000,
        ppsCapabilityMaxMa: 3_000,
        manualPpsError: null,
        heaterLockReason: null,
        calibration: idleCalibrationRuntime,
        heaterEnabled: false,
        heaterOutputPercent: 0,
        activeCoolingEnabled: true,
        fanState: 'AUTO',
        wifiRssi: null,
        capabilities: ['identity', 'status', 'monitor', 'usb_jsonl'],
        networkState: 'idle',
        leaseState: 'active',
      },
    ],
  } satisfies ControlPlaneScenario
}

async function verifyStoryDelay(timeoutMs: number) {
  await new Promise((resolve) => window.setTimeout(resolve, timeoutMs))
}
