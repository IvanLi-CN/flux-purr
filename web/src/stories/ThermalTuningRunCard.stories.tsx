import type { Meta, StoryObj } from '@storybook/react-vite'
import { fn } from 'storybook/test'
import {
  createDefaultThermalTuningSnapshot,
  ThermalTuningRunCard,
} from '@/features/control-plane-demo/components/thermal-tuning-card'
import type { ThermalTuningRunSnapshot } from '@/features/control-plane-demo/contracts'

const idleSnapshot = createDefaultThermalTuningSnapshot()
const runningSnapshot: ThermalTuningRunSnapshot = {
  ...idleSnapshot,
  run: {
    ...idleSnapshot.run,
    runId: 'run-pps3a-001',
    state: 'running',
    powerClass: 'pps3a',
    phase: 'retune',
    currentTargetC: 140,
    targetProgress: { acceptedC: [60, 240], failedC: [], skippedC: [] },
    review: { ...idleSnapshot.run.review, state: 'recording' },
    candidate: {
      ...idleSnapshot.run.candidate,
      powerClass: 'pps3a',
      promotionState: 'awaiting_review',
    },
  },
  page: {
    ...idleSnapshot.page,
    emittedThrough: 18,
    nextAfterSequence: 19,
    events: [
      {
        sequence: 17,
        elapsedMs: 1_120_000,
        kind: 'sample',
        phase: 'retune',
        targetC: 140,
        temperatureCentiC: 13_920,
      },
      {
        sequence: 18,
        elapsedMs: 1_121_000,
        kind: 'decision',
        phase: 'retune',
        targetC: 140,
        disposition: 'accepted',
        scoreTracking: 92,
        gates: 31,
      },
    ],
  },
}

const reviewReadySnapshot: ThermalTuningRunSnapshot = {
  ...runningSnapshot,
  run: {
    ...runningSnapshot.run,
    state: 'terminal',
    phase: 'terminal',
    terminalDisposition: 'completed',
    review: { ...runningSnapshot.run.review, state: 'complete', acknowledgedThrough: 18 },
    candidate: {
      candidateId: 'candidate-pps3a-001',
      candidateHash: '0123456789abcdef0123456789abcdef',
      powerClass: 'pps3a',
      promotionState: 'previewed',
    },
  },
  page: { ...runningSnapshot.page, acknowledgedThrough: 18, digestThroughPage: '0123456789abcdef' },
}

const traceGapSnapshot: ThermalTuningRunSnapshot = {
  ...reviewReadySnapshot,
  page: {
    ...reviewReadySnapshot.page,
    events: [
      { sequence: 17, elapsedMs: 1_120_000, kind: 'sample', phase: 'retune', targetC: 140 },
      { sequence: 19, elapsedMs: 1_122_000, kind: 'decision', phase: 'retune', targetC: 140 },
    ],
    emittedThrough: 19,
    acknowledgedThrough: 17,
  },
  run: {
    ...reviewReadySnapshot.run,
    review: { ...reviewReadySnapshot.run.review, state: 'incomplete', reason: 'trace_gap' },
    candidate: { ...reviewReadySnapshot.run.candidate, promotionState: 'unavailable' },
  },
}

const meta = {
  title: 'Calibration/ThermalTuningRunCard',
  component: ThermalTuningRunCard,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
  decorators: [
    (Story) => (
      <div
        data-visual-evidence-surface
        style={{
          width: 'min(100vw - 2rem, 72rem)',
          boxSizing: 'border-box',
          padding: '24px',
          background: '#edf2f6',
        }}
      >
        <div data-visual-evidence-target>
          <Story />
        </div>
      </div>
    ),
  ],
  args: {
    deviceId: 'fp-lab-01',
    snapshot: idleSnapshot,
    onCommand: fn(),
  },
} satisfies Meta<typeof ThermalTuningRunCard>

export default meta
type Story = StoryObj<typeof meta>

export const Ready: Story = {}

export const ReadyMobile: Story = {
  parameters: {
    viewport: { defaultViewport: 'thermalTuningMobile' },
  },
}

export const Running: Story = {
  args: { snapshot: runningSnapshot },
}

export const ReviewReady: Story = {
  args: { snapshot: reviewReadySnapshot },
}

export const TraceGap: Story = {
  args: { snapshot: traceGapSnapshot },
}

export const UnsupportedFirmware: Story = {
  args: { unsupported: true },
}
