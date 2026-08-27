import type { Meta, StoryObj } from '@storybook/react-vite'
import { fn } from 'storybook/test'
import {
  createDefaultThermalPlantSnapshot,
  ThermalPlantRunCard,
} from '@/features/control-plane-demo/components/thermal-plant-run-card'
import type { ThermalPlantRunSnapshot } from '@/features/control-plane-demo/contracts'

const completedSnapshot = createDefaultThermalPlantSnapshot()
const completedAttempt = requireAttempt(completedSnapshot)
const completedResult = requireActiveResult(completedSnapshot)
const runningSnapshot: ThermalPlantRunSnapshot = {
  ...completedSnapshot,
  attempt: {
    ...completedAttempt,
    status: 'running',
    phase: 'heating',
    progressPercent: 54,
    currentTempCentiC: 13_800,
    heaterVoltageMv: 21_000,
    dutyPercent: 100,
    restartAllowed: false,
  },
  provisionalCurve: {
    state: 'preview',
    coveragePercent: 75,
    curve: completedResult.curve,
  },
  activeResult: null,
}
const failedSnapshot: ThermalPlantRunSnapshot = {
  ...completedSnapshot,
  attempt: {
    ...completedAttempt,
    status: 'failed',
    phase: 'cooling',
    restartAllowed: true,
    error: '自然冷却阶段未达到 80℃，未覆盖已有 active 结果。',
  },
}

function requireAttempt(snapshot: ThermalPlantRunSnapshot) {
  if (!snapshot.attempt) throw new Error('Story fixture requires an attempt')
  return snapshot.attempt
}

function requireActiveResult(snapshot: ThermalPlantRunSnapshot) {
  if (!snapshot.activeResult) throw new Error('Story fixture requires an active result')
  return snapshot.activeResult
}

const meta = {
  title: 'Calibration/ThermalPlantRunCard',
  component: ThermalPlantRunCard,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
  decorators: [
    (Story) => (
      <div style={{ width: 'min(100vw - 2rem, 72rem)' }}>
        <Story />
      </div>
    ),
  ],
  args: {
    snapshot: completedSnapshot,
    onStartStop: fn(),
  },
} satisfies Meta<typeof ThermalPlantRunCard>

export default meta
type Story = StoryObj<typeof meta>

export const Completed: Story = {}

export const Running: Story = {
  args: { snapshot: runningSnapshot },
}

export const Failed: Story = {
  args: { snapshot: failedSnapshot },
}

export const UnsupportedFirmware: Story = {
  args: {
    snapshot: {
      version: 1,
      attempt: null,
      tracePage: { startSample: 0, nextSample: null, totalSamples: 0, points: [] },
      provisionalCurve: null,
      activeResult: null,
    },
    unsupported: true,
  },
}
