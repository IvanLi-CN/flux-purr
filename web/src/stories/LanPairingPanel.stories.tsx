import type { Meta, StoryObj } from '@storybook/react-vite'
import type { ComponentProps } from 'react'
import { expect, fn, userEvent, within } from 'storybook/test'
import { LanPairingPanel } from '@/features/control-plane-demo/components/lan-pairing-panel'
import type { LanPublicInfo } from '@/features/control-plane-demo/lan-client'
import { ControlPlaneClientError } from '@/features/control-plane-demo/transport-client'

const requiredActive: LanPublicInfo = {
  api: 'v1',
  deviceId: '001122334455',
  hostname: 'flux-purr-001122334455',
  firmwareVersion: 'fw/v0.4.0',
  pairing: { mode: 'required', active: true, attemptsRemaining: 5 },
}

const requiredInactive: LanPublicInfo = {
  ...requiredActive,
  pairing: { mode: 'required', active: false, attemptsRemaining: 5 },
}

const optionalPairing: LanPublicInfo = {
  ...requiredActive,
  pairing: { mode: 'optional', active: false, attemptsRemaining: 5 },
}

const unavailablePairing: LanPublicInfo = {
  ...requiredActive,
  pairing: { mode: 'unavailable', active: false, attemptsRemaining: 0 },
}

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

let resolveEnterConnect: ((info: LanPublicInfo) => void) | undefined
let resolveEnterScan:
  | ((devices: Array<{ baseUrl: string; info: LanPublicInfo }>) => void)
  | undefined

const enterConnectDevice = fn(
  () =>
    new Promise<LanPublicInfo>((resolve) => {
      resolveEnterConnect = resolve
    })
)
const enterScanDevices = fn(
  () =>
    new Promise<Array<{ baseUrl: string; info: LanPublicInfo }>>((resolve) => {
      resolveEnterScan = resolve
    })
)

const noSavedLanSession: NonNullable<
  ComponentProps<typeof LanPairingPanel>['resumeSession']
> = async () => null

const meta = {
  title: 'App/LanPairingPanel',
  component: LanPairingPanel,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
  decorators: [
    (Story) => (
      <div
        className="industrial-shell industrial-lan-pairing-story-surface"
        data-testid="lan-pairing-story-surface"
        style={{
          display: 'grid',
          width: 'min(100vw - 2rem, 54rem)',
          border: 0,
          background: '#a8cbd2',
          boxShadow: 'none',
          padding: '16px 17px 17px',
        }}
      >
        <div style={{ width: '100%' }}>
          <Story />
        </div>
      </div>
    ),
  ],
  args: {
    initialAddress: 'http://192.168.1.18',
    supported: true,
    resumeSession: noSavedLanSession,
  },
} satisfies Meta<typeof LanPairingPanel>

export default meta
type Story = StoryObj<typeof meta>

export const RequiredPairing: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: mockPairing,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const addressInput = canvas.getByLabelText('设备地址')
    const connectButton = canvas.getByRole('button', { name: '连接设备' })

    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
    expect(connectButton.getBoundingClientRect().height).toBe(
      addressInput.getBoundingClientRect().height
    )

    await userEvent.click(connectButton)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    const codeInput = dialog.getByLabelText('四位配对码')
    const pairButton = dialog.getByRole('button', { name: '配对设备' })
    expect(pairButton.getBoundingClientRect().height).toBe(codeInput.getBoundingClientRect().height)
    await userEvent.type(codeInput, '4827')
    await expect(pairButton).toBeEnabled()
    await userEvent.click(pairButton)
    await expect(canvas.getByText('已配对 flux-purr-001122334455，正在获取控制租约')).toBeVisible()
  },
}

export const RestoresSavedPairing: Story = {
  args: {
    connectDevice: async () => requiredActive,
    resumeSession: async (baseUrl, _health) => mockPairing(baseUrl),
    onPaired: fn(),
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))

    await expect(
      canvas.getByText('已验证已保存的配对凭据，正在获取 flux-purr-001122334455 控制租约')
    ).toBeVisible()
    expect(canvas.queryByRole('dialog', { name: '输入 LAN 配对码' })).toBeNull()
    await expect(args.onPaired).toHaveBeenCalledTimes(1)
  },
}

