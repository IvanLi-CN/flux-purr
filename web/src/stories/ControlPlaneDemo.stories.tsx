import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fireEvent, userEvent, waitFor, within } from 'storybook/test'
import { ControlPlaneDemo } from '@/features/control-plane-demo/components/control-plane-demo'
import type {
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
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
  name: 'Demo / Calibration tab',
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

    await step('calibration panels are visible', async () => {
      await expect(await canvas.findByRole('heading', { name: 'ADC trim' })).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: 'RTD ADC' })).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: 'VIN ADC' })).toBeVisible()
    })

    await step('capture creates a draft sample', async () => {
      await userEvent.click((await canvas.findAllByRole('button', { name: 'Capture sample' }))[0])
      await waitFor(() => {
        expect(canvas.getAllByText(/sample captured/i).length).toBeGreaterThan(0)
      })
      await expect(await canvas.findByText(/1\/8 samples/i)).toBeVisible()
    })
  },
}

export const DemoCalibrationApplyBlocked: Story = {
  name: 'Demo / Calibration apply blocked',
  args: {
    scenario: {
      ...controlPlaneScenario,
      selectedDeviceId: 'fp-kit-02',
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

    await step('heater output blocks calibration apply', async () => {
      await expect(await canvas.findByRole('heading', { name: 'ADC trim' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: 'Apply calibration' })).toBeDisabled()
      await expect(
        await canvas.findByText('Apply is blocked while heater output is active.')
      ).toBeVisible()
    })
  },
}

export const DemoCalibrationManualFit: Story = {
  name: 'Demo / Calibration manual fit',
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
      await expect(await canvas.findByRole('heading', { name: 'ADC trim' })).toBeVisible()

      const gainInputs = await canvas.findAllByRole('spinbutton', { name: /Draft gain/ })
      const offsetInputs = await canvas.findAllByRole('spinbutton', { name: /Draft offset/ })
      const setFitButtons = await canvas.findAllByRole('button', { name: 'Set draft fit' })

      await userEvent.clear(gainInputs[0])
      await userEvent.type(gainInputs[0], '1.01234')
      await userEvent.clear(offsetInputs[0])
      await userEvent.type(offsetInputs[0], '12.3')
      await userEvent.click(setFitButtons[0])

      await userEvent.clear(gainInputs[1])
      await userEvent.type(gainInputs[1], '0.98047')
      await userEvent.clear(offsetInputs[1])
      await userEvent.type(offsetInputs[1], '149.8')
      await userEvent.click(setFitButtons[1])

      await waitFor(() => {
        expect(canvas.getAllByText('8/8 samples')).toHaveLength(2)
      })
      await expect(await canvas.findByText(/VIN ADC draft fit set/)).toBeVisible()
    })
  },
}

export const DemoCalibrationDenseLists: Story = {
  name: 'Demo / Calibration dense lists',
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
      await expect(await canvas.findByRole('heading', { name: 'ADC trim' })).toBeVisible()

      const captureButtons = await canvas.findAllByRole('button', { name: 'Capture sample' })
      for (let index = 0; index < 8; index += 1) {
        await userEvent.click(captureButtons[0])
        await userEvent.click(captureButtons[1])
      }

      await waitFor(() => {
        expect(canvas.getAllByText('8/8 samples')).toHaveLength(2)
      })

      const rtdList = await canvas.findByRole('region', { name: 'RTD ADC sample list' })
      const vinList = await canvas.findByRole('region', { name: 'VIN ADC sample list' })
      rtdList.scrollTop = rtdList.scrollHeight
      vinList.scrollTop = vinList.scrollHeight
      fireEvent.scroll(rtdList)
      fireEvent.scroll(vinList)

      const logScroller = canvasElement.querySelector<HTMLElement>(
        '.industrial-log-panel__rows .simplebar-content-wrapper'
      )
      if (!logScroller) {
        throw new Error('Log scroller was not found.')
      }
      logScroller.scrollTop = 900
      fireEvent.scroll(logScroller)

      await expect(
        within(rtdList).getByRole('button', { name: 'Delete RTD ADC sample 8' })
      ).toBeVisible()
      await expect(
        within(vinList).getByRole('button', { name: 'Delete VIN ADC sample 8' })
      ).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ frames/)).toBeVisible()
      await waitFor(() => {
        expect(canvas.getAllByText(/calibration_config response payload/).length).toBeGreaterThan(0)
      })
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
      const addDeviceButtons = ['WiFi', 'Web Serial', 'Bridge'].map((name) =>
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
        await expect(await canvas.findByText('flux-purr-s3-001 / SERIAL')).toBeVisible()
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
      expect(webSerialRuntimeWrites.at(-1)?.manualPpsMa).toBe(3_000)
      await expect(await canvas.findByText(/Manual 10.4V \/ 3.00A/)).toBeVisible()
      await userEvent.click(await canvas.findByRole('button', { name: 'Clear' }))
      await waitFor(() => {
        expect(webSerialRuntimeWrites.at(-1)?.manualPpsEnabled).toBe(false)
      })
    })

    await step('global log remains expanded after switching to Settings', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /Settings/ }))

      await expect(await canvas.findByRole('heading', { name: 'Heat policy' })).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: 'Runtime trace' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: 'All' })).toBeVisible()
      await expect(await canvas.findByRole('button', { name: 'Ok' })).toBeVisible()
      await userEvent.click(await canvas.findByRole('button', { name: 'Ok' }))
      await expect(await canvas.findByRole('button', { name: 'Ok' })).toHaveAttribute(
        'aria-pressed',
        'true'
      )
      await userEvent.click(await canvas.findByRole('button', { name: 'All' }))
      await expect(
        await canvas.findByText(
          'flux-purr-s3-001 USB JSONL probe accepted: get_identity / get_network / get_status'
        )
      ).toBeVisible()
      await expect(await canvas.findByText(/\d+ \/ \d+ frames/)).toBeVisible()
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
      await expect(await canvas.findByRole('button', { name: /Bridge/ })).toBeVisible()
      const addDeviceRows = new Set(
        ['WiFi', 'Web Serial', 'Bridge'].map((name) =>
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
      await userEvent.click(await canvas.findByRole('button', { name: /Bridge/ }))

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
        await expect(await canvas.findByText('flux-purr-s3-001 / SERIAL')).toBeVisible()
        await expect(canvas.queryByText('Native bridge / BRIDGE')).not.toBeInTheDocument()
        await expect(await canvas.findByText('Web Serial connected')).toBeVisible()
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

class FakeWebSerialClient {
  private currentStatus: ControlPlaneStatus = status

  connect() {
    return Promise.resolve({ ...webSerialProbe, status: this.currentStatus })
  }

  probe() {
    return Promise.resolve({ ...webSerialProbe, status: this.currentStatus })
  }

  configureRuntime(request: DirectRuntimeConfigRequest) {
    webSerialRuntimeWrites.push(request)
    this.currentStatus = {
      ...this.currentStatus,
      ...request,
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
