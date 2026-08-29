import type { Meta, StoryObj } from '@storybook/react-vite'
import { useState } from 'react'
import { expect, fn, userEvent, within } from 'storybook/test'
import { shouldUseWifiReceipt } from '@/features/control-plane-demo/components/control-plane-demo'
import { WifiNetworkSettings } from '@/features/control-plane-demo/components/wifi-network-settings'
import type { NetworkFailureCode, NetworkSummary } from '@/features/control-plane-demo/contracts'
import rawWifiProvisioningFixture from '../../../fixtures/wifi-provisioning-v2.json'

interface WifiProvisioningFixture {
  traces: Array<{
    name: string
    snapshots: Array<{
      state: NetworkSummary['state']
      configurationGeneration: number
      transitionSequence: number
      failureCode?: NetworkFailureCode
    }>
  }>
  failureCodes: NetworkFailureCode[]
  outOfOrderDelivery: {
    snapshots: Array<{
      state: NetworkSummary['state']
      configurationGeneration: number
      transitionSequence: number
      failureCode?: NetworkFailureCode
    }>
  }
}

const wifiProvisioningFixture = rawWifiProvisioningFixture as WifiProvisioningFixture

const snapshot = (
  state: NetworkSummary['state'],
  generation = 1,
  sequence = 1,
  failureCode: NetworkSummary['failureCode'] = null
): NetworkSummary => ({
  state,
  configurationGeneration: generation,
  transitionSequence: sequence,
  failureCode,
  ssid: state === 'disabled' ? null : 'FluxPurr-Lab',
  wifiPasswordLength: state === 'disabled' ? 0 : 11,
  wifiRssi: state === 'connected' ? -48 : null,
})

function fixtureSnapshot(value: {
  state: NetworkSummary['state']
  configurationGeneration: number
  transitionSequence: number
  failureCode?: NetworkSummary['failureCode']
}): NetworkSummary {
  return snapshot(
    value.state,
    value.configurationGeneration,
    value.transitionSequence,
    value.failureCode ?? null
  )
}

function fixtureTrace(name: string) {
  const trace = wifiProvisioningFixture.traces.find((candidate) => candidate.name === name)
  if (!trace) {
    throw new Error(`Missing WiFi provisioning fixture trace: ${name}`)
  }
  return trace.snapshots.map(fixtureSnapshot)
}

const meta = {
  title: 'App/WifiNetworkSettings',
  component: WifiNetworkSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    (Story) => (
      <div
        data-testid="wifi-network-settings-story"
        className="h-screen overflow-hidden bg-[#d6e4ed] p-8"
      >
        <div className="w-[60rem] border border-[#536171] bg-[#eef1f4] p-4">
          <Story />
        </div>
      </div>
    ),
  ],
  args: {
    deviceId: 'serial-001122334455',
    networkState: 'disabled',
    wifiRssi: null,
    disabled: false,
    configurationGeneration: 0,
    transitionSequence: 0,
    onSave: fn(async () => snapshot('connecting', 1, 1)),
    onClear: fn(async () => snapshot('disabled', 1, 2)),
  },
} satisfies Meta<typeof WifiNetworkSettings>

export default meta
type Story = StoryObj<typeof meta>

function TraceHarness({ trace }: { trace: NetworkSummary[] }) {
  const [index, setIndex] = useState(0)
  const [clearSnapshot, setClearSnapshot] = useState<NetworkSummary | null>(null)
  const current = clearSnapshot ?? trace[index]
  return (
    <>
      <button
        type="button"
        onClick={() => {
          setClearSnapshot(null)
          setIndex((value) => Math.min(value + 1, trace.length - 1))
        }}
      >
        推进设备 trace
      </button>
      <WifiNetworkSettings
        deviceId="serial-001122334455"
        networkState={current.state}
        savedSsid={current.ssid}
        wifiRssi={current.wifiRssi}
        savedPasswordLength={current.wifiPasswordLength}
        configurationGeneration={current.configurationGeneration}
        transitionSequence={current.transitionSequence}
        failureCode={current.failureCode}
        onSave={async () => {
          const nextIndex = Math.min(index + 1, trace.length - 1)
          const next = trace[nextIndex] ?? current
          setClearSnapshot(null)
          setIndex(nextIndex)
          return next
        }}
        onClear={async () => {
          const cleared = snapshot(
            'disabled',
            (current.configurationGeneration ?? 0) + 1,
            (current.transitionSequence ?? 0) + 1
          )
          setClearSnapshot(cleared)
          return cleared
        }}
      />
    </>
  )
}

