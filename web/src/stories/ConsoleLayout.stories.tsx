import type { Meta, StoryObj } from '@storybook/react-vite'
import { ConsoleLayout } from '@/features/device-console/components/console-layout'
import {
  mockStatus,
  mockTelemetrySeries,
  mockWifiConfig,
} from '@/features/device-console/mock-data'

const meta = {
  title: 'Pages/ConsoleLayout',
  component: ConsoleLayout,
  parameters: {
    layout: 'fullscreen',
  },
  args: {
    title: 'S3 Runtime Cockpit',
    subtitle: '设备监控、配置与采样趋势统一在一屏内。',
    status: mockStatus,
    telemetry: mockTelemetrySeries,
    wifiConfig: mockWifiConfig,
  },
} satisfies Meta<typeof ConsoleLayout>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const FaultState: Story = {
  args: {
    status: {
      ...mockStatus,
      mode: 'fault',
      current: 0.13,
      boardTempC: 72.3,
      wifiRssi: -86,
    },
  },
}