export const LeaseFailureDoesNotLookLikeProbeFailure: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: mockPairing,
    onPaired: async () => {
      throw new Error('LAN lease conflict')
    },
  },
  play: async ({ canvasElement }) => {
    await connectRequired(canvasElement)
    const canvas = within(canvasElement)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(dialog.getByLabelText('四位配对码'), '4827')
    await userEvent.click(dialog.getByRole('button', { name: '配对设备' }))
    await expect(
      canvas.getByText('设备已配对，但控制租约获取失败。请检查其他客户端后重试。')
    ).toBeVisible()
    expect(
      canvas.queryByText('配对凭据已保存，但读取设备状态时连接中断。请重新连接设备。')
    ).toBeNull()
  },
}

export const RestoresBrowserPreferences: Story = {
  args: { initialAddress: '', supported: true },
  render: (args) => {
    if (typeof window !== 'undefined') {
      window.localStorage.setItem('flux-purr:lan-address', 'http://192.168.31.118')
      window.localStorage.setItem('flux-purr:lan-scan-cidr', '192.168.31.0/24')
    }
    return <LanPairingPanel {...args} />
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByLabelText('设备地址')).toHaveValue('http://192.168.31.118')
    await expect(canvas.getByLabelText('CIDR 网段')).toHaveValue('192.168.31.0/24')

    window.localStorage.removeItem('flux-purr:lan-address')
    window.localStorage.removeItem('flux-purr:lan-scan-cidr')
  },
}

export const EnterSubmitsDeviceAddress: Story = {
  args: {
    connectDevice: enterConnectDevice,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: mockPairing,
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement)
    const addressInput = canvas.getByLabelText('设备地址')
    const connectButton = canvas.getByRole('button', { name: '连接设备' })
    await userEvent.click(addressInput)
    await userEvent.keyboard('{Enter}')
    await userEvent.keyboard('{Enter}')
    await expect(args.connectDevice).toHaveBeenCalledTimes(1)
    await expect(connectButton).toBeDisabled()
    resolveEnterConnect?.(requiredActive)
    await expect(await canvas.findByRole('dialog', { name: '输入 LAN 配对码' })).toBeVisible()
  },
}

export const EnterSubmitsPairingCode: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: fn(mockPairing),
  },
  play: async ({ args, canvasElement }) => {
    await connectRequired(canvasElement)
    const canvas = within(canvasElement)
    const codeInput = canvas.getByLabelText('四位配对码')
    await userEvent.type(codeInput, '482')
    await userEvent.keyboard('{Enter}')
    await expect(args.pairDevice).not.toHaveBeenCalled()

    await userEvent.type(codeInput, '7')
    await userEvent.keyboard('{Enter}')
    await expect(args.pairDevice).toHaveBeenCalledTimes(1)
    await expect(
      await canvas.findByText('已配对 flux-purr-001122334455，正在获取控制租约')
    ).toBeVisible()
  },
}

export const EnterPairingCodeLoadingBlocksRepeat: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: fn(() => new Promise<never>(() => undefined)),
  },
  play: async ({ args, canvasElement }) => {
    await connectRequired(canvasElement)
    const canvas = within(canvasElement)
    const codeInput = canvas.getByLabelText('四位配对码')
    const pairButton = canvas.getByRole('button', { name: '配对设备' })
    await userEvent.type(codeInput, '4827')
    await userEvent.keyboard('{Enter}')
    await userEvent.keyboard('{Enter}')
    await expect(args.pairDevice).toHaveBeenCalledTimes(1)
    await expect(pairButton).toBeDisabled()
  },
}

export const RequiredPairingPrompt: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: mockPairing,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    const dialog = canvas.getByRole('dialog', { name: '输入 LAN 配对码' })
    await expect(dialog).toBeVisible()
    const identity = within(dialog).getByLabelText('已连接设备详情')
    await expect(identity).toBeVisible()
    await expect(within(identity).getByText('192.168.1.18')).toBeVisible()
    await expect(within(identity).getByText('001122334455')).toBeVisible()
    expect(getComputedStyle(dialog).backgroundColor).not.toBe('rgba(0, 0, 0, 0)')
  },
}

export const SafariUnsupported: Story = {
  args: { supported: false },
}