function ReceiptHarness() {
  const [current, setCurrent] = useState(snapshot('connecting', 5, 19))
  const [resolveReceipt, setResolveReceipt] = useState<((receipt: NetworkSummary) => void) | null>(
    null
  )
  return (
    <>
      <button
        type="button"
        disabled={!resolveReceipt}
        onClick={() => {
          const receipt = snapshot('connecting', 6, 20)
          setCurrent(receipt)
          resolveReceipt?.(receipt)
          setResolveReceipt(null)
        }}
      >
        发布设备 receipt
      </button>
      <WifiNetworkSettings
        deviceId="serial-001122334455"
        networkState={current.state}
        wifiRssi={current.wifiRssi}
        savedPasswordLength={current.wifiPasswordLength}
        configurationGeneration={current.configurationGeneration}
        transitionSequence={current.transitionSequence}
        failureCode={current.failureCode}
        onSave={() =>
          new Promise<NetworkSummary>((resolve) => {
            setResolveReceipt(() => resolve)
          })
        }
        onClear={async () => snapshot('disabled', 6, 21)}
      />
    </>
  )
}

function PendingClearHarness() {
  const [current] = useState(snapshot('connected', 5, 19))
  const [resolveReceipt, setResolveReceipt] = useState<((receipt: NetworkSummary) => void) | null>(
    null
  )
  return (
    <>
      <button
        type="button"
        disabled={!resolveReceipt}
        onClick={() => {
          resolveReceipt?.(snapshot('disabled', 6, 20))
          setResolveReceipt(null)
        }}
      >
        发布清除 receipt
      </button>
      <WifiNetworkSettings
        deviceId="serial-001122334455"
        networkState={current.state}
        savedSsid={current.ssid}
        wifiRssi={current.wifiRssi}
        savedPasswordLength={current.wifiPasswordLength}
        configurationGeneration={current.configurationGeneration}
        transitionSequence={current.transitionSequence}
        failureCode={current.failureCode}
        onSave={async () => snapshot('connecting', 6, 21)}
        onClear={() =>
          new Promise<NetworkSummary>((resolve) => {
            setResolveReceipt(() => resolve)
          })
        }
      />
    </>
  )
}

function SameVersionDeviceSnapshotHarness() {
  const [deviceSnapshot, setDeviceSnapshot] = useState(snapshot('connecting', 1, 4))
  const [receipt, setReceipt] = useState<NetworkSummary | null>(null)
  const current =
    receipt && shouldUseWifiReceipt(deviceSnapshot, receipt) ? receipt : deviceSnapshot

  return (
    <>
      <button
        type="button"
        disabled={!receipt}
        onClick={() => setDeviceSnapshot(snapshot('connected', 1, 5))}
      >
        发布设备 connected snapshot
      </button>
      <WifiNetworkSettings
        deviceId="serial-001122334455"
        networkState={current.state}
        savedSsid={current.ssid}
        wifiRssi={current.wifiRssi}
        savedPasswordLength={current.wifiPasswordLength}
        configurationGeneration={current.configurationGeneration}
        transitionSequence={current.transitionSequence}
        failureCode={current.failureCode}
        onSave={async () => {
          const next = snapshot('connecting', 1, 5)
          setReceipt(next)
          return next
        }}
        onClear={async () => snapshot('disabled', 1, 6)}
      />
    </>
  )
}

