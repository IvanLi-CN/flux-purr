import type { Meta, StoryObj } from '@storybook/react-vite'
import { WifiConfigForm } from '@/features/device-console/components/wifi-config-form'
import { mockWifiConfig } from '@/features/device-console/mock-data'

const meta = {
  title: 'Components/WifiConfigForm',
  component: WifiConfigForm,
  args: {
    initialConfig: mockWifiConfig,
  },
} satisfies Meta<typeof WifiConfigForm>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}
