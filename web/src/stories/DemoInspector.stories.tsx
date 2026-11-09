import type { Meta, StoryObj } from '@storybook/react-vite'
import { useEffect, useState } from 'react'
import { expect, fireEvent, userEvent, within } from 'storybook/test'
import { DemoInspector } from '@/features/control-plane-demo/components/demo-inspector'
import {
  type DemoAction,
  defaultDemoInspectorState,
  deriveDemoScenario,
} from '@/features/control-plane-demo/demo-inspector-state'

function InspectorStory({
  initialState = defaultDemoInspectorState,
  clipboard,
}: {
  initialState?: typeof defaultDemoInspectorState
  clipboard?: { writeText(value: string): Promise<void> } | null
}) {
  const [state, setState] = useState(initialState)
  const [selectedDeviceId, setSelectedDeviceId] = useState(
    () => deriveDemoScenario(initialState).selectedDeviceId
  )
  const [actions, setActions] = useState<DemoAction[]>([])
  const scenario = deriveDemoScenario(state, [], actions, selectedDeviceId)
  return (
    <div className="industrial-shell" style={{ minHeight: 740, padding: 24 }}>
      <DemoInspector
        state={state}
        devices={scenario.devices}
        selectedDeviceId={scenario.selectedDeviceId}
        currentModule="/devices/fp-lab-01/overview"
        moduleOptions={[
          { value: '/devices/fp-lab-01/overview', label: 'Overview' },
          { value: '/devices/fp-lab-01/settings/wifi', label: 'Settings · WiFi' },
          {
            value: '/devices/fp-lab-01/calibration/heater-curve',
            label: 'Calibration · Heater curve',
          },
        ]}
        onModuleChange={() => undefined}
        onStateChange={(partial) => setState((current) => ({ ...current, ...partial }))}
        onSelectDevice={setSelectedDeviceId}
        shareUrl="https://flux-purr-demo.ivanli.cc/devices/fp-lab-01/overview"
        clipboard={clipboard}
        recentActions={actions}
        onSimulate={(kind) =>
          setActions((current) => [
            ...current,
            {
              kind,
              targetDeviceId: selectedDeviceId,
              sequence: current.length + 1,
            },
          ])
        }
        onReset={() => {
          setState(defaultDemoInspectorState)
          setActions([])
          setSelectedDeviceId(deriveDemoScenario(defaultDemoInspectorState).selectedDeviceId)
        }}
      />
    </div>
  )
}

function MobileInspectorStory() {
  const [ready, setReady] = useState(false)

  useEffect(() => {
    const originalMatchMedia = window.matchMedia
    window.matchMedia = (query) => {
      const mediaQuery = originalMatchMedia(query)
      if (query === '(max-width: 1179px)') {
        Object.defineProperty(mediaQuery, 'matches', { configurable: true, value: true })
      }
      return mediaQuery
    }
    setReady(true)
    return () => {
      window.matchMedia = originalMatchMedia
    }
  }, [])

  return ready ? <InspectorStory /> : null
}

const meta = {
  title: 'Components/DemoInspector',
  component: InspectorStory,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof InspectorStory>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const InteractionSmoke: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const body = within(canvasElement.ownerDocument.body)
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    await userEvent.click(await canvas.findByRole('combobox', { name: 'Scenario preset' }))
    await userEvent.click(await body.findByRole('option', { name: 'Degraded handoff' }))
    await expect(
      await canvas.findByRole('combobox', { name: 'Scenario preset' })
    ).toHaveTextContent('Degraded handoff')
    await expect(
      await canvas.findByRole('checkbox', { name: 'Simulate network timeout' })
    ).toBeChecked()
    await userEvent.click(await canvas.findByRole('checkbox', { name: 'Simulate network timeout' }))
    await expect(
      await canvas.findByRole('checkbox', { name: 'Simulate network timeout' })
    ).not.toBeChecked()
    await userEvent.click(await canvas.findByRole('combobox', { name: 'Current device fixture' }))
    await expect(
      await body.findByRole('option', { name: /USB Config Fixture.*WiFi 可配置/ })
    ).toHaveAttribute('data-value', 'fp-wifi-usb-04')
    await userEvent.keyboard('{Escape}')
    const permissionHelp = await canvas.findByRole('button', { name: '查看当前用户角色权限' })
    await userEvent.click(permissionHelp)
    await expect(await canvas.findByRole('note', { name: 'Role permissions' })).toBeVisible()
    await userEvent.keyboard('{Escape}')
    await expect(canvas.queryByRole('note', { name: 'Role permissions' })).toBeNull()
    await userEvent.click(await canvas.findByRole('button', { name: '查看 Demo Inspector 说明' }))
    await expect(await canvas.findByRole('note', { name: 'Demo scope' })).toBeVisible()
  },
}

export const DataUnavailable: Story = {
  render: () => (
    <InspectorStory
      initialState={{
        ...defaultDemoInspectorState,
        demoWifiCapability: 'unavailable',
        demoLogs: 'empty',
        demoTelemetry: 'unavailable',
      }}
    />
  ),
}

export const LayoutMenuReview: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    fireEvent.contextMenu(await canvas.findByRole('heading', { name: 'Demo Inspector' }))
    await expect(await canvas.queryByRole('menu', { name: 'Demo Inspector 布局' })).toBeNull()
    fireEvent.contextMenu(await canvas.findByRole('button', { name: '收起 Demo Inspector' }))
    await expect(await canvas.findByRole('menu', { name: 'Demo Inspector 布局' })).toBeVisible()
    await expect(await canvas.findByRole('menuitemradio', { name: /左侧/ })).toBeVisible()
    await expect(await canvas.findByRole('menuitemradio', { name: /右侧/ })).toBeVisible()
    await expect(await canvas.findByRole('menuitemradio', { name: /底部/ })).toBeVisible()
    await expect(await canvas.findByRole('menuitemradio', { name: /悬浮/ })).toBeVisible()
    await expect(await canvas.findByRole('menuitem', { name: /收起/ })).toBeVisible()
  },
}

export const MobileReview: Story = {
  render: () => <MobileInspectorStory />,
  parameters: {
    viewport: { value: 'fluxPurrMobile', isRotated: false },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    await expect(await canvas.findByRole('dialog', { name: 'Demo Inspector' })).toHaveAttribute(
      'aria-modal',
      'true'
    )
    await userEvent.keyboard('{Escape}')
    await expect(await canvas.findByRole('complementary', { name: 'Demo Inspector' })).toBeVisible()
  },
}

export const CopyFailure: Story = {
  render: () => (
    <InspectorStory clipboard={{ writeText: async () => Promise.reject(new Error('denied')) }} />
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    await userEvent.click(await canvas.findByRole('button', { name: '复制 Demo 分享链接' }))
    await expect(await canvas.findByText('Copy failed; select the URL manually')).toBeVisible()
  },
}

export const ResetState: Story = {
  render: () => (
    <InspectorStory initialState={{ ...defaultDemoInspectorState, demoScene: 'degraded' }} />
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    await userEvent.click(await canvas.findByRole('button', { name: 'Reset current case' }))
    await expect(
      await canvas.findByRole('combobox', { name: 'Scenario preset' })
    ).toHaveTextContent('Nominal operator')
  },
}
