import type { Meta, StoryObj } from '@storybook/react-vite'
import { useState } from 'react'
import { expect, userEvent, within } from 'storybook/test'
import { DemoInspector } from '@/features/control-plane-demo/components/demo-inspector'
import {
  defaultDemoInspectorState,
  deriveDemoScenario,
} from '@/features/control-plane-demo/demo-inspector-state'

function InspectorStory() {
  const [state, setState] = useState(defaultDemoInspectorState)
  const scenario = deriveDemoScenario(state)
  return (
    <div className="industrial-shell" style={{ minHeight: 740, padding: 24 }}>
      <DemoInspector
        state={state}
        devices={scenario.devices}
        selectedDeviceId={scenario.selectedDeviceId}
        onStateChange={(partial) => setState((current) => ({ ...current, ...partial }))}
        onSelectDevice={() => undefined}
        onSimulate={() => undefined}
      />
    </div>
  )
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
    await userEvent.click(await canvas.findByRole('button', { name: '打开 Demo Inspector' }))
    await userEvent.click(await canvas.findByRole('button', { name: /^Degraded/ }))
    await expect(await canvas.findByRole('button', { name: /^Degraded/ })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    await userEvent.click(await canvas.findByRole('checkbox', { name: 'Simulate network timeout' }))
    await expect(
      await canvas.findByRole('checkbox', { name: 'Simulate network timeout' })
    ).toBeChecked()
    await expect(
      await canvas.findByRole('button', { name: /USB Config Fixture.*WiFi 可配置/ })
    ).toBeVisible()
  },
}
