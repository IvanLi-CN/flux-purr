import type { Meta, StoryObj } from '@storybook/react-vite'
import { useState } from 'react'
import { expect, fireEvent, fn, userEvent, within } from 'storybook/test'
import {
  createDefaultThermalPlantSnapshot,
  createEmptyThermalPlantSnapshot,
  ThermalPlantRunCard,
} from '@/features/control-plane-demo/components/thermal-plant-run-card'
import { TemperatureRequestChart } from '@/features/control-plane-demo/components/thermal-supply-observations'
import type {
  SupplyBoundary,
  ThermalPlantRunSnapshot,
} from '@/features/control-plane-demo/contracts'
import observedBoundary from '@/features/control-plane-demo/fixtures/supply-observations.json'

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
  tracePage: {
    ...completedSnapshot.tracePage,
    points: completedSnapshot.tracePage.points.map((point, index) =>
      index === completedSnapshot.tracePage.points.length - 1
        ? { ...point, temperatureCentiC: 9000 }
        : point
    ),
  },
  attempt: {
    ...completedAttempt,
    status: 'failed',
    currentTempCentiC: 9000,
    phase: 'cooling',
    restartAllowed: true,
    error: 'thermal_plant_timeout',
    failureContext: {
      stage: 'cooling',
      elapsedMs: 450000,
      stageElapsedMs: 300000,
      condition: 'passive_temperature_at_most_80c',
    },
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

export const ObservedSixteenPoints: Story = {
  args: {
    snapshot: {
      ...failedSnapshot,
      supplyBoundary: observedBoundary as SupplyBoundary,
      supplyBoundarySource: 'attempt',
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    const points = canvas.getAllByRole('button', { name: /^观测 / })
    await expect(points).toHaveLength(16)
    await expect(canvas.getByText('5.5', { exact: true })).toBeVisible()
    await expect(canvas.getByText('28.0', { exact: true })).toBeVisible()
    await expect(canvas.queryByText('0.0', { exact: true })).not.toBeInTheDocument()
    await expect(canvas.getByLabelText('3.00 A 温度—申请电压拟合曲线')).toBeInTheDocument()
    const fitPath = canvas.getByLabelText(
      '3.00 A 温度—申请电压拟合曲线'
    ) as unknown as SVGGraphicsElement
    const temperatureAxis = canvasElement.querySelector(
      '.thermal-supply-chart svg line'
    ) as SVGGraphicsElement
    await expect(fitPath.getBBox().y).toBeCloseTo(temperatureAxis.getBBox().y, 1)
    await expect(fitPath.getBBox().height).toBeCloseTo(temperatureAxis.getBBox().height, 1)
    const traceSvg = canvasElement.querySelector(
      '.thermal-plant-run-card__svg--trace'
    ) as SVGSVGElement
    const traceLine = traceSvg.querySelector(
      '.thermal-plant-run-card__trace--heating'
    ) as SVGGraphicsElement
    const supplySvg = fitPath.closest('svg') as SVGSVGElement
    await expect(supplySvg.viewBox.baseVal.width).toBe(traceSvg.viewBox.baseVal.width)
    await expect(getComputedStyle(supplySvg.querySelector('text') as Element).fontSize).toBe(
      getComputedStyle(traceSvg.querySelector('text') as Element).fontSize
    )
    await expect(getComputedStyle(fitPath).stroke).toBe(getComputedStyle(traceLine).stroke)
    await expect(getComputedStyle(fitPath).strokeWidth).toBe(
      getComputedStyle(traceLine).strokeWidth
    )

    const footers = canvasElement.querySelectorAll('.thermal-chart__footer')
    await expect(footers).toHaveLength(2)
    for (const footer of footers) {
      const detail = footer.querySelector('output') as HTMLElement
      const label = footer.querySelector('.thermal-chart__axis-title') as HTMLElement
      const lineHeight = Number.parseFloat(getComputedStyle(detail).lineHeight)
      await expect(footer.getBoundingClientRect().height).toBeLessThanOrEqual(lineHeight + 1)
      await expect(detail.getBoundingClientRect().top).toBeCloseTo(
        label.getBoundingClientRect().top,
        0
      )
    }

    await expect(canvas.queryByLabelText('2.85 A 温度—申请电压拟合曲线')).not.toBeInTheDocument()
    await expect(canvas.queryByText(/29.8℃ · 申请 9.5 V/)).not.toBeInTheDocument()
    await expect(canvas.queryByText(/合同 2.85 A · RTD 1324 mV/)).not.toBeInTheDocument()
    await expect(canvasElement.querySelector('.thermal-chart__detail')).toBeNull()
    const tableScroll = canvasElement.querySelector(
      '.thermal-plant-run-card__table-scroll .simplebar-content-wrapper'
    ) as HTMLElement
    const tableWrap = canvasElement.querySelector(
      '.thermal-plant-run-card__table-wrap'
    ) as HTMLElement
    const tableHead = tableScroll.querySelector('thead') as HTMLElement
    await expect(tableScroll.scrollHeight).toBeGreaterThan(tableScroll.clientHeight)
    await expect(getComputedStyle(tableScroll).overflowY).toBe('scroll')
    await expect(getComputedStyle(tableHead).position).toBe('sticky')
    await expect(getComputedStyle(tableHead).backgroundColor).toBe(
      getComputedStyle(tableWrap).backgroundColor
    )

    const expectSelectedRowBelowStickyHeader = async () => {
      const deadline = performance.now() + 1_000
      let selectedRowRect: DOMRect | undefined
      let tableScrollRect: DOMRect | undefined
      let tableHeadRect: DOMRect | undefined
      while (performance.now() < deadline) {
        const selectedRow = tableScroll.querySelector('tr[aria-selected="true"]') as HTMLElement
        tableScrollRect = tableScroll.getBoundingClientRect()
        tableHeadRect = tableHead.getBoundingClientRect()
        selectedRowRect = selectedRow.getBoundingClientRect()
        if (
          selectedRowRect.top >= tableHeadRect.bottom - 1 &&
          selectedRowRect.bottom <= tableScrollRect.bottom + 1
        ) {
          return
        }
        await new Promise((resolve) => window.setTimeout(resolve, 50))
      }
      if (!selectedRowRect || !tableScrollRect || !tableHeadRect) {
        throw new Error('selected observation geometry was not available')
      }
      await expect(selectedRowRect.top).toBeGreaterThanOrEqual(tableHeadRect.bottom - 1)
      await expect(selectedRowRect.bottom).toBeLessThanOrEqual(tableScrollRect.bottom + 1)
    }

    await userEvent.click(points[15])
    await expect(points[15]).toHaveAttribute('aria-pressed', 'true')
    await expectSelectedRowBelowStickyHeader()
    const supplyRect = supplySvg.getBoundingClientRect()
    fireEvent.pointerMove(supplySvg, {
      clientX: supplyRect.left + supplyRect.width / 2,
      clientY: supplyRect.top + supplyRect.height / 2,
    })
    await expect(
      canvasElement.querySelector('.thermal-supply-chart .thermal-chart__tooltip')
    ).not.toBeNull()
    await userEvent.click(points[0])
    await expect(points[0]).toHaveAttribute('aria-pressed', 'true')
    await expectSelectedRowBelowStickyHeader()
    await points[3].focus()
    await userEvent.keyboard('{Enter}')
    await expect(points[3]).toHaveAttribute('aria-pressed', 'true')
    await userEvent.click(canvas.getByRole('button', { name: '选择观测 1' }))
    await expect(points[0]).toHaveAttribute('aria-pressed', 'true')

    traceSvg.focus()
    fireEvent.keyDown(traceSvg, { key: 'ArrowRight' })
    await expect(
      canvasElement.querySelector('.thermal-plant-run-card__svg--trace ~ .thermal-chart__tooltip')
    ).not.toBeNull()
    await expect(
      canvasElement.querySelector('.thermal-plant-run-card__svg--trace ~ .thermal-chart__tooltip')
        ?.textContent
    ).toContain('时间')
  },
}
export const NarrowObserved: Story = {
  ...ObservedSixteenPoints,
  globals: { viewport: { value: 'fluxPurrMobile', isRotated: false } },
  parameters: {
    viewport: {
      options: { mobile1: { name: '审查窄屏', styles: { width: '393px', height: '852px' } } },
    },
  },
}
export const LegacyUnlabelled: Story = {
  args: { snapshot: { ...completedSnapshot, supplyBoundarySource: undefined, attempt: null } },
}
export const InvalidRtdMetadata: Story = {
  args: {
    snapshot: {
      ...completedSnapshot,
      supplyBoundary: {
        ...observedBoundary,
        adc: { ...observedBoundary.adc, dividerSupplyMv: 0 },
      } as SupplyBoundary,
    },
  },
}
export const Disconnected: Story = {
  args: { snapshot: failedSnapshot, readError: '设备已断开', disabled: true },
}

export const NarrowChart: Story = {
  globals: { viewport: { value: 'fluxPurrMobile', isRotated: false } },
  render: function NarrowChartStory() {
    const [selectedIndex, setSelectedIndex] = useState(0)
    return (
      <div style={{ height: 300, display: 'grid' }}>
        <TemperatureRequestChart
          boundary={observedBoundary as SupplyBoundary}
          loading={false}
          running={false}
          selectedIndex={selectedIndex}
          onSelect={setSelectedIndex}
        />
      </div>
    )
  },
  play: async ({ canvasElement }) => {
    const chart = canvasElement.querySelector('.thermal-supply-chart')
    const detail = chart?.querySelector('output')
    const plot = chart?.querySelector('.thermal-plant-run-card__chart-canvas')
    if (!detail || !plot) throw new Error('Missing chart surface')
    const style = getComputedStyle(detail)
    await expect(style.whiteSpace).toBe('nowrap')
    await expect(detail.getBoundingClientRect().height).toBeLessThanOrEqual(
      Number.parseFloat(style.lineHeight) + 1
    )
    await expect(plot.getBoundingClientRect().height).toBeGreaterThanOrEqual(224)
  },
}