export const PrivateNetworkBlocked: Story = {
  args: {
    connectDevice: async () => {
      throw new ControlPlaneClientError(
        '浏览器无法访问私网设备。',
        'lan_private_network_unavailable',
        false
      )
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    await expect(
      canvas.getByText(
        '浏览器阻止了对私网设备的访问。请确认使用 Chrome、Chromium 或 Edge，并允许私网访问。'
      )
    ).toBeVisible()
    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
  },
}

export const PairingWindowClosed: Story = {
  args: {
    connectDevice: async () => requiredInactive,
    getPairingMetadata: async () => requiredInactive.pairing,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await expect(dialog.getByText('请在设备上打开 WiFi Info 页面后重新检查配对码。')).toBeVisible()
    expect(dialog.queryByLabelText('四位配对码')).toBeNull()
  },
}

export const PairingCodeInvalid: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: async () => {
      throw new ControlPlaneClientError('Pairing code is invalid.', 'pairing_code_invalid', false)
    },
  },
  play: async (context) => {
    await connectRequired(context.canvasElement)
    const canvas = within(context.canvasElement)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(dialog.getByLabelText('四位配对码'), '4827')
    await userEvent.click(dialog.getByRole('button', { name: '配对设备' }))
    await expect(dialog.getByText('四位配对码不正确，请核对设备屏幕。')).toBeVisible()
    expect(
      canvas.queryByText('无法连接到设备。请确认设备地址、同网连接和私网访问权限。')
    ).toBeNull()
  },
}

export const PairingResponseInvalid: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: async () => {
      throw new ControlPlaneClientError('设备返回的数据格式无效。', 'lan_response_invalid', false)
    },
  },
  play: async (context) => {
    await connectRequired(context.canvasElement)
    const canvas = within(context.canvasElement)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(dialog.getByLabelText('四位配对码'), '4827')
    await userEvent.click(dialog.getByRole('button', { name: '配对设备' }))
    await expect(
      dialog.getByText('设备返回数据格式异常，配对尚未完成。请更新设备固件后重试。')
    ).toBeVisible()
    expect(
      canvas.queryByText('无法连接到设备。请确认设备地址、同网连接和私网访问权限。')
    ).toBeNull()
  },
}

export const PairingLocked: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: async () => {
      throw new ControlPlaneClientError('Pairing window is locked.', 'pairing_locked', false)
    },
  },
  play: async (context) => {
    await connectRequired(context.canvasElement)
    const canvas = within(context.canvasElement)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(dialog.getByLabelText('四位配对码'), '4827')
    await userEvent.click(dialog.getByRole('button', { name: '配对设备' }))
    await expect(
      canvas.getByText('该配对窗口已因连续失败锁定。请离开并重新进入 WiFi Info 页面获取新码。')
    ).toBeVisible()
  },
}

export const PairingInProgress: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: () => new Promise<never>(() => undefined),
  },
  play: async (context) => {
    await connectRequired(context.canvasElement)
    const canvas = within(context.canvasElement)
    const dialog = within(canvas.getByRole('dialog', { name: '输入 LAN 配对码' }))
    await userEvent.type(dialog.getByLabelText('四位配对码'), '4827')
    await userEvent.click(dialog.getByRole('button', { name: '配对设备' }))
    await expect(dialog.getByRole('button', { name: '配对设备' })).toBeDisabled()
  },
}

export const CodeExempt: Story = {
  args: {
    connectDevice: async () => optionalPairing,
    pairDevice: mockPairing,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    await expect(canvas.getByText('已配对 flux-purr-001122334455，正在获取控制租约')).toBeVisible()
    expect(canvas.queryByRole('dialog', { name: '输入 LAN 配对码' })).toBeNull()
  },
}

export const PairingUnavailable: Story = {
  args: {
    connectDevice: async () => unavailablePairing,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    await expect(
      canvas.getByText('已连接 flux-purr-001122334455。此设备仅提供基础低频信息读取。')
    ).toBeVisible()
    await expect(canvas.getByLabelText('基础设备信息')).toBeVisible()
    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
    expect(canvas.queryByRole('dialog', { name: '输入 LAN 配对码' })).toBeNull()
  },
}

