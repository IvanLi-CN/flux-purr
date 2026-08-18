import { describe, expect, it, vi } from 'vitest'

import type { BrowserSerial, BrowserSerialPort } from '../control-plane-demo/web-serial'
import {
  type BrowserPreflightTraceEvent,
  beginBrowserUsbPreflight,
} from './browser-preflight-trace'

describe('Browser USB preflight trace', () => {
  it('requests the chooser synchronously before any later async preflight work', async () => {
    const port = {} as BrowserSerialPort
    const serial = { requestPort: vi.fn(() => Promise.resolve(port)) } as BrowserSerial
    const trace: BrowserPreflightTraceEvent[] = []

    const selection = beginBrowserUsbPreflight({
      serial,
      now: () => new Date('2026-08-17T08:00:00.000Z'),
      onTrace: (entry) => trace.push(entry),
    })

    expect(serial.requestPort).toHaveBeenCalledOnce()
    expect(serial.requestPort).toHaveBeenCalledWith({
      filters: [{ usbVendorId: 0x303a, usbProductId: 0x1001 }],
    })
    expect(trace.map((entry) => entry.event)).toEqual(['预检已点击', '浏览器 USB 选择器已请求'])
    await expect(selection).resolves.toBe(port)
    expect(trace.at(-1)).toMatchObject({ event: '浏览器 USB 端口已选择', tone: 'success' })
  })

  it('records a rejected chooser with the normalized local error', async () => {
    const serial = {
      requestPort: vi.fn(() => Promise.reject(new Error('No port selected by the user.'))),
    } as BrowserSerial
    const trace: BrowserPreflightTraceEvent[] = []

    await expect(
      beginBrowserUsbPreflight({
        serial,
        onTrace: (entry) => trace.push(entry),
      })
    ).rejects.toMatchObject({ code: 'web_serial_port_not_selected' })

    expect(trace.at(-1)).toMatchObject({
      event: '浏览器 USB 选择器被拒绝',
      detail: '浏览器未确认串口设备。请重新选择 Flux Purr USB JTAG/serial 设备。',
      tone: 'error',
    })
  })
})
