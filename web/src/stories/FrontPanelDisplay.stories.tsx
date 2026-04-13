import type { Meta, StoryObj } from '@storybook/react-vite'
import { FrontPanelDesignBoard } from '@/features/frontpanel-preview/components/front-panel-design-board'
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
    <div
      style={{
        minHeight: '100vh',
        background:
          'radial-gradient(circle at top, rgba(23,37,84,0.92) 0%, rgba(8,17,31,1) 34%, rgba(2,6,23,1) 100%)',
        padding: '36px',
        color: '#f8fafc',
        fontFamily: '"Space Grotesk", system-ui, sans-serif',
      }}
    >
      <section
        style={{
          maxWidth: '1440px',
          margin: '0 auto',
          borderRadius: '36px',
          border: '1px solid rgba(42, 61, 93, 0.88)',
          background: 'linear-gradient(180deg, rgba(8,17,31,0.94) 0%, rgba(2,6,23,0.96) 100%)',
          boxShadow: '0 30px 80px rgba(2, 6, 23, 0.48)',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            display: 'grid',
            gap: '20px',
            padding: '32px 36px 28px',
            borderBottom: '1px solid rgba(42, 61, 93, 0.72)',
            background: 'linear-gradient(180deg, rgba(27,42,67,0.68) 0%, rgba(8,17,31,0.12) 100%)',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'flex-start',
              gap: '24px',
            }}
          >
            <div style={{ maxWidth: '920px' }}>
              <p
                style={{
                  margin: 0,
                  color: '#63d8ff',
                  fontSize: '12px',
                  fontWeight: 700,
                  letterSpacing: '0.26em',
                  textTransform: 'uppercase',
                }}
              >
                Flux Purr
              </p>
              <h1
                style={{
                  margin: '14px 0 14px',
                  color: '#f8fafc',
                  fontSize: '48px',
                  lineHeight: 1.04,
                }}
              >
                160×50 front-panel state gallery
              </h1>
              <p
                style={{
                  margin: 0,
                  maxWidth: '840px',
                  color: '#cbd5e1',
                  fontSize: '16px',
                  lineHeight: 1.7,
                }}
              >
                Dark-theme overview for the complete front-panel UI set. Dashboard stays dominant,
                preferences use horizontal icon switching, and every secondary page inherits the
                same palette, bitmap typography, and tiny-screen spacing rules.
              </p>
            </div>

            <div
              style={{
                display: 'grid',
                gap: '12px',
                minWidth: '220px',
              }}
            >
              {[
                ['Theme', 'Dark embedded UI'],
                ['Screen set', '10 states'],
              ].map(([label, value]) => (
                <div
                  key={label}
                  style={{
                    borderRadius: '18px',
                    border: '1px solid rgba(42, 61, 93, 0.88)',
                    background: 'rgba(8, 17, 31, 0.72)',
                    padding: '14px 16px',
                  }}
                >
                  <div
                    style={{
                      color: '#8ea3c6',
                      fontSize: '11px',
                      letterSpacing: '0.2em',
                      textTransform: 'uppercase',
                    }}
                  >
                    {label}
                  </div>
                  <div
                    style={{
                      marginTop: '8px',
                      color: '#f8fafc',
                      fontSize: '22px',
                      fontWeight: 700,
                    }}
                  >
                    {value}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div style={{ padding: '28px 36px 36px' }}>
          <FrontPanelGallery screens={frontPanelGalleryOrder} />
        </div>
      </section>
    </div>
  ),
}

export const DesignSpec: Story = {
  name: 'Design Spec',
  render: () => (
    <div
      style={{
        minHeight: '100vh',
        background:
          'radial-gradient(circle at top, rgba(23,37,84,0.92) 0%, rgba(8,17,31,1) 34%, rgba(2,6,23,1) 100%)',
        padding: '36px',
      }}
    >
      <FrontPanelDesignBoard />
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

export const PresetTempDisabled: Story = {
  args: {
    screen: frontPanelStoryStates.presetTempDisabled,
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
