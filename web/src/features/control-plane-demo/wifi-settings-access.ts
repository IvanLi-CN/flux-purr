import type { DeviceTarget } from './types'

export type WifiSettingsAccessMode = 'hidden' | 'read-only' | 'read-write'

export interface WifiSettingsAccess {
  mode: WifiSettingsAccessMode
  reason?: string
}

export function resolveWifiSettingsAccess(
  device: Pick<
    DeviceTarget,
    | 'transport'
    | 'bridgeTransport'
    | 'capabilities'
    | 'leaseId'
    | 'leaseState'
    | 'severity'
    | 'connectionAvailable'
    | 'networkState'
  >
): WifiSettingsAccess {
  const hasWifiCapability =
    device.capabilities.includes('wifi_config') || device.capabilities.includes('wifi_state_v2')
  if (!hasWifiCapability) {
    return { mode: 'hidden' }
  }

  if (device.severity === 'offline' || device.connectionAvailable === false) {
    return { mode: 'read-only', reason: '目标设备当前离线，只能查看最近一次 WiFi 状态。' }
  }

  const isUsbConfigurationTransport =
    (device.transport === 'devd' && device.bridgeTransport === 'usb') ||
    device.transport === 'serial'
  const supportsWifiConfig = device.capabilities.includes('wifi_config')
  const supportsWifiStateV2 = device.capabilities.includes('wifi_state_v2')
  const hasActiveAuthority =
    device.transport === 'serial'
      ? device.leaseState === 'active'
      : Boolean(device.leaseId && device.leaseState === 'active')

  if (
    isUsbConfigurationTransport &&
    supportsWifiConfig &&
    supportsWifiStateV2 &&
    hasActiveAuthority
  ) {
    return { mode: 'read-write' }
  }

  if (!isUsbConfigurationTransport) {
    return {
      mode: 'read-only',
      reason: '当前通过 WiFi / LAN 连接，只能查看网络信息；请通过 USB 配置连接修改 WiFi。',
    }
  }
  if (!supportsWifiConfig) {
    return { mode: 'read-only', reason: '当前设备固件不支持 WiFi 配置。' }
  }
  if (!supportsWifiStateV2) {
    return { mode: 'read-only', reason: '当前设备固件需要 WiFi 状态协议更新后才能提交设置。' }
  }
  return { mode: 'read-only', reason: '正在获取 USB 配置授权，请稍候再提交 WiFi 设置。' }
}