export const StateGallery: Story = {
  render: () => (
    <div className="grid gap-4 md:grid-cols-2">
      {(['disabled', 'connecting', 'connected', 'error'] as const).map((state) => {
        const value = snapshot(state)
        return (
          <WifiNetworkSettings
            key={state}
            deviceId={`serial-${state}`}
            networkState={value.state}
            savedSsid={value.ssid}
            wifiRssi={value.wifiRssi}
            savedPasswordLength={value.wifiPasswordLength}
            configurationGeneration={value.configurationGeneration}
            transitionSequence={value.transitionSequence}
            failureCode={value.failureCode}
            onSave={async () => snapshot('connecting', 2, 1)}
            onClear={async () => snapshot('disabled', 2, 2)}
          />
        )
      })}
    </div>
  ),
}

export const ReadOnlyLanSnapshot: Story = {
  name: 'Read-only / LAN snapshot',
  args: {
    networkState: 'connected',
    savedSsid: 'FluxPurr-Lab',
    wifiRssi: -54,
    savedPasswordLength: 11,
    readOnly: true,
    unavailableReason: '当前通过 WiFi / LAN 连接，只能查看网络信息；请通过 USB 配置连接修改 WiFi。',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('alert')).toHaveTextContent(
      '当前通过 WiFi / LAN 连接，只能查看网络信息；请通过 USB 配置连接修改 WiFi。'
    )
    await expect(canvas.getByText('FluxPurr-Lab')).toBeVisible()
    await expect(canvas.getByText('•••••••••••')).toBeVisible()
    expect(canvas.queryByRole('textbox', { name: 'WiFi 名称' })).toBeNull()
    expect(canvas.queryByRole('button', { name: '保存并连接' })).toBeNull()
    expect(canvas.queryByRole('button', { name: '清除 WiFi' })).toBeNull()
  },
}

export const ProvisioningSucceedsAfterTransientRetry: Story = {
  render: () => <TraceHarness trace={fixtureTrace('transient_retry_success')} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByLabelText('WiFi 名称'), 'FluxPurr-Lab')
    await userEvent.type(canvas.getByLabelText('密码'), 'secret-pass')
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await expect(canvas.getByText('已提交，正在等待设备连接。')).toBeVisible()
    await expect(canvas.queryByText('WiFi 连接失败，请检查名称和密码。')).not.toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await expect(canvas.getByText('WiFi 已连接。')).toBeVisible()
    await expect(canvas.getByLabelText('WiFi 名称')).toHaveValue('FluxPurr-Lab')
    await expect(canvas.queryByText('WiFi 连接失败，请检查名称和密码。')).not.toBeInTheDocument()
  },
}

export const WaitsForDeviceReceipt: Story = {
  render: () => <ReceiptHarness />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByLabelText('WiFi 名称'), 'FluxPurr-Lab')
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await expect(canvas.queryByText('已提交，正在等待设备连接。')).not.toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: '保存中' })).toBeDisabled()
    await userEvent.click(canvas.getByRole('button', { name: '发布设备 receipt' }))
    const loadingToast = canvas.getByText('已提交，正在等待设备连接。')
    await expect(loadingToast).toBeVisible()
    await expect(loadingToast.closest('[role="status"]')).toHaveAttribute('aria-busy', 'true')
    const saveButton = canvas.getByRole('button', { name: '保存中' })
    const clearButton = canvas.getByRole('button', { name: '清除 WiFi' })
    await expect(saveButton).toBeDisabled()
    await expect(saveButton).toHaveAttribute('aria-busy', 'true')
    await expect(clearButton).toBeDisabled()
    await expect(clearButton).toHaveAttribute('aria-busy', 'false')
    await expect(canvas.queryByRole('button', { name: '清除中' })).not.toBeInTheDocument()
  },
}

