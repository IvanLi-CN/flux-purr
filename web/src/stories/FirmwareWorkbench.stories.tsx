import type { Meta, StoryObj } from '@storybook/react-vite'
import { expect, fn, userEvent, within } from 'storybook/test'

import { FirmwareWorkbench } from '@/features/firmware-installer'

const meta = {
  title: 'Components/FirmwareWorkbench',
  component: FirmwareWorkbench,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
  },
  decorators: [
    (Story) => (
      <div
        data-testid="firmware-evidence-surface"
        style={{
          width: 'min(936px, 100vw)',
          boxSizing: 'border-box',
          margin: 0,
          padding: 'clamp(16px, 2vw, 18px)',
          background: '#fcfcf7',
        }}
      >
        <div style={{ width: '100%', overflow: 'hidden' }}>
          <div
            className="industrial-shell"
            style={{
              width: '100%',
              boxSizing: 'border-box',
              margin: 0,
              padding: '16px',
              background: 'var(--industrial-bg)',
            }}
          >
            <Story />
          </div>
        </div>
      </div>
    ),
  ],
  args: {
    devdAvailable: true,
    browserAvailable: true,
    updateEligible: true,
    currentVersion: '1.2.3',
    currentTemperatureC: 25,
    heaterEnabled: false,
    onPreflight: fn(),
    onInstall: fn(),
  },
} satisfies Meta<typeof FirmwareWorkbench>

export default meta
type Story = StoryObj<typeof meta>

export const UpdateReady: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('button', { name: /更新现有设备/ })).toHaveClass('is-active')
    await expect(canvas.getByLabelText('本机 devd推荐')).toBeChecked()
    await userEvent.click(canvas.getByRole('button', { name: '运行预检' }))
    await expect(canvas.getByText('devd 预检需要在线设备和有效租约。')).toBeVisible()
  },
}

export const RecoveryForForeignFirmware: Story = {
  args: {
    devdAvailable: false,
    browserAvailable: true,
    updateEligible: false,
    message: 'ROM 目标可用于完整擦除和安装。',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement)
    await expect(canvas.getByRole('button', { name: /更新现有设备/ })).toBeDisabled()
    await expect(canvas.getByRole('button', { name: /安装或恢复/ })).toHaveClass('is-active')
    await expect(canvas.getByLabelText('浏览器 USBChrome / Edge')).toBeChecked()
  },
}

export const SecurityBlocked: Story = {
  args: {
    outcome: 'blocked',
    message: 'Secure Boot 已启用，预检已阻止写入。',
  },
}

export const WriteCompleteUnverified: Story = {
  args: {
    outcome: 'write_complete_unverified',
    progress: 100,
    message: '三段 ROM MD5 已通过，但目标运行时未在时限内重连。',
  },
}

export const Busy: Story = {
  args: {
    busy: true,
    outcome: 'running',
    progress: 58,
    message: '正在逐段写入固件。',
  },
}

export const MobileRecovery: Story = {
  args: {
    devdAvailable: false,
    browserAvailable: true,
    updateEligible: false,
  },
  parameters: {
    viewport: {
      options: {
        responsive393: {
          name: 'Responsive review 393x852',
          styles: { width: '393px', height: '852px' },
          type: 'mobile',
        },
      },
    },
  },
  globals: {
    viewport: { value: 'responsive393', isRotated: false },
  },
}
