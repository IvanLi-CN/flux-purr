import { describe, expect, it } from 'vitest'
import {
  createWifiPasswordMask,
  formatWifiWaitElapsed,
  isWifiNetworkSettingsDirty,
  isWifiSnapshotOlder,
  resolveWifiSettingsUnavailableReason,
  shouldClearStaleWifiOutcome,
  validateWifiNetworkSettingsDraft,
  wifiConnectionOutcome,
  wifiFailureReason,
} from './wifi-network-settings'

describe('WiFi password presentation', () => {
  it('uses a real masked input value for the saved password length', () => {
    expect(createWifiPasswordMask(4)).toBe('••••')
    expect(createWifiPasswordMask(0)).toBe('')
  })

  it('does not mark an untouched saved configuration dirty', () => {
    expect(
      isWifiNetworkSettingsDirty({
        ssid: 'FluxPurr-Lab',
        savedSsid: 'FluxPurr-Lab',
        password: '•••••••••••',
        passwordMode: 'saved-mask',
        savedPasswordLength: 11,
      })
    ).toBe(false)
  })

  it('marks deleting a saved mask dirty so an explicit empty password can be sent', () => {
    expect(
      isWifiNetworkSettingsDirty({
        ssid: 'FluxPurr-Lab',
        savedSsid: 'FluxPurr-Lab',
        password: '',
        passwordMode: 'draft',
        savedPasswordLength: 11,
      })
    ).toBe(true)
  })
})

describe('validateWifiNetworkSettingsDraft', () => {
  it('requires a non-empty SSID', () => {
    expect(validateWifiNetworkSettingsDraft({ ssid: '   ', password: '' })).toBe(
      '请输入 WiFi 名称。'
    )
  })

  it('enforces firmware byte limits without rejecting an open network', () => {
    expect(
      validateWifiNetworkSettingsDraft({
        ssid: 'FluxPurr-Lab',
        password: '',
      })
    ).toBeNull()
    expect(
      validateWifiNetworkSettingsDraft({
        ssid: '中'.repeat(11),
        password: '',
      })
    ).toBe('WiFi 名称最多 32 个字节。')
    expect(
      validateWifiNetworkSettingsDraft({
        ssid: 'FluxPurr-Lab',
        password: 'a'.repeat(65),
      })
    ).toBe('WiFi 密码最多 64 个字节。')
  })
})

describe('resolveWifiSettingsUnavailableReason', () => {
  it('reports an active transport fault instead of claiming the firmware protocol is missing', () => {
    expect(
      resolveWifiSettingsUnavailableReason({
        supportsWifiStateV2: false,
        transportIssue: '授权串口当前不可用。',
      })
    ).toBe('授权串口当前不可用。')
  })
})

describe('wifiConnectionOutcome', () => {
  it('only maps terminal device states to a result message', () => {
    expect(wifiConnectionOutcome('error')).toBe('WiFi 连接失败：设备未返回具体失败原因。')
    expect(wifiConnectionOutcome('timeout')).toBe('WiFi 连接超时，请检查网络是否可用。')
    expect(wifiConnectionOutcome('connecting')).toBeNull()
  })

  it('maps success and terminal failure states', () => {
    expect(wifiConnectionOutcome('connected')).toBe('WiFi 已连接。')
    expect(wifiConnectionOutcome('error', 'association_rejected')).toBe(
      'WiFi 连接失败：接入点拒绝认证，请检查 WiFi 名称和密码。'
    )
    expect(wifiConnectionOutcome('timeout')).toBe('WiFi 连接超时，请检查网络是否可用。')
  })

  it('never manufactures a timeout for a connecting device', () => {
    expect(wifiConnectionOutcome('connecting')).toBeNull()
    expect(wifiConnectionOutcome('saving')).toBeNull()
  })

  it('treats disabled as a terminal outcome only for a clear receipt', () => {
    expect(wifiConnectionOutcome('disabled')).toBeNull()
  })
})

describe('WiFi connection diagnostics', () => {
  it('makes every firmware failure code actionable without exposing credentials', () => {
    expect(wifiFailureReason('disconnect_timed_out')).toBe('连接完成前设备网络断开。')
    expect(wifiFailureReason('configuration_failed')).toBe('设备未能应用 WiFi 配置。')
    expect(wifiFailureReason('association_rejected')).toBe(
      '接入点拒绝认证，请检查 WiFi 名称和密码。'
    )
    expect(wifiFailureReason('association_timed_out')).toBe('等待接入点响应超时。')
    expect(wifiFailureReason('ipv4_timed_out')).toBe('已连接接入点，但未能获得 IPv4 地址。')
    expect(wifiFailureReason('station_disconnected')).toBe('设备与接入点断开。')
    expect(wifiFailureReason('lan_startup_failed')).toBe('设备网络接口启动失败。')
    expect(wifiFailureReason(null)).toBe('设备未返回具体失败原因。')
  })

  it('formats elapsed host wait time for the status line', () => {
    expect(formatWifiWaitElapsed(0)).toBe('0 秒')
    expect(formatWifiWaitElapsed(9.9)).toBe('9 秒')
    expect(formatWifiWaitElapsed(10)).toBe('10 秒')
    expect(formatWifiWaitElapsed(65)).toBe('1 分 05 秒')
  })
})

describe('shouldClearStaleWifiOutcome', () => {
  it('clears a terminal failure toast when a newer device phase replaces it', () => {
    expect(
      shouldClearStaleWifiOutcome(
        'connecting',
        'WiFi 连接失败：接入点拒绝认证，请检查 WiFi 名称和密码。'
      )
    ).toBe(true)
    expect(shouldClearStaleWifiOutcome('connecting', 'WiFi 连接超时，请检查网络是否可用。')).toBe(
      true
    )
    expect(shouldClearStaleWifiOutcome('connecting', 'WiFi 已连接。')).toBe(true)
    expect(shouldClearStaleWifiOutcome('saving', 'WiFi 连接失败：设备未返回具体失败原因。')).toBe(
      true
    )
    expect(
      shouldClearStaleWifiOutcome('connected', 'WiFi 连接失败：设备未返回具体失败原因。')
    ).toBe(true)
  })
})

describe('isWifiSnapshotOlder', () => {
  it('rejects an older generation or sequence without rejecting an equal receipt', () => {
    const receipt = { configurationGeneration: 4, transitionSequence: 12 }
    expect(
      isWifiSnapshotOlder({ configurationGeneration: 3, transitionSequence: 99 }, receipt)
    ).toBe(true)
    expect(
      isWifiSnapshotOlder({ configurationGeneration: 4, transitionSequence: 11 }, receipt)
    ).toBe(true)
    expect(
      isWifiSnapshotOlder({ configurationGeneration: 4, transitionSequence: 12 }, receipt)
    ).toBe(false)
    expect(
      isWifiSnapshotOlder({ configurationGeneration: 5, transitionSequence: 1 }, receipt)
    ).toBe(false)
  })
})
