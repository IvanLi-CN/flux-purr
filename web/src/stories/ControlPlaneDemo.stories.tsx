import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fireEvent, userEvent, waitFor, within } from 'storybook/test'
import { ControlPlaneDemo } from '@/features/control-plane-demo/components/control-plane-demo'
import type {
  CalibrationRuntimeState,
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
  HeaterCurvePackage,
  HeaterCurveState,
  Identity,
  NetworkSummary,
} from '@/features/control-plane-demo/contracts'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'
import type { ControlPlaneScenario } from '@/features/control-plane-demo/types'
import type { WebSerialControlPlaneClient } from '@/features/control-plane-demo/web-serial'

const meta = {
  title: 'App/ControlPlaneDemo',
  component: ControlPlaneDemo,
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
      clientFactory: () => new FakeWebSerialClient() as unknown as WebSerialControlPlaneClient,
    },
  },
} satisfies Meta<typeof ControlPlaneDemo>

export default meta
type Story = StoryObj<typeof meta>
const webSerialRuntimeWrites: DirectRuntimeConfigRequest[] = []
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
      await expect(await canvas.findByText(/0\/8 已生效/i)).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: '运行时追踪' })).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ 帧/)).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '导入预览' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeDisabled()
      await expect(await canvas.findByText('未加载预览')).toBeVisible()
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
          )?.querySelectorAll('button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual(['申请 PPS', '自动校准', '开启加热'])
        await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))
        await expect(await canvas.findByRole('slider', { name: '目标 ADC 滑块' })).toBeVisible()
        await expect(await canvas.findByRole('spinbutton', { name: '目标 ADC 输入' })).toBeVisible()
        actionButtons = Array.from(
          (
            canvasElement.querySelector(
              '.industrial-calibration-inline-actions--single-row'
            ) as HTMLElement | null
          )?.querySelectorAll('button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual(['申请 PPS', '开启加热'])
        await expect(await canvas.findByRole('heading', { name: '温度 ADC' })).toBeVisible()
        const targetAdcInput = await canvas.findByRole('spinbutton', { name: '目标 ADC 输入' })
        const referenceTempInput = await canvas.findByRole('spinbutton', { name: '参考温度' })
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
        await expect(within(rtdSampleTable).getByText('标定温度')).toBeVisible()
        await expect(within(rtdSampleTable).getByText('21.6℃')).toBeVisible()
        await expect(within(rtdSampleTable).getByText('目标 ADC')).toBeVisible()
        await expect(within(rtdSampleTable).getByText('970mV')).toBeVisible()
        await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
        await expect(await canvas.findByRole('heading', { name: '电压 ADC' })).toBeVisible()
        await expect(await canvas.findByRole('slider', { name: 'PPS 电压滑块' })).toBeVisible()
        await expect(await canvas.findByRole('spinbutton', { name: 'PPS 电压输入' })).toBeVisible()
        await expect(await canvas.findByRole('button', { name: '申请 PPS' })).toBeVisible()
        expect(canvas.queryByText('当前电流')).not.toBeInTheDocument()
        actionButtons = Array.from(
          (
            canvasElement.querySelector(
              '.industrial-calibration-inline-actions--single-row'
            ) as HTMLElement | null
          )?.querySelectorAll('button') ?? []
        ).map((button) => button.textContent?.trim())
        expect(actionButtons).toEqual(['申请 PPS', '自动校准', '开启加热'])
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
    })

    await step('voltage mode action buttons stay on one row', async () => {
      const actionRow = canvasElement.querySelector(
        '.industrial-calibration-inline-actions--single-row'
      ) as HTMLElement | null
      expect(actionRow).not.toBeNull()
      const buttons = Array.from(actionRow?.querySelectorAll('button') ?? []) as HTMLElement[]
      expect(buttons.length).toBeGreaterThanOrEqual(2)
      const topOffsets = new Set(buttons.map((button) => Math.round(button.offsetTop)))
      expect(topOffsets.size).toBe(1)
    })

    await step('voltage mode toggle actions block rapid repeat clicks', async () => {
      const modeToggle = canvasElement.querySelector('[role="switch"]') as HTMLElement | null
      expect(modeToggle).not.toBeNull()
      if (!modeToggle) {
        throw new Error('Expected calibration mode toggle to exist')
      }
      await userEvent.click(modeToggle)
      const applyPpsButton = await canvas.findByRole('button', { name: '申请 PPS' })
      const startAutoButton = await canvas.findByRole('button', { name: '自动校准' })
      await userEvent.click(applyPpsButton)
      await waitFor(() => {
        expect(applyPpsButton).toBeDisabled()
        expect(startAutoButton).toBeDisabled()
      })
    })

    await step(
      'armed calibration mode blocks page-internal tab switching until closed',
      async () => {
        const modeToggle = await canvas.findByRole('switch', { name: '标定模式' })
        const portalCanvas = within(canvasElement.ownerDocument.body)
        await waitFor(() => {
          expect(modeToggle).toHaveAttribute('aria-checked', 'true')
        })

        await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

        const leaveGuardMessage = await portalCanvas.findByText(
          '校准控制仍开着，先关闭后再切到“温度标定”。'
        )
        await expect(leaveGuardMessage).toBeVisible()
        await expect(await canvas.findByRole('tab', { name: '电压读数标定' })).toHaveAttribute(
          'data-state',
          'active'
        )
        const leaveGuard = canvasElement.ownerDocument.body.querySelector(
          '.industrial-calibration-leave-guard'
        ) as HTMLElement | null
        const liveCard = canvasElement.querySelector(
          '.industrial-calibration-live-card'
        ) as HTMLElement | null
        expect(leaveGuard).not.toBeNull()
        expect(liveCard).not.toBeNull()
        if (!leaveGuard || !liveCard) {
          throw new Error('Expected calibration leave guard and live card to exist')
        }

        const leaveGuardRect = leaveGuard.getBoundingClientRect()
        const liveCardRect = liveCard.getBoundingClientRect()
        const leaveGuardAnchor = canvasElement.querySelector(
          '.industrial-calibration-leave-guard-anchor'
        ) as HTMLElement | null
        expect(leaveGuardAnchor).not.toBeNull()
        if (!leaveGuardAnchor) {
          throw new Error('Expected calibration leave guard anchor to exist')
        }
        expect(leaveGuardAnchor.offsetWidth).toBe(0)
        expect(leaveGuardAnchor.offsetHeight).toBe(0)
        expect(leaveGuardRect.left).toBeLessThanOrEqual(liveCardRect.right)
        expect(leaveGuardRect.right).toBeGreaterThanOrEqual(liveCardRect.left)
        expect(leaveGuardRect.bottom).toBeGreaterThanOrEqual(liveCardRect.top)
        expect(leaveGuardRect.top).toBeLessThanOrEqual(liveCardRect.bottom)

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
      await expect(await canvas.findByText(/8\/8 预览/i)).toBeVisible()
      await expect(await canvas.findByRole('columnheader', { name: '预览温度' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeEnabled()
    })

    await step('save promotes preview to active curve', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: '保存曲线' }))
      await waitFor(() => {
        expect(canvas.getByText('未加载预览')).toBeVisible()
      })
      await expect(await canvas.findByRole('button', { name: '保存曲线' })).toBeDisabled()
      await expect(canvas.getByRole('table', { name: '加热曲线点表' })).toBeVisible()
    })
  },
}

