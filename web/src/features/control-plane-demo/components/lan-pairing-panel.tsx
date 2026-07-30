import { LoaderCircle, MonitorSmartphone, Wifi } from 'lucide-react'
import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  claimLanPairing,
  isChromiumPrivateNetworkSupported,
  type LanDeviceSession,
  probeLanDevice,
} from '../lan-client'
import { ControlPlaneClientError } from '../transport-client'

interface LanPairingPanelProps {
  onPaired?: (session: LanDeviceSession, probe: Awaited<ReturnType<typeof probeLanDevice>>) => void
  initialAddress?: string
  supported?: boolean
  pairDevice?: (
    address: string,
    code: string
  ) => Promise<{
    session: LanDeviceSession
    probe: Awaited<ReturnType<typeof probeLanDevice>>
  }>
}

export function LanPairingPanel({
  onPaired,
  initialAddress = 'http://192.168.1.18',
  supported = isChromiumPrivateNetworkSupported(),
  pairDevice = pairLanDevice,
}: LanPairingPanelProps) {
  const [address, setAddress] = useState(initialAddress)
  const [code, setCode] = useState('')
  const [state, setState] = useState<'idle' | 'pairing' | 'paired' | 'error'>('idle')
  const [message, setMessage] = useState('')

  const submit = async () => {
    setState('pairing')
    setMessage('')
    try {
      const { session, probe } = await pairDevice(address, code)
      setState('paired')
      setMessage(session.hostname ? `已连接 ${session.hostname}` : '设备已配对')
      onPaired?.(session, probe)
    } catch (error) {
      const detail = error instanceof ControlPlaneClientError ? error.message : '无法连接到设备。'
      setState('error')
      setMessage(detail)
    }
  }

  return (
    <section className="industrial-lan-pairing-panel" aria-label="WiFi LAN pairing">
      <div className="industrial-lan-pairing-panel__heading">
        <span className="industrial-lan-pairing-panel__icon">
          <Wifi size={16} aria-hidden="true" />
        </span>
        <div>
          <strong>WiFi / LAN</strong>
          <small>输入设备地址和 WiFi Info 页面显示的四位码</small>
        </div>
      </div>
      {!supported ? (
        <output className="industrial-lan-pairing-panel__unsupported">
          <MonitorSmartphone size={16} aria-hidden="true" />
          此浏览器不支持 HTTPS 页面直连 HTTP 私网设备。请使用 Chrome、Chromium 或 Edge。
        </output>
      ) : (
        <div className="industrial-lan-pairing-panel__fields">
          <label htmlFor="lan-pairing-address">
            <span>设备地址</span>
            <Input
              id="lan-pairing-address"
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              inputMode="url"
            />
          </label>
          <label htmlFor="lan-pairing-code">
            <span>四位配对码</span>
            <Input
              id="lan-pairing-code"
              value={code}
              onChange={(event) => setCode(event.target.value.replace(/\D/g, '').slice(0, 4))}
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={4}
              placeholder="0000"
            />
          </label>
          <Button
            type="button"
            onClick={() => void submit()}
            disabled={state === 'pairing' || code.length !== 4}
          >
            {state === 'pairing' ? (
              <LoaderCircle className="animate-spin" aria-hidden="true" />
            ) : null}
            {state === 'paired' ? '已配对' : '配对设备'}
          </Button>
        </div>
      )}
      {message ? (
        <p
          className={`industrial-lan-pairing-panel__message industrial-lan-pairing-panel__message--${state}`}
        >
          {message}
        </p>
      ) : null}
    </section>
  )
}

async function pairLanDevice(address: string, code: string) {
  const session = await claimLanPairing(address, code)
  const probe = await probeLanDevice(session)
  return { session, probe }
}