export const ClearWaitsForDeviceReceipt: Story = {
  render: () => <PendingClearHarness />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '清除 WiFi' }))
    await userEvent.click(canvas.getByRole('button', { name: '确认清除' }))
    const clearButton = canvas.getByRole('button', { name: '确认清除' })
    const saveButton = canvas.getByRole('button', { name: '保存并连接' })
    await expect(clearButton).toBeDisabled()
    await expect(clearButton).toHaveAttribute('aria-busy', 'true')
    await expect(saveButton).toBeDisabled()
    await expect(saveButton).toHaveAttribute('aria-busy', 'false')
    await expect(canvas.queryByRole('button', { name: '保存中' })).not.toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: '发布清除 receipt' }))
    await expect(canvas.getByText('已清除设备中的 WiFi 设置。')).toBeVisible()
  },
}

export const SameVersionDeviceSnapshotCompletes: Story = {
  render: () => <SameVersionDeviceSnapshotHarness />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByLabelText('WiFi 名称'), 'FluxPurr-Lab')
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await expect(canvas.getByText('已提交，正在等待设备连接。')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: '发布设备 connected snapshot' }))
    await expect(canvas.getByText('已连接', { selector: 'strong' })).toBeVisible()
    await expect(canvas.getByText('WiFi 已连接。')).toBeVisible()
    await expect(canvas.queryByText('已提交，正在等待设备连接。')).not.toBeInTheDocument()
  },
}

export const TerminalFailure: Story = {
  render: () => <TraceHarness trace={fixtureTrace('invalid_credentials_terminal_error')} />,
}

export const TerminalTimeout: Story = {
  render: () => <TraceHarness trace={fixtureTrace('ipv4_terminal_timeout')} />,
}

export const TerminalFailureStaysSettledUntilNewConfiguration: Story = {
  render: () => <TraceHarness trace={fixtureTrace('terminal_error_requires_new_configuration')} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByLabelText('WiFi 名称'), 'FluxPurr-Lab')
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await expect(canvas.getByText('WiFi 连接失败，请检查名称和密码。')).toBeVisible()
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await expect(canvas.getByText('已提交，正在等待设备连接。')).toBeVisible()
    await expect(canvas.queryByText('WiFi 连接失败，请检查名称和密码。')).not.toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await expect(canvas.getByText('已连接', { selector: 'strong' })).toBeVisible()
    await expect(canvas.queryByText('WiFi 连接失败，请检查名称和密码。')).not.toBeInTheDocument()
    await expect(canvas.getByText('WiFi 已连接。')).toBeVisible()
  },
}

export const StaleSnapshotIsIgnored: Story = {
  render: () => (
    <TraceHarness
      trace={[
        snapshot('idle', 5, 19),
        ...wifiProvisioningFixture.outOfOrderDelivery.snapshots.map(fixtureSnapshot),
      ]}
    />
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.type(canvas.getByLabelText('WiFi 名称'), 'FluxPurr-Lab')
    await userEvent.click(canvas.getByRole('button', { name: '保存并连接' }))
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await expect(canvas.getByText('已提交，正在等待设备连接。')).toBeVisible()
    await expect(canvas.queryByText('WiFi 连接失败，请检查名称和密码。')).not.toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: '推进设备 trace' }))
    await expect(canvas.getByText('WiFi 已连接。')).toBeVisible()
  },
}

export const SavedPasswordLength: Story = {
  args: {
    networkState: 'connected',
    savedSsid: 'FluxPurr-Lab',
    wifiRssi: -48,
    savedPasswordLength: 11,
    configurationGeneration: 2,
    transitionSequence: 5,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByLabelText('WiFi 名称')).toHaveValue('FluxPurr-Lab')
  },
}

export const SavedPasswordMaskIsEditable: Story = {
  args: {
    networkState: 'connected',
    savedSsid: 'FluxPurr-Lab',
    wifiRssi: -48,
    savedPasswordLength: 11,
    configurationGeneration: 2,
    transitionSequence: 5,
    onSave: fn(async () => snapshot('connecting', 3, 6)),
  },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement)
    const password = canvas.getByLabelText('密码') as HTMLInputElement
    const saveButton = canvas.getByRole('button', { name: '保存并连接' })
    const savedMask = '•••••••••••'

    await expect(password).toHaveValue(savedMask)
    await expect(password).not.toHaveAttribute('placeholder')
    await expect(saveButton).toBeDisabled()

    await userEvent.click(password)
    expect(password.selectionStart).toBe(0)
    expect(password.selectionEnd).toBe(savedMask.length)

    await userEvent.keyboard('{Backspace}')
    await expect(password).toHaveValue('')
    await expect(saveButton).toBeEnabled()
    await userEvent.click(saveButton)
    await expect(args.onSave).toHaveBeenCalledWith({ ssid: 'FluxPurr-Lab', password: '' })
  },
}

