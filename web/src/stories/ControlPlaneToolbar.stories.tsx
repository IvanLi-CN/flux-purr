import type { Meta, StoryObj } from '@storybook/react-vite'
import { DeviceToolbar } from '@/features/control-plane-demo/components/control-plane-demo'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'

const meta = {
  title: 'Components/ControlPlaneToolbar',
  component: DeviceToolbar,
  args: {
    devices: controlPlaneScenario.devices,
    device: controlPlaneScenario.devices[0],
    showDegraded: false,
    allowDegradedMode: true,
    webSerial: {
      state: 'idle',
      supported: true,
    },
    onDeviceChange: () => undefined,
    onToggleDegraded: () => undefined,
    onWebSerialConnect: () => undefined,
  },
} satisfies Meta<typeof DeviceToolbar>

export default meta
type Story = StoryObj<typeof meta>

export const WebSerialReady: Story = {}

export const WebSerialConnected: Story = {
  args: {
    device: {
      ...controlPlaneScenario.devices[1],
      id: 'web-serial-flux-purr-s3-001',
      alias: 'flux-purr-s3-001',
      baseUrl: 'webserial://selected',
      leaseState: 'active',
      capabilities: ['identity', 'status', 'network', 'usb_jsonl', 'monitor'],
    },
    webSerial: {
      state: 'connected',
      supported: true,
    },
  },
}

export const Unsupported: Story = {
  args: {
    webSerial: {
      state: 'unsupported',
      supported: false,
    },
  },
}
