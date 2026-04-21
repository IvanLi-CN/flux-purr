import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, userEvent, within } from 'storybook/test'
import { FrontPanelDesignBoard } from '@/features/frontpanel-preview/components/front-panel-design-board'
import { FrontPanelDisplay } from '@/features/frontpanel-preview/components/front-panel-display'
import { FrontPanelGallery } from '@/features/frontpanel-preview/components/front-panel-gallery'
import { FrontPanelRuntimeHarness } from '@/features/frontpanel-preview/components/front-panel-runtime-harness'
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
          '160×50 front-panel interaction contract for the Flux Purr hotplate. Storybook is the visual source for the two-stage rollout: key-test calibration first, then dashboard/menu mock navigation.',
      },
    },
  },
  args: {
    screen: frontPanelStoryStates.dashboard,
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
                160×50 front-panel interaction gallery
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
                Stage one verifies the five-way key mapping with short, double, and long gestures.
                Stage two keeps dashboard, menu, and child-page behavior fully mock-driven so the UI
                can be validated before any real heater or fan wiring lands.
              </p>
            </div>

            <div
              style={{
                display: 'grid',
                gap: '12px',
                minWidth: '240px',
              }}
            >
              {[
                ['Theme', 'Dark embedded UI'],
                ['Screen set', '7 core screens'],
                ['Gestures', 'Short / Double / Long'],
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

export const KeyTestIdle: Story = {
  args: {
    screen: frontPanelStoryStates.keyTestIdle,
  },
}

export const KeyTestShort: Story = {
  args: {
    screen: frontPanelStoryStates.keyTestShort,
  },
}

export const KeyTestDouble: Story = {
  args: {
    screen: frontPanelStoryStates.keyTestDouble,
  },
}

export const KeyTestLong: Story = {
  args: {
    screen: frontPanelStoryStates.keyTestLong,
  },
}

export const Dashboard: Story = {
  args: {
    screen: frontPanelStoryStates.dashboard,
  },
}

export const DashboardManual: Story = {
  args: {
    screen: frontPanelStoryStates.dashboardManual,
  },
}

export const Menu: Story = {
  args: {
    screen: frontPanelStoryStates.menu,
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

export const KeyTestInteractions: Story = {
  name: 'Interaction / Key Test',
  render: () => <FrontPanelRuntimeHarness mode="key-test" scale={6} />,
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const debug = await canvas.findByTestId('frontpanel-runtime-debug')

    await step('short press keeps success color semantics', async () => {
      await userEvent.click(await canvas.findByTestId('frontpanel-action-up-short'))
      await expect(debug).toHaveTextContent('keyTest: U / U / SHORT')
      await expect(debug).toHaveTextContent('route: key-test')
    })

    await step('double press reports accent gesture', async () => {
      await userEvent.click(await canvas.findByTestId('frontpanel-action-right-double'))
      await expect(debug).toHaveTextContent('keyTest: R / R / DOUBLE')
    })

    await step('long press reports info-cyan gesture', async () => {
      await userEvent.click(await canvas.findByTestId('frontpanel-action-left-long'))
      await expect(debug).toHaveTextContent('keyTest: D / L / LONG')
    })
  },
}

export const AppInteractionFlow: Story = {
  name: 'Interaction / App Flow',
  render: () => <FrontPanelRuntimeHarness mode="app" scale={6} />,
  play: async ({ canvasElement, step }) => {
    const canvas = within(canvasElement)
    const debug = await canvas.findByTestId('frontpanel-runtime-debug')

    await step('dashboard short and double presses stay on dashboard', async () => {
      await expect(debug).toHaveTextContent('route: dashboard')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-up-short'))
      await expect(debug).toHaveTextContent('targetTempC: 381')
      for (let index = 0; index < 19; index += 1) {
        await userEvent.click(await canvas.findByTestId('frontpanel-action-up-short'))
      }
      await expect(debug).toHaveTextContent('targetTempC: 400')
      await expect(debug).toHaveTextContent('selectedPresetIndex: 4')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-short'))
      await expect(debug).toHaveTextContent('heaterEnabled: true')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-double'))
      await expect(debug).toHaveTextContent('fanEnabled: true')
    })

    await step('center long enters menu and active cooling mock page', async () => {
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-long'))
      await expect(debug).toHaveTextContent('route: menu')
      await expect(debug).toHaveTextContent('selectedMenuItem: active-cooling')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-short'))
      await expect(debug).toHaveTextContent('route: active-cooling')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-right-short'))
      await expect(debug).toHaveTextContent('activeCooling: true / boost')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-up-short'))
      await expect(debug).toHaveTextContent('activeCooling: false / boost')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-long'))
      await expect(debug).toHaveTextContent('route: menu')
    })

    await step('left moves to preset temp and exit fallback returns dashboard', async () => {
      await userEvent.click(await canvas.findByTestId('frontpanel-action-left-short'))
      await expect(debug).toHaveTextContent('selectedMenuItem: preset-temp')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-short'))
      await expect(debug).toHaveTextContent('route: preset-temp')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-up-short'))
      await expect(debug).toHaveTextContent('targetTempC: 181')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-short'))
      await expect(debug).toHaveTextContent('route: menu')
      await userEvent.click(await canvas.findByTestId('frontpanel-action-center-long'))
      await expect(debug).toHaveTextContent('route: dashboard')
    })
  },
}