export const MobileSavedPasswordLength: Story = {
  render: (args) => (
    <div
      className="max-w-[422px] border border-[#536171] bg-[#d6e4ed] p-5"
      data-testid="wifi-mobile-story"
    >
      <WifiNetworkSettings {...args} />
    </div>
  ),
  args: {
    networkState: 'connected',
    savedSsid: 'FluxPurr-Lab',
    wifiRssi: -48,
    savedPasswordLength: 11,
    configurationGeneration: 2,
    transitionSequence: 5,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByLabelText('自动重连')).not.toBeInTheDocument()
    const surface = canvasElement.querySelector<HTMLElement>('[data-testid="wifi-mobile-story"]')
    if (!surface) {
      throw new Error('Mobile WiFi story surface was not rendered.')
    }
    const surfaceBounds = surface.getBoundingClientRect()
    const controls = [
      canvas.getByLabelText('WiFi 名称'),
      canvas.getByLabelText('密码'),
      canvas.getByRole('button', { name: '保存并连接' }),
      canvas.getByRole('button', { name: '清除 WiFi' }),
    ]
    for (const control of controls) {
      const bounds = control.getBoundingClientRect()
      await expect(bounds.left).toBeGreaterThanOrEqual(surfaceBounds.left)
      await expect(bounds.right).toBeLessThanOrEqual(surfaceBounds.right)
    }
  },
}

export const TransportUnavailable: Story = {
  args: {
    disabled: true,
    readOnly: true,
    networkState: 'connecting',
    configurationGeneration: 2,
    transitionSequence: 2,
    unavailableReason: '授权串口当前不可用。',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByText('授权串口当前不可用。')).toBeVisible()
    await expect(
      canvas.queryByText('当前设备固件需要 WiFi 状态协议更新后才能提交设置。')
    ).not.toBeInTheDocument()
  },
}

export const FailureCodeGallery: Story = {
  render: () => (
    <div className="grid gap-4 md:grid-cols-2">
      {wifiProvisioningFixture.failureCodes.map((failureCode, index) => {
        const terminalState = 'error' as const
        const value = snapshot(
          terminalState,
          10,
          index + 1,
          failureCode as NetworkSummary['failureCode']
        )
        return (
          <div key={failureCode} className="space-y-1">
            <code>{failureCode}</code>
            <WifiNetworkSettings
              deviceId={`serial-${failureCode}`}
              networkState={value.state}
              savedSsid={value.ssid}
              wifiRssi={value.wifiRssi}
              savedPasswordLength={value.wifiPasswordLength}
              configurationGeneration={value.configurationGeneration}
              transitionSequence={value.transitionSequence}
              failureCode={value.failureCode}
              onSave={async () => snapshot('connecting', 11, index + 1)}
              onClear={async () => snapshot('disabled', 11, index + 2)}
            />
          </div>
        )
      })}
    </div>
  ),
}

export const ClearReceiptCompletes: Story = {
  render: () => <TraceHarness trace={[snapshot('connected', 1, 1)]} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(canvas.getByRole('button', { name: '清除 WiFi' }))
    await userEvent.click(canvas.getByRole('button', { name: '确认清除' }))
    await expect(canvas.getByText('已清除设备中的 WiFi 设置。')).toBeVisible()
  },
}

export const CleanFormBlocksSaveAndClearConfirmation: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('button', { name: '保存并连接' })).toBeDisabled()
    await userEvent.click(canvas.getByRole('button', { name: '清除 WiFi' }))
    await expect(canvas.getByRole('button', { name: '确认清除' })).toBeVisible()
  },
}
