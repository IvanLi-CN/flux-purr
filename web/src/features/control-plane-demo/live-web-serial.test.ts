import { describe, expect, it } from 'vitest'
import {
  appendWebSerialEvent,
  appendWebSerialEvents,
  formatWebSerialWifiUpdateFailure,
} from './live-web-serial'
import { ControlPlaneClientError } from './transport-client'
import type { EventLogEntry } from './types'

describe('Web Serial WiFi write feedback', () => {
  it('maps a known browser transport error to a Chinese operator message', () => {
    expect(formatWebSerialWifiUpdateFailure(new Error('Web Serial WiFi transport lost.'))).toBe(
      '浏览器 Web Serial 连接已中断，WiFi 设置未能提交。请重新连接设备后重试。'
    )
  })

  it('preserves the raw message for an unclassified write failure', () => {
    expect(
      formatWebSerialWifiUpdateFailure(
        new ControlPlaneClientError('Firmware returned an unsuccessful USB response.', 'usb_error')
      )
    ).toBe('Firmware returned an unsuccessful USB response.')
  })

  it('uses a Chinese fallback when an unknown failure has no message', () => {
    expect(formatWebSerialWifiUpdateFailure(undefined)).toBe(
      '浏览器 Web Serial 未能提交 WiFi 设置。请确认设备仍已连接后重试。'
    )
  })
})

describe('Web Serial event feed', () => {
  const event = (time: string): EventLogEntry => ({
    time,
    source: 'webserial',
    message: `event-${time}`,
    tone: 'info',
  })

  it('keeps older events above newer events', () => {
    const current = [event('09:59:51'), event('09:59:54')]

    expect(appendWebSerialEvent(current, event('09:59:58')).map((item) => item.time)).toEqual([
      '09:59:51',
      '09:59:54',
      '09:59:58',
    ])
  })

  it('keeps the newest 1000 events when the feed reaches its limit', () => {
    const current = Array.from({ length: 1_000 }, (_, index) => event(String(index)))

    const next = appendWebSerialEvent(current, event('1000'))

    expect(next).toHaveLength(1_000)
    expect(next[0]?.time).toBe('1')
    expect(next.at(-1)?.time).toBe('1000')
  })

  it('appends Web Serial events after the existing scenario events', () => {
    const merged = appendWebSerialEvents([event('09:59:48')], [event('09:59:51')])

    expect(merged.map((item) => item.time)).toEqual(['09:59:48', '09:59:51'])
  })
})
