import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, waitFor, within } from 'storybook/test'
import { ControlPlaneDemo } from '@/features/control-plane-demo/components/control-plane-demo'
import type {
  ControlPlaneStatus,
  DirectRuntimeConfigRequest,
  Identity,
  NetworkSummary,
} from '@/features/control-plane-demo/contracts'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
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

export const LiveWebSerialAddDevice: Story = {
  name: 'Live / Web Serial Add Device',
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

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

    await step('global log remains expanded after switching to Settings', async () => {
      await userEvent.click(await canvas.findByRole('button', { name: /Settings/ }))

      await expect(await canvas.findByRole('heading', { name: 'Heat policy' })).toBeVisible()
      await expect(await canvas.findByRole('heading', { name: 'Runtime trace' })).toBeVisible()
      await expect(
        await canvas.findByText(
          'flux-purr-s3-001 USB JSONL probe accepted: get_identity / get_network / get_status'
        )
      ).toBeVisible()
      await expect(canvas.queryByText(/\d+ frames/)).not.toBeInTheDocument()
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

class FakeWebSerialClient {
  connect() {
    return Promise.resolve(webSerialProbe)
  }

  probe() {
    return Promise.resolve(webSerialProbe)
  }

  configureRuntime(request: DirectRuntimeConfigRequest) {
    return Promise.resolve({
      ...status,
      ...request,
      heaterOutputPercent: request.heaterEnabled === false ? 0 : status.heaterOutputPercent,
      fanDisplayState: request.activeCoolingEnabled === false ? 'OFF' : status.fanDisplayState,
    } satisfies ControlPlaneStatus)
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
  apiVersion: '2026-05-23',
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
