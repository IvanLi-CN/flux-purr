import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import {
  createDefaultThermalPlantSnapshot,
  createEmptyThermalPlantSnapshot,
  ThermalPlantRunCard,
} from '@/features/control-plane-demo/components/thermal-plant-run-card'
import type { ThermalPlantRunSnapshot } from '@/features/control-plane-demo/contracts'

const completedSnapshot = createDefaultThermalPlantSnapshot()
const emptySnapshot = createEmptyThermalPlantSnapshot()
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
      <div
        data-visual-evidence-surface
        style={{
          width: 'min(100vw - 2rem, 72rem)',
          padding: '24px',
          background: 'var(--industrial-panel)',
        }}
      >
        <div data-visual-evidence-target>
          <Story />
        </div>
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

export const IdleReady: Story = {
  name: 'Idle / Ready to calibrate',
  args: { snapshot: emptySnapshot },
}

export const Loading: Story = {
  name: 'Loading live snapshot',
  args: { snapshot: emptySnapshot, loading: true },
}

export const LoadingRunning: Story = {
  name: 'Running / Snapshot pending',
  args: { snapshot: emptySnapshot, loading: true, runtimeRunning: true },
  play: async ({ canvasElement, args }) => {
    const stop = within(canvasElement).getByRole('button', { name: '停止自动校准' })
    await expect(stop).toBeEnabled()
    await userEvent.click(stop)
    await expect(args.onStartStop).toHaveBeenCalledOnce()
  },
}

export const StaleRunningSnapshot: Story = {
  name: 'Stopped / Snapshot stale',
  args: { snapshot: runningSnapshot, runtimeRunning: false },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.queryByRole('button', { name: '停止自动校准' })).not.toBeInTheDocument()
    await expect(canvas.getByRole('button', { name: '重新开始自动校准' })).toBeDisabled()
  },
}

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
