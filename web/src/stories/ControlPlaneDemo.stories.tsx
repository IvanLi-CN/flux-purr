import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, within } from 'storybook/test'
import type { ControlPlaneScenario } from '@/features/control-plane-demo'
import {
  ControlPlaneDemo,
  controlPlaneScenario,
  degradedControlPlaneScenario,
} from '@/features/control-plane-demo'

const serialTimeoutScenario: ControlPlaneScenario = {
  ...controlPlaneScenario,
  name: 'Native serial timeout',
  selectedDeviceId: 'fp-devd-timeout',
  devices: [
    {
      ...controlPlaneScenario.devices[0],
      id: 'fp-devd-timeout',
      alias: 'Authorized USB target',
      location: '/dev/cu.usbmodem21221401',
      transport: 'devd',
      severity: 'warning',
      baseUrl: 'devd://serial-303a-1001-D0:CF:13:08:A1:48',
      networkState: 'timeout',
      leaseState: 'active',
      leaseId: 'lease-timeout',
      transportIssue: 'Timed out waiting for a matching USB JSONL response.',
      capabilities: ['identity', 'status', 'network', 'wifi_config', 'monitor', 'flash'],
    },
  ],
  metrics: controlPlaneScenario.metrics.map((metric) =>
    metric.label === 'Bound targets'
      ? {
          ...metric,
          value: '01',
          detail: 'native serial timeout',
          tone: 'warning' as const,
        }
      : metric
  ),
  events: [
    {
      time: '20:18:04',
      source: 'serial',
      message: 'native serial RPC failed: identity usb_response_timeout',
      tone: 'danger',
    },
    ...controlPlaneScenario.events,
  ],
}

const devdTraceScenario: ControlPlaneScenario = {
  ...controlPlaneScenario,
  name: 'Runtime trace with devd events',
  selectedDeviceId: 'fp-devd-trace',
  devices: [
    {
      ...controlPlaneScenario.devices[0],
      id: 'fp-devd-trace',
      alias: 'Authorized USB target',
      location: '/dev/cu.usbmodem21221401',
      transport: 'devd',
      severity: 'warning',
      baseUrl: 'devd://serial-303a-1001-D0:CF:13:08:A1:48',
      networkState: 'timeout',
      leaseState: 'active',
      leaseId: 'lease-devd-trace',
      transportIssue: 'Native serial bridge reported bounded daemon events.',
      capabilities: ['identity', 'status', 'network', 'wifi_config', 'monitor', 'flash'],
    },
  ],
  metrics: controlPlaneScenario.metrics.map((metric) =>
    metric.label === 'Trace buffer'
      ? {
          ...metric,
          value: '80',
          detail: 'devd event summaries',
          tone: 'accent' as const,
        }
      : metric
  ),
  events: [
    {
      time: '20:24:02',
      source: 'serial',
      message: 'native serial RPC failed: identity / usb_response_timeout',
      tone: 'danger',
    },
    {
      time: '20:24:05',
      source: 'flash',
      message: 'artifact dry-run passed: verify / local-esp32s3-release',
      tone: 'success',
    },
    {
      time: '20:24:09',
      source: 'flash',
      message: 'real flash blocked: guard / real_flash_disabled',
      tone: 'success',
    },
    {
      time: '20:24:11',
      source: 'lease',
      message: 'lease heartbeat renewed',
      tone: 'info',
    },
  ],
}

const meta = {
  title: 'Pages/ControlPlaneDemo',
  component: ControlPlaneDemo,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    docs: {
      description: {
        component:
          'Runtime control-plane shell for thermal control, WiFi provisioning, devd artifact checks, and bounded trace review.',
      },
    },
  },
} satisfies Meta<typeof ControlPlaneDemo>

export default meta
type Story = StoryObj<typeof meta>

export const Dashboard: Story = {
  args: {
    scenario: controlPlaneScenario,
    initialView: 'dashboard',
  },
}

export const WifiProvisioning: Story = {
  args: {
    scenario: controlPlaneScenario,
    initialView: 'wifi',
  },
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)

    await step('provision mock WiFi credentials', async () => {
      await userEvent.clear(canvas.getByRole('textbox', { name: /ssid/i }))
      await userEvent.type(canvas.getByRole('textbox', { name: /ssid/i }), 'FluxPurr-Fixture')
      await userEvent.type(canvas.getByLabelText(/password/i), 'secret-pass')
      await userEvent.click(canvas.getByRole('button', { name: /provision/i }))

      await expect(canvas.findByText(/WiFi provisioned/i)).resolves.toBeInTheDocument()
      await expect(
        canvas.findByText(/Bench Alpha Fixture stored FluxPurr-Fixture\./i)
      ).resolves.toBeInTheDocument()
    })

    await step('clear mock WiFi credentials', async () => {
      await userEvent.click(canvas.getByRole('button', { name: /^clear$/i }))
      await expect(canvas.findByText(/WiFi cleared/i)).resolves.toBeInTheDocument()
    })
  },
}

export const WifiLeaseBlocked: Story = {
  args: {
    scenario: degradedControlPlaneScenario,
    initialView: 'wifi',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    await expect(canvas.findByText(/Provisioning blocked/i)).resolves.toBeInTheDocument()
    await expect(canvas.findByText(/USB lease is not available/i)).resolves.toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: /provision/i })).toBeDisabled()
  },
}

export const NativeSerialTimeout: Story = {
  args: {
    scenario: serialTimeoutScenario,
    initialView: 'wifi',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    await expect(canvas.findByText(/Timed out waiting/i)).resolves.toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: /provision/i })).toBeDisabled()
    await expect(canvas.getByRole('button', { name: /^clear$/i })).toBeDisabled()
  },
}

export const RuntimeTraceDevdEvents: Story = {
  args: {
    scenario: devdTraceScenario,
    initialView: 'dashboard',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    await expect(canvas.findAllByText(/native serial RPC failed/i)).resolves.not.toHaveLength(0)
    await expect(canvas.findAllByText(/artifact dry-run passed/i)).resolves.not.toHaveLength(0)
    await expect(canvas.findAllByText(/real flash blocked/i)).resolves.not.toHaveLength(0)
  },
}

export const DocsGallery: Story = {
  name: 'Docs / Gallery',
  render: () => (
    <div style={{ display: 'grid', gap: 24, background: '#e0e5ec', padding: 24 }}>
      <ControlPlaneDemo scenario={controlPlaneScenario} initialView="wifi" />
      <ControlPlaneDemo scenario={degradedControlPlaneScenario} initialView="wifi" />
      <ControlPlaneDemo scenario={serialTimeoutScenario} initialView="wifi" />
      <ControlPlaneDemo scenario={devdTraceScenario} initialView="dashboard" />
    </div>
  ),
}
