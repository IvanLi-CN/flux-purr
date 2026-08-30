import type { Preview } from '@storybook/react-vite'
import { MINIMAL_VIEWPORTS } from 'storybook/viewport'
import '../src/index.css'

const thermalTuningMobileViewport = {
  name: '热控调优 · 393x852',
  styles: {
    width: '393px',
    height: '852px',
  },
  type: 'mobile',
} as const

const preview: Preview = {
  parameters: {
    viewport: {
      options: {
        ...MINIMAL_VIEWPORTS,
        fluxPurrMobile: {
          name: 'Flux Purr mobile',
          styles: {
            width: '393px',
            height: '852px',
          },
        },
        thermalTuningMobile: thermalTuningMobileViewport,
      },
    },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },

    a11y: {
      // 'todo' - show a11y violations in the test UI only
      // 'error' - fail CI on a11y violations
      // 'off' - skip a11y checks entirely
      test: 'todo',
    },
  },
}

export default preview
