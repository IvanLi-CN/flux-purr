import type { Meta, StoryObj } from '@storybook/react-vite'
import { TelemetryTrendCard } from '@/features/device-console/components/telemetry-trend-card'
import { mockTelemetrySeries } from '@/features/device-console/mock-data'

const meta = {
  title: 'Components/TelemetryTrendCard',
  component: TelemetryTrendCard,
  args: {
    points: mockTelemetrySeries,
  },
} satisfies Meta<typeof TelemetryTrendCard>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}
