import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, within } from 'storybook/test'
import {
  DeviceTargetPicker,
  DeviceToolbar,
} from '@/features/control-plane-demo/components/control-plane-demo'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'

const meta = {
  title: 'Components/ControlPlaneToolbar',
  component: DeviceToolbar,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <div
        style={{
          width: 'min(1100px, calc(100vw - 48px))',
          margin: '24px',
          padding: '20px',
          background: '#c6d4df',
        }}
      >
        <Story />
      </div>
    ),
  ],
  args: {
    devices: controlPlaneScenario.devices,
    device: controlPlaneScenario.devices[0],
    onDeviceChange: () => undefined,
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
  },
}

export const Unsupported: Story = {
  args: {},
}

export const LanDeviceName: Story = {
  args: {
    devices: [
      {
        ...controlPlaneScenario.devices[0],
        id: 'lan-001122334455',
        alias: 'flux-purr-001122334455',
        location: '192.168.1.42',
        transport: 'wifi',
        baseUrl: 'http://192.168.1.42',
      },
    ],
    device: {
      ...controlPlaneScenario.devices[0],
      id: 'lan-001122334455',
      alias: 'flux-purr-001122334455',
      location: '192.168.1.42',
      transport: 'wifi',
      baseUrl: 'http://192.168.1.42',
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const trigger = canvas.getByRole('button', { name: '目标设备' })
    await expect(trigger).toHaveTextContent('flux-purr-001122334455')
    await userEvent.click(trigger)
    const option = within(canvasElement.ownerDocument.body).getByRole('dialog', {
      name: '设备与连接方式',
    })
    await expect(option).toHaveTextContent('WiFi')
    await expect(option).toHaveTextContent('192.168.1.42')
  },
}

export const NativeFirmwareName: Story = {
  args: {
    devices: [
      {
        ...controlPlaneScenario.devices[0],
        id: 'native-a0f262f20d6c',
        alias: 'flux-purr-a0f262f20d6c',
        location: '/dev/cu.usbmodem2111401',
        transport: 'devd',
        baseUrl: 'devd://native-a0f262f20d6c',
      },
    ],
    device: {
      ...controlPlaneScenario.devices[0],
      id: 'native-a0f262f20d6c',
      alias: 'flux-purr-a0f262f20d6c',
      location: '/dev/cu.usbmodem2111401',
      transport: 'devd',
      baseUrl: 'devd://native-a0f262f20d6c',
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const trigger = canvas.getByRole('button', { name: '目标设备' })
    await expect(trigger).toHaveTextContent('flux-purr-a0f262f20d6c')
    await expect(trigger).not.toHaveTextContent('USB JTAG/serial debug unit')
    await expect(trigger.querySelector('.industrial-device-select-value')).not.toBeNull()
  },
}

export const MergedDeviceConnections: Story = {
  render: ({ devices, device, onDeviceChange }) => (
    <div
      className="merged-device-picker-story"
      style={{
        width: 'min(760px, calc(100vw - 48px))',
        minHeight: '400px',
        margin: '24px',
        padding: '20px 44px 20px 32px',
        background: '#94aabb',
      }}
    >
      <DeviceTargetPicker devices={devices} device={device} onDeviceChange={onDeviceChange} />
    </div>
  ),
  args: {
    devices: [
      {
        ...controlPlaneScenario.devices[0],
        id: 'native-device-1',
        identityId: 'device-1',
        alias: 'flux-purr-device-1',
        location: '/dev/cu.usbmodem2111401',
        transport: 'devd',
        bridgeTransport: 'usb',
        baseUrl: 'devd://native-device-1',
      },
      {
        ...controlPlaneScenario.devices[0],
        id: 'lan-device-1',
        identityId: 'device-1',
        alias: 'flux-purr-device-1',
        location: '192.168.1.42',
        transport: 'wifi',
        baseUrl: 'http://192.168.1.42',
      },
      {
        ...controlPlaneScenario.devices[0],
        id: 'web-serial-device-1',
        identityId: 'device-1',
        alias: 'flux-purr-device-1',
        location: 'Browser Web Serial',
        transport: 'serial',
        baseUrl: 'webserial://device-1',
      },
      {
        ...controlPlaneScenario.devices[0],
        id: 'bridge-lan-device-1',
        identityId: 'device-1',
        alias: 'flux-purr-device-1',
        location: '192.168.1.42',
        transport: 'devd',
        bridgeTransport: 'wifi',
        baseUrl: 'devd://bridge-lan-device-1',
      },
      {
        ...controlPlaneScenario.devices[0],
        id: 'web-serial-device-2',
        identityId: 'device-2',
        alias: 'flux-purr-device-2',
        location: 'Browser Web Serial',
        transport: 'serial',
        baseUrl: 'webserial://device-2',
      },
    ],
    device: {
      ...controlPlaneScenario.devices[0],
      id: 'native-device-1',
      identityId: 'device-1',
      alias: 'flux-purr-device-1',
      location: '/dev/cu.usbmodem2111401',
      transport: 'devd',
      bridgeTransport: 'usb',
      baseUrl: 'devd://native-device-1',
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const trigger = canvas.getByRole('button', { name: '目标设备' })
    await userEvent.click(trigger)
    const dialog = within(canvasElement.ownerDocument.body).getByRole('dialog', {
      name: '设备与连接方式',
    })
    await expect(dialog.querySelectorAll('[data-device-id="device-1"]')).toHaveLength(1)
    await expect(dialog).toHaveTextContent('flux-purr-device-1')
    await expect(dialog).toHaveTextContent('设备 ID · device-1')
    await expect(dialog).toHaveTextContent('桥接')
    await expect(dialog).toHaveTextContent('WiFi / LAN')
    await expect(dialog).toHaveTextContent('Web Serial')
    const connectionButtons = dialog.querySelectorAll('button[aria-label*="flux-purr-device-1"]')
    await expect(connectionButtons).toHaveLength(4)
    await expect(
      new Set(Array.from(connectionButtons, (button) => button.getAttribute('aria-label'))).size
    ).toBe(4)
    await expect(
      within(dialog).getByRole('button', {
        name: '桥接 · USB · /dev/cu.usbmodem2111401 · flux-purr-device-1',
      })
    ).toBeInTheDocument()
    await expect(
      within(dialog).getByRole('button', {
        name: '桥接 · WiFi / LAN · 192.168.1.42 · flux-purr-device-1',
      })
    ).toBeInTheDocument()
    await expect(
      dialog.querySelectorAll('.industrial-device-connection-button__icon')
    ).toHaveLength(5)
    await expect(dialog.querySelectorAll('.industrial-device-picker__add > svg')).toHaveLength(1)
    await expect(dialog).not.toHaveTextContent('桥接 · USB')
    await expect(dialog).not.toHaveTextContent('桥接 · WiFi / LAN')

    const triggerStyle = getComputedStyle(trigger)
    await expect(triggerStyle.backgroundImage).toBe('none')
    await expect(triggerStyle.paddingRight).toBe(triggerStyle.paddingLeft)
    await expect(trigger.querySelectorAll(':scope > svg')).toHaveLength(1)

    const singleConnectionCard = dialog.querySelector('[data-device-id="device-2"]')
    const connectionGrid = singleConnectionCard?.querySelector(
      '.industrial-device-choice-card__connections'
    )
    const connectionButton = connectionGrid?.querySelector('.industrial-device-connection-button')
    expect(connectionGrid).not.toBeNull()
    expect(connectionButton).not.toBeNull()
    const gridWidth = connectionGrid?.getBoundingClientRect().width ?? 0
    const buttonWidth = connectionButton?.getBoundingClientRect().width ?? 0
    expect(buttonWidth / gridWidth).toBeGreaterThan(0.3)
    expect(buttonWidth / gridWidth).toBeLessThan(0.36)

    for (const button of dialog.querySelectorAll<HTMLElement>(
      '.industrial-device-connection-button'
    )) {
      const icon = button.querySelector<HTMLElement>('.industrial-device-connection-button__icon')
      const copy = button.querySelector<HTMLElement>(':scope > span')
      const arrow = button.querySelector<HTMLElement>('.industrial-device-connection-button__arrow')
      expect(icon).not.toBeNull()
      expect(copy).not.toBeNull()
      expect(arrow).not.toBeNull()
      if (!icon || !copy || !arrow) continue

      const iconGap = copy.getBoundingClientRect().left - icon.getBoundingClientRect().right
      const arrowGap = arrow.getBoundingClientRect().left - copy.getBoundingClientRect().right
      expect(Math.abs(iconGap - arrowGap)).toBeLessThanOrEqual(1)
    }
  },
}