export const BrowserIpScan: Story = {
  args: {
    connectDevice: async () => requiredInactive,
    scanDevices: async (_cidr, options) => {
      options?.onProgress?.({ done: 254, total: 254 })
      return [{ baseUrl: 'http://192.168.1.42', info: requiredInactive }]
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    expect(canvas.queryByText('DEVD')).toBeNull()
    const cidrInput = canvas.getByLabelText('CIDR 网段')
    await expect(cidrInput).toBeVisible()
    expect(canvas.queryByRole('button', { name: '扫描设备' })).toBeNull()
    await userEvent.clear(cidrInput)
    await userEvent.type(cidrInput, '192.168.1.0/24')
    await userEvent.click(canvas.getByRole('button', { name: '开始扫描' }))
    await expect(canvas.getByText('flux-purr-001122334455')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: '连接 flux-purr-001122334455' }))
    await expect(canvas.getByLabelText('设备地址')).toHaveValue('http://192.168.1.42')
    await expect(canvas.getByRole('dialog', { name: '输入 LAN 配对码' })).toBeVisible()
    await expect(
      canvas.getByRole('button', { name: '连接 flux-purr-001122334455' })
    ).toHaveAttribute('aria-pressed', 'true')
  },
}

export const EnterStartsBrowserIpScan: Story = {
  args: {
    scanDevices: enterScanDevices,
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement)
    const cidrInput = canvas.getByLabelText('CIDR 网段')
    await userEvent.click(cidrInput)
    await userEvent.keyboard('{Enter}')
    await userEvent.keyboard('{Enter}')
    await expect(args.scanDevices).toHaveBeenCalledTimes(1)
    await expect(cidrInput).toBeDisabled()
    await expect(await canvas.findByRole('button', { name: '取消' })).toBeVisible()
    resolveEnterScan?.([{ baseUrl: 'http://192.168.1.42', info: requiredInactive }])
    await expect(await canvas.findByLabelText('发现的设备')).toBeVisible()
  },
}

export const BrowserIpScanResults: Story = {
  args: {
    scanDevices: async (_cidr, options) => {
      options?.onProgress?.({ done: 254, total: 254 })
      return [{ baseUrl: 'http://192.168.1.42', info: requiredInactive }]
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByLabelText('CIDR 网段')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: '开始扫描' }))
    await expect(canvas.getByLabelText('发现的设备')).toBeVisible()
    expect(canvas.queryByText('DEVD')).toBeNull()
  },
}

export const BrowserIpScanInvalidRange: Story = {
  args: {
    scanDevices: async () => {
      throw new ControlPlaneClientError(
        '扫描范围最多包含 256 个地址。',
        'lan_scan_cidr_too_large',
        false
      )
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByLabelText('CIDR 网段')).toBeVisible()
    await userEvent.clear(canvas.getByLabelText('CIDR 网段'))
    await userEvent.type(canvas.getByLabelText('CIDR 网段'), '192.168.0.0/16')
    await userEvent.click(canvas.getByRole('button', { name: '开始扫描' }))
    await expect(canvas.getByText('扫描范围最多包含 256 个地址。')).toBeVisible()
  },
}

export const MobileRequiredPairing: Story = {
  args: {
    connectDevice: async () => requiredActive,
    getPairingMetadata: async () => requiredActive.pairing,
    pairDevice: mockPairing,
  },
  parameters: {
    viewport: { defaultViewport: 'mobile1' },
  },
  decorators: [
    (Story) => (
      <div style={{ width: '393px', maxWidth: '100%' }}>
        <Story />
      </div>
    ),
  ],
  play: async (context) => {
    await connectRequired(context.canvasElement)
    expect(
      within(context.canvasElement).getByLabelText('WiFi LAN pairing').getBoundingClientRect().width
    ).toBeLessThanOrEqual(393)
  },
}

export const MobilePairingUnavailable: Story = {
  args: {
    connectDevice: async () => unavailablePairing,
  },
  parameters: {
    viewport: { defaultViewport: 'mobile1' },
  },
  decorators: [
    (Story) => (
      <div style={{ width: '393px', maxWidth: '100%' }}>
        <Story />
      </div>
    ),
  ],
  play: async (context) => {
    const canvas = within(context.canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
    await expect(
      canvas.getByText('已连接 flux-purr-001122334455。此设备仅提供基础低频信息读取。')
    ).toBeVisible()
    expect(canvas.queryByLabelText('四位配对码')).toBeNull()
    expect(
      canvas.getByLabelText('WiFi LAN pairing').getBoundingClientRect().width
    ).toBeLessThanOrEqual(393)
  },
}

async function connectRequired(canvasElement: HTMLElement) {
  const canvas = within(canvasElement)
  await userEvent.click(canvas.getByRole('button', { name: '连接设备' }))
  await expect(canvas.getByRole('dialog', { name: '输入 LAN 配对码' })).toBeVisible()
}
