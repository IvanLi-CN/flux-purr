import { LanPairingPanel } from '@/features/control-plane-demo/components/lan-pairing-panel'
import type { LanProbe } from '@/features/control-plane-demo/lan-client'

const mockProbe: LanProbe = {
  identity: {
    deviceId: '001122334455',
    firmwareVersion: 'fw/v0.4.0',
    buildId: 's3-lan-demo',
    gitSha: 'demo',
    board: 'esp32s3_frontpanel',
    apiVersion: 'v1',
    protocolVersion: 'flux-purr.usb.v1',
    hostname: 'flux-purr-001122334455',
    capabilities: ['status', 'runtime', 'lan_http'],
  },
  network: { state: 'connected', ip: '192.168.1.18', wifiRssi: -48 },
  status: {
    mode: 'idle',
    uptimeSeconds: 3723,
    currentTempC: 25.4,
    targetTempC: 120,
    heaterEnabled: false,
    heaterOutputPercent: 0,
    activeCoolingEnabled: true,
    fanDisplayState: 'AUTO',
    fanEnabled: true,
    fanPwmPermille: 400,
    voltageMv: 20000,
    currentMa: 0,
    boardTempCenti: 2840,
    pdRequestMv: 20000,
    pdContractMv: 20000,
    pdState: 'ready',
    calibration: {
      mode: 'off',
      ppsEnabled: false,
      heaterEnabled: false,
      stable: false,
      job: { status: 'idle', progressPercent: 0, samplesCollected: 0 },
    },
    network: { state: 'connected' },
  },
}

export function UiDemo() {
  const params = new URLSearchParams(window.location.search)
  if (params.get('uiDemo') !== 'lan-pairing') {
    return null
  }
  return (
    <main className="industrial-ui-demo" aria-label="LAN pairing demo">
      <LanPairingPanel
        initialAddress="http://192.168.1.18"
        pairDevice={async (address, code) => {
          if (code !== '4827') throw new Error('配对码无效。')
          return {
            session: {
              baseUrl: address.replace(/\/$/, ''),
              token: 'a'.repeat(64),
              hostname: mockProbe.identity.hostname,
            },
            probe: mockProbe,
          }
        }}
      />
    </main>
  )
}