export const DemoCalibrationApplyBlocked: Story = {
  name: 'Demo / 温度标定 apply blocked',
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

    await step('heater enabled blocks calibration apply before output rises', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))
      await expect(await canvas.findByRole('button', { name: '应用标定' })).toBeDisabled()
      await expect(await canvas.findByRole('heading', { name: '温度 ADC' })).toBeVisible()
    })
  },
}

export const DemoCalibrationManualFit: Story = {
  name: 'Demo / ADC draft fit',
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

    await step('manual fit controls update both draft channels', async () => {
      await expect(await canvas.findByRole('tab', { name: '温度标定' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('tab', { name: '温度标定' }))

      let gainInput = await canvas.findByRole('spinbutton', { name: /草稿增益/ })
      let offsetInput = await canvas.findByRole('spinbutton', { name: /草稿偏移/ })
      let setFitButton = await canvas.findByRole('button', { name: '设置草稿拟合' })

      await userEvent.clear(gainInput)
      await userEvent.type(gainInput, '1.01234')
      await userEvent.clear(offsetInput)
      await userEvent.type(offsetInput, '12.3')
      await userEvent.click(setFitButton)

      await userEvent.click(await canvas.findByRole('tab', { name: '电压读数标定' }))
      gainInput = await canvas.findByRole('spinbutton', { name: /草稿增益/ })
      offsetInput = await canvas.findByRole('spinbutton', { name: /草稿偏移/ })
      setFitButton = await canvas.findByRole('button', { name: '设置草稿拟合' })

      await userEvent.clear(gainInput)
      await userEvent.type(gainInput, '0.98047')
      await userEvent.clear(offsetInput)
      await userEvent.type(offsetInput, '149.8')
      await userEvent.click(setFitButton)

      await waitFor(() => {
        expect(canvas.getByText('8/8 个样本')).toBeVisible()
      })
      await expect(
        await canvas.findByText(
          /电压 ADC 草稿拟合已设为|VIN ADC draft fit set|VIN ADC 草稿拟合已设置/
        )
      ).toBeVisible()
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

      for (let index = 0; index < 8; index += 1) {
        await userEvent.click(await canvas.findByRole('button', { name: '采集样本' }))
      }

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
      for (let index = 0; index < 8; index += 1) {
        await userEvent.click(await canvas.findByRole('button', { name: '采集样本' }))
      }
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

export const LiveWebSerialAddDevice: Story = {
  name: 'Live / Web Serial Add Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    webSerialRuntimeWrites.length = 0

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
        await expect(await canvas.findByText(/flux-purr-s3-001\s*\/\s*串口/)).toBeVisible()
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
      await expect(await canvas.findByRole('button', { name: /桥接/ })).toBeVisible()
      const addDeviceRows = new Set(
        ['WiFi', 'Web Serial', '桥接'].map((name) =>
          Math.round(
            canvas.getByRole('button', { name: new RegExp(name) }).getBoundingClientRect().top
          )
        )
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

    await step('quick add WiFi switches into the add flow and triggers the action', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /WiFi/ }))

      await expect(await canvas.findByRole('heading', { name: 'Choose connection' })).toBeVisible()
      await expect(await canvas.findByText('WiFi target added')).toBeVisible()
      await expect(await canvas.findByText(/WiFi handoff is pending/)).toBeVisible()
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
    })
  },
}

export const LiveQuickAddBridgeDevice: Story = {
  name: 'Live / Quick Add Bridge Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('quick add Bridge switches into the add flow and triggers the action', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /桥接/ }))

      await expect(await canvas.findByRole('heading', { name: 'Choose connection' })).toBeVisible()
      await expect(await canvas.findByText('Native bridge added')).toBeVisible()
      await expect(
        await canvas.findByText(/native bridge target before runtime control/)
      ).toBeVisible()
      await expect(canvas.queryByRole('heading', { name: 'Runtime trace' })).not.toBeInTheDocument()
    })

    await step(
      'connecting Web Serial from the pending Bridge flow selects the hardware target',
      async () => {
        await userEvent.click(await canvas.findByRole('button', { name: /Web Serial/ }))

        await waitFor(() => {
          expect(canvas.getByRole('heading', { name: 'Thermal runtime' })).toBeVisible()
        })
        await expect(await canvas.findByText(/flux-purr-s3-001\s*\/\s*串口/)).toBeVisible()
        await expect(
          canvas.queryByText(/Native bridge \/ BRIDGE|本机桥接 \/ 桥接/)
        ).not.toBeInTheDocument()
        await expect(await canvas.findByText('Web Serial connected')).toBeVisible()
      }
    )
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
    'calibration_config response payload includes active and draft ADC fits, eight persisted sample slots, raw observed millivolts, reference targets, and operator feedback metadata for the current lease'

  return {
    ...controlPlaneScenario,
    devices: controlPlaneScenario.devices.map((device) =>
      device.id === controlPlaneScenario.selectedDeviceId
        ? { ...device, heaterOutputPercent: 0, currentTempC: 183.6, voltageMv: 20_010 }
        : { ...device, heaterOutputPercent: 0 }
    ),
    events: controlPlaneScenario.events.map((event, index) => ({
      ...event,
      detail: index % 2 === 0 ? longTraceDetail : event.detail,
      message:
        index % 3 === 0
          ? `${event.message}; calibration draft and event stream remained bounded after dense operator sampling`
          : event.message,
    })),
  }
}

type FakeWebSerialClientOptions = {
  mutateOnProbe?: (currentStatus: ControlPlaneStatus) => ControlPlaneStatus
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
