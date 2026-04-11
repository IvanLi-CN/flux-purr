import type { Meta, StoryObj } from '@storybook/react-vite'
import { FrontPanelDisplay } from '@/features/frontpanel-preview/components/front-panel-display'
import { FrontPanelGallery } from '@/features/frontpanel-preview/components/front-panel-gallery'
import {
  frontPanelGalleryOrder,
  frontPanelStoryStates,
} from '@/features/frontpanel-preview/mock-data'

const meta = {
  title: 'Embedded/FrontPanelDisplay',
  component: FrontPanelDisplay,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
    docs: {
      description: {
        component:
          '160×50 front-panel preview contract for the Flux Purr hotplate. The preview keeps temperature dominant and squeezes PWM, VIN, fan, and connection state into a compact right stack.',
      },
    },
  },
  decorators: [
    (Story) => (
      <div className="min-h-screen bg-[radial-gradient(circle_at_top,_#172554,_#020617_50%)] px-8 py-10 text-slate-100">
        <Story />
      </div>
    ),
  ],
  args: {
    screen: frontPanelStoryStates.home,
    scale: 6,
  },
} satisfies Meta<typeof FrontPanelDisplay>

export default meta
type Story = StoryObj<typeof meta>

export const DocsGallery: Story = {
  name: 'Docs / Gallery',
  render: () => (
    <div className="mx-auto max-w-7xl space-y-6">
      <header className="space-y-2">
        <p className="text-sm font-medium uppercase tracking-[0.22em] text-cyan-300">Flux Purr</p>
        <h1 className="text-4xl font-semibold tracking-tight">160×50 front-panel state gallery</h1>
        <p className="max-w-3xl text-sm leading-6 text-slate-300">
          The layout is intentionally optimized for the tiny 1.12-inch screen: temperature owns the
          left column, setpoint plus protocol/fan status sit on the right, and heat output stays in
          a full-width bottom bar.
        </p>
      </header>
      <FrontPanelGallery screens={frontPanelGalleryOrder} />
    </div>
  ),
}

export const Home: Story = {
  args: {
    screen: frontPanelStoryStates.home,
  },
}

export const MenuLevel1: Story = {
  args: {
    screen: frontPanelStoryStates.menu,
  },
}

export const PreferencesPresetTemp: Story = {
  args: {
    screen: {
      ...frontPanelStoryStates.menu,
      selectedItem: 'preset-temp',
    },
  },
}

export const PreferencesActiveCooling: Story = {
  args: {
    screen: {
      ...frontPanelStoryStates.menu,
      selectedItem: 'active-cooling',
    },
  },
}

export const PreferencesWifiInfo: Story = {
  args: {
    screen: {
      ...frontPanelStoryStates.menu,
      selectedItem: 'wifi-info',
    },
  },
}

export const PreferencesDeviceInfo: Story = {
  args: {
    screen: {
      ...frontPanelStoryStates.menu,
      selectedItem: 'device-info',
    },
  },
}

export const PresetTemp: Story = {
  args: {
    screen: frontPanelStoryStates.presetTemp,
  },
}

export const ActiveCooling: Story = {
  args: {
    screen: frontPanelStoryStates.activeCooling,
  },
}

export const WifiInfo: Story = {
  args: {
    screen: frontPanelStoryStates.wifiInfo,
  },
}

export const DeviceInfo: Story = {
  args: {
    screen: frontPanelStoryStates.deviceInfo,
  },
}
