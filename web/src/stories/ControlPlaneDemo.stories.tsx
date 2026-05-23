import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, within } from 'storybook/test'
import {
  ControlPlaneDemo,
  ControlPlaneDemoGallery,
  degradedControlPlaneScenario,
} from '@/features/control-plane-demo'

const meta = {
  title: 'Pages/ControlPlaneDemo',
  component: ControlPlaneDemo,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    docs: {
      description: {
        component:
          'Fixed-height industrial skeuomorphic bench console with Dashboard, Settings, Update, and a desktop global log panel. It uses mock data only and does not connect to hardware.',
      },
    },
    a11y: {
      test: 'error',
    },
  },
} satisfies Meta<typeof ControlPlaneDemo>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const DegradedTransport: Story = {
  args: {
    scenario: degradedControlPlaneScenario,
  },
}

export const SettingsReview: Story = {
  args: {
    initialView: 'settings',
  },
}

export const UpdateReview: Story = {
  args: {
    initialView: 'update',
  },
}

export const DocsGallery: Story = {
  name: 'Docs / Gallery',
  parameters: {
    a11y: {
      test: 'todo',
    },
  },
  render: () => <ControlPlaneDemoGallery />,
}

export const MobileReview: Story = {
  parameters: {
    viewport: {
      defaultViewport: 'mobile1',
    },
  },
  render: () => (
    <div style={{ maxWidth: 390 }}>
      <ControlPlaneDemo />
    </div>
  ),
}

export const InteractionSmoke: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)

    const targetSelect = canvas.getByRole('combobox', { name: /target/i })
    await userEvent.selectOptions(targetSelect, 'fp-kit-02')
    await expect(targetSelect).toHaveValue('fp-kit-02')
    await expect(canvas.getAllByText('SERIAL').length).toBeGreaterThan(0)
    await userEvent.click(canvas.getByRole('button', { name: /increase target temperature/i }))
    await expect(canvas.getByLabelText('Dashboard target temperature')).toHaveValue(265)
    await expect(canvas.getByText('Target updated')).toBeInTheDocument()

    await userEvent.click(canvas.getByRole('button', { name: /settings/i }))
    await expect(canvas.getByText('Heat policy')).toBeInTheDocument()
    await expect(canvas.getByText('265℃')).toBeInTheDocument()
    await expect(canvas.getByText('Preset temperatures')).toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: /M3 120℃ disabled/i }))
    await expect(canvas.queryByRole('button', { name: /use as target/i })).not.toBeInTheDocument()
    const presetSwitch = canvas.getByRole('switch', { name: /preset M3/i })
    await expect(presetSwitch).toHaveAttribute('aria-checked', 'false')
    await userEvent.click(presetSwitch)
    await expect(presetSwitch).toHaveAttribute('aria-checked', 'true')
    await expect(canvas.queryByRole('button', { name: /use as target/i })).not.toBeInTheDocument()
    await expect(canvas.getByText('Preset M3 enabled')).toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: /M8/i }))
    await expect(canvas.getByLabelText('Preset temperature')).toHaveValue(220)
    await userEvent.click(canvas.getByRole('button', { name: /increase target temperature/i }))
    await expect(canvas.getByLabelText('Preset temperature')).toHaveValue(225)
    await expect(await canvas.findByText('Preset M8 updated')).toBeInTheDocument()
    await expect(canvas.queryByRole('button', { name: /use as target/i })).not.toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: 'RUN' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    await userEvent.click(canvas.getByRole('button', { name: 'OFF' }))
    await expect(canvas.getByRole('button', { name: 'OFF' })).toHaveAttribute(
      'aria-pressed',
      'true'
    )
    await expect(canvas.getByText('Fan policy updated')).toBeInTheDocument()

    await userEvent.click(canvas.getByRole('button', { name: /update/i }))
    await expect(canvas.getByText('Firmware check')).toBeInTheDocument()
    await expect(canvas.getByText('Ready to check')).toBeInTheDocument()
    const artifactSelect = canvas.getByRole('combobox', { name: /firmware artifact/i })
    await userEvent.selectOptions(artifactSelect, 'c3-legacy')
    await expect(canvas.getByText('Not compatible')).toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: /run dry-check/i })).toBeDisabled()
    await userEvent.selectOptions(artifactSelect, 'wifi-http-rc')
    await expect(canvas.getByText('Check recommended')).toBeInTheDocument()
    await userEvent.click(canvas.getByRole('button', { name: /run dry-check/i }))
    await expect(canvas.getByRole('button', { name: /checking/i })).toBeDisabled()
    await expect(
      await canvas.findByText('Check passed', undefined, { timeout: 5000 })
    ).toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: /run again/i })).toBeEnabled()

    await userEvent.click(canvas.getByRole('button', { name: /degrade/i }))
    await expect(canvas.getByText('CHECK')).toBeInTheDocument()
  },
}
