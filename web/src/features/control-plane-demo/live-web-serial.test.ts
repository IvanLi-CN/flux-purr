import { describe, expect, it } from 'vitest'
import { formatWebSerialWifiUpdateFailure } from './live-web-serial'
import { ControlPlaneClientError } from './transport-client'

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
