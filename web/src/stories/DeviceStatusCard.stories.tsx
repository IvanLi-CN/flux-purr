import type { Meta, StoryObj } from '@storybook/react-vite'
import { DeviceStatusCard } from '@/features/device-console/components/device-status-card'
import { mockStatus } from '@/features/device-console/mock-data'

const meta = {
  title: 'Components/DeviceStatusCard',
  component: DeviceStatusCard,
  args: {
    status: mockStatus,
  },
} satisfies Meta<typeof DeviceStatusCard>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const Idle: Story = {
  args: {
    status: {
      ...mockStatus,
      mode: 'idle',
      current: 0.0,
    },
  },
}
