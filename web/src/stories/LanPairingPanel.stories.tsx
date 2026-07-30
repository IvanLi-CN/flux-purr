import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, within } from 'storybook/test'
import { LanPairingPanel } from '@/features/control-plane-demo/components/lan-pairing-panel'

const mockPairing = async (baseUrl: string) => ({
  session: { baseUrl, token: 'a'.repeat(64), hostname: 'flux-purr-001122334455' },
  probe: {
    identity: {
      deviceId: '001122334455',
      firmwareVersion: 'fw/v0.4.0',
      buildId: 'demo',
      gitSha: 'demo',
      board: 'esp32s3',
      apiVersion: 'v1',
      protocolVersion: 'usb.v1',
      hostname: 'flux-purr-001122334455',
      capabilities: [],
    },
    network: { state: 'connected' as const },
    status: {
      mode: 'idle' as const,
      uptimeSeconds: 0,
      currentTempC: 25,
      targetTempC: 120,
      heaterEnabled: false,
      heaterOutputPercent: 0,
      activeCoolingEnabled: true,
      fanDisplayState: 'AUTO' as const,
      fanEnabled: true,
      fanPwmPermille: 400,
      voltageMv: 20000,
      currentMa: 0,
      boardTempCenti: 2500,
      pdRequestMv: 20000,
      pdContractMv: 20000,
      pdState: 'ready' as const,
      calibration: {
        mode: 'off' as const,
        ppsEnabled: false,
        heaterEnabled: false,
        stable: false,
        job: { status: 'idle' as const, progressPercent: 0, samplesCollected: 0 },
      },
      network: { state: 'connected' as const },
    },
  },
})

const meta = {
  title: 'App/LanPairingPanel',
  component: LanPairingPanel,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
  args: { supported: true },
} satisfies Meta<typeof LanPairingPanel>

export default meta
type Story = StoryObj<typeof meta>

export const ChromiumPairing: Story = {
  args: { pairDevice: mockPairing },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const addressInput = canvas.getByLabelText('设备地址')
    const codeInput = canvas.getByLabelText('四位配对码')
    const pairButton = canvas.getByRole('button', { name: '配对设备' })

    expect(pairButton.getBoundingClientRect().height).toBe(
      addressInput.getBoundingClientRect().height
    )
    expect(codeInput.getBoundingClientRect().height).toBe(
      addressInput.getBoundingClientRect().height
    )

    await userEvent.type(codeInput, '4827')
    await expect(pairButton).toBeEnabled()
    await userEvent.click(pairButton)
    await expect(canvas.getByText('已连接 flux-purr-001122334455')).toBeVisible()
  },
}

export const SafariUnsupported: Story = {
  args: { supported: false },
}
