import type { Meta, StoryObj } from '@storybook/react-vite'
import { useState } from 'react'
import { expect, userEvent, within } from 'storybook/test'
import { DemoInspector } from '@/features/control-plane-demo/components/demo-inspector'
import { defaultDemoInspectorState } from '@/features/control-plane-demo/demo-inspector-state'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'

function InspectorStory() {
  const [state, setState] = useState(defaultDemoInspectorState)
  return (
    <div className="industrial-shell" style={{ minHeight: 740, padding: 24 }}>
      <DemoInspector
        state={state}
        devices={controlPlaneScenario.devices}
        selectedDeviceId={controlPlaneScenario.selectedDeviceId}
        onStateChange={setState}
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
    await userEvent.click(await canvas.findByRole('button', { name: /^Degraded/ }))
    await expect(await canvas.findByRole('button', { name: /^Degraded/ })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    await userEvent.click(await canvas.findByRole('checkbox', { name: 'Simulate network timeout' }))
    await expect(
      await canvas.findByRole('checkbox', { name: 'Simulate network timeout' })
    ).toBeChecked()
  },
}
