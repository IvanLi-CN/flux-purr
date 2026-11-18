import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'
import { TuningWriteNotice } from '@/features/control-plane-demo/components/tuning-write-notice'

const meta = {
  title: 'App/TuningWriteNotice',
  component: TuningWriteNotice,
  tags: ['autodocs'],
  parameters: { layout: 'centered' },
  decorators: [
    (Story) => (
      <div
        data-visual-evidence-surface
        style={{
          width: 'min(100vw - 2rem, 64rem)',
          padding: 24,
          background: 'var(--industrial-bg)',
        }}
      >
        <div data-visual-evidence-target>
          <Story />
        </div>
      </div>
    ),
  ],
  args: { stopping: false, onStop: fn() },
} satisfies Meta<typeof TuningWriteNotice>

export default meta
type Story = StoryObj<typeof meta>

export const Running: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('status')).toHaveTextContent('其他写操作已停用')
    await userEvent.click(canvas.getByRole('button', { name: '停止调优' }))
    await expect(args.onStop).toHaveBeenCalledTimes(1)
  },
}

export const Stopping: Story = {
  args: { stopping: true },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole('button', { name: '正在停止…' })).toBeDisabled()
  },
}

export const StopFailed: Story = {
  args: { error: '停止请求未完成，请重试。' },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('alert')).toHaveTextContent('停止请求未完成')
    await userEvent.click(canvas.getByRole('button', { name: '停止调优' }))
    await expect(args.onStop).toHaveBeenCalledTimes(1)
  },
}
