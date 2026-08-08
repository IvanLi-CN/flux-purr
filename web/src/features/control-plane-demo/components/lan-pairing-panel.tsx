import { LoaderCircle, MonitorSmartphone, ScanSearch, Wifi, X } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  claimLanPairing,
  type DiscoveredLanDevice,
  getLanPairingMetadata,
  getLanPublicInfo,
  isChromiumPrivateNetworkSupported,
  type LanDeviceSession,
  type LanPublicInfo,
  loadLanAddress,
  loadLanScanCidr,
  normalizeLanBaseUrl,
  probeLanDevice,
  scanLanSubnet,
  storeLanAddress,
  storeLanDeviceSession,
  storeLanScanCidr,
} from '../lan-client'
import { ControlPlaneClientError } from '../transport-client'

type LanPairingResult = {
  session: LanDeviceSession
  probe: Awaited<ReturnType<typeof probeLanDevice>>
}

type LanConnectionState = 'idle' | 'connecting' | 'connected' | 'claiming' | 'paired' | 'error'

export interface LanPairingPanelProps {
  onPaired?: (
    session: LanDeviceSession,
    probe: Awaited<ReturnType<typeof probeLanDevice>>
  ) => void | Promise<void>
  initialAddress?: string
  supported?: boolean
  connectDevice?: (address: string) => Promise<LanPublicInfo>
  getPairingMetadata?: (address: string) => ReturnType<typeof getLanPairingMetadata>
  pairDevice?: (address: string, code?: string) => Promise<LanPairingResult>
  scanDevices?: typeof scanLanSubnet
}

export function lanPairingFailureMessage(error: unknown) {
  if (!(error instanceof ControlPlaneClientError)) {
    return '无法连接到设备。请确认设备地址、同网连接和私网访问权限。'
  }

  switch (error.code) {
    case 'lan_private_network_unavailable':
      return '浏览器阻止了对私网设备的访问。请确认使用 Chrome、Chromium 或 Edge，并允许私网访问。'
    case 'pairing_inactive':
      return '配对码尚未开启。请保持硬件的 WiFi Info 页面打开，再检查配对码。'
    case 'pairing_locked':
      return '该配对窗口已因连续失败锁定。请离开并重新进入 WiFi Info 页面获取新码。'
    case 'pairing_code_invalid':
      return '四位配对码不正确，请核对设备屏幕。'
    case 'pairing_unavailable':
      return '此设备不支持 LAN 配对码，只提供基础低频信息读取。'
    case 'lan_response_invalid':
      return '设备返回数据格式异常，配对尚未完成。请更新设备固件后重试。'
    case 'lan_probe_unavailable':
      return '配对凭据已保存，但读取设备状态时连接中断。请重新连接设备。'
    case 'unauthorized':
      return '此浏览器的配对凭据已失效，请重新进行物理配对。'
    default:
      return error.message || '无法连接到设备。请确认设备地址。'
  }
}

export function LanPairingPanel({
  onPaired,
  initialAddress = '',
  supported = isChromiumPrivateNetworkSupported(),
  connectDevice = getLanPublicInfo,
  getPairingMetadata = getLanPairingMetadata,
  pairDevice = pairLanDevice,
  scanDevices = scanLanSubnet,
}: LanPairingPanelProps) {
  const [address, setAddress] = useState(() => loadLanAddress(initialAddress))
  const [code, setCode] = useState('')
  const [state, setState] = useState<LanConnectionState>('idle')
  const [publicInfo, setPublicInfo] = useState<LanPublicInfo | null>(null)
  const [connectedBaseUrl, setConnectedBaseUrl] = useState<string | null>(null)
  const [connectedAddress, setConnectedAddress] = useState<string | null>(null)
  const [pairingDialogOpen, setPairingDialogOpen] = useState(false)
  const [message, setMessage] = useState('')
  const [pairingMessage, setPairingMessage] = useState('')
  const [scanCidr, setScanCidr] = useState(() => {
    const rememberedAddress = loadLanAddress(initialAddress)
    return loadLanScanCidr(defaultScanCidr(rememberedAddress))
  })
  const [scanProgress, setScanProgress] = useState({ done: 0, total: 0 })
  const [scanResults, setScanResults] = useState<DiscoveredLanDevice[]>([])
  const [selectedScanBaseUrl, setSelectedScanBaseUrl] = useState<string | null>(null)
  const [scanState, setScanState] = useState<'idle' | 'scanning' | 'ready' | 'error'>('idle')
  const [scanError, setScanError] = useState('')
  const scanControllerRef = useRef<AbortController | null>(null)

  useEffect(
    () => () => {
      scanControllerRef.current?.abort()
    },
    []
  )

  const startScan = async () => {
    scanControllerRef.current?.abort()
    const controller = new AbortController()
    scanControllerRef.current = controller
    setScanState('scanning')
    setScanError('')
    setScanResults([])
    setSelectedScanBaseUrl(null)
    setScanProgress({ done: 0, total: 0 })
    try {
      const devices = await scanDevices(scanCidr, {
        signal: controller.signal,
        onProgress: setScanProgress,
      })
      if (controller.signal.aborted) return
      setScanResults(devices)
      setScanState('ready')
    } catch (error) {
      if (controller.signal.aborted) return
      setScanState('error')
      setScanError(lanPairingFailureMessage(error))
    } finally {
      if (scanControllerRef.current === controller) {
        scanControllerRef.current = null
      }
    }
  }

  const cancelScan = () => {
    scanControllerRef.current?.abort()
    scanControllerRef.current = null
    setScanState('idle')
  }

  const claim = async (baseUrl: string, pairingCode?: string) => {
    setState('claiming')
    setPairingMessage('')
    let result: LanPairingResult
    try {
      result = await pairDevice(baseUrl, pairingCode)
    } catch (error) {
      setState('connected')
      setPairingMessage(lanPairingFailureMessage(error))
      return
    }

    // A successful injected pairing path must establish the same persisted
    // session boundary as the production claim client before control leasing.
    storeLanDeviceSession(result.session)
    setState('paired')
    setPairingDialogOpen(false)
    setMessage(
      result.session.hostname
        ? `已配对 ${result.session.hostname}，正在获取控制租约`
        : '已配对设备，正在获取控制租约'
    )
    try {
      await onPaired?.(result.session, result.probe)
    } catch {
      // Pairing and lease acquisition are separate facts. Never surface a
      // lease failure as a WiFi/probe failure after the token was saved.
      setState('error')
      setMessage('设备已配对，但控制租约获取失败。请检查其他客户端后重试。')
    }
  }

  const connect = async (nextAddress = address) => {
    setState('connecting')
    setMessage('')
    setPublicInfo(null)
    setConnectedBaseUrl(null)
    setConnectedAddress(null)
    setPairingDialogOpen(false)
    setPairingMessage('')
    try {
      const baseUrl = normalizeLanBaseUrl(nextAddress)
      storeLanAddress(baseUrl)
      const info = await connectDevice(baseUrl)
      setPublicInfo(info)
      setConnectedBaseUrl(baseUrl)
      setConnectedAddress(new URL(baseUrl).hostname)
      setState('connected')
      if (info.pairing.mode === 'required') {
        setMessage(`已连接 ${info.hostname}。输入四位配对码后才可启用控制。`)
        setPairingDialogOpen(true)
        return
      }
      if (info.pairing.mode === 'optional') {
        setMessage(`已连接 ${info.hostname}。此设备免配对码，正在获取控制租约。`)
        await claim(baseUrl)
        return
      }
      setMessage(`已连接 ${info.hostname}。此设备仅提供基础低频信息读取。`)
    } catch (error) {
      setState('error')
      setMessage(lanPairingFailureMessage(error))
    }
  }

  const connectScanDevice = (device: DiscoveredLanDevice) => {
    setAddress(device.baseUrl)
    storeLanAddress(device.baseUrl)
    setSelectedScanBaseUrl(device.baseUrl)
    void connect(device.baseUrl)
  }

  const refreshPairing = async () => {
    if (!publicInfo) return
    setState('connecting')
    setMessage('')
    try {
      const pairing = await getPairingMetadata(connectedBaseUrl ?? address)
      setPublicInfo((current) => (current ? { ...current, pairing } : current))
      setState('connected')
      if (!pairing.active) {
        setMessage('配对码尚未开启。请保持硬件的 WiFi Info 页面打开，再检查配对码。')
      }
    } catch (error) {
      setState('connected')
      setMessage(lanPairingFailureMessage(error))
    }
  }

  const pairingReady = publicInfo?.pairing.mode === 'required' && publicInfo.pairing.active

  return (
    <section className="industrial-lan-pairing-panel" aria-label="WiFi LAN pairing">
      <div className="industrial-lan-pairing-panel__heading">
        <span className="industrial-lan-pairing-panel__icon">
          <Wifi size={16} aria-hidden="true" />
        </span>
        <div>
          <strong>WiFi / LAN</strong>
          <small>输入设备地址以连接设备</small>
        </div>
      </div>
      {!supported ? (
        <output className="industrial-lan-pairing-panel__unsupported">
          <MonitorSmartphone size={16} aria-hidden="true" />
          此浏览器不支持 HTTPS 页面直连 HTTP 私网设备。请使用 Chrome、Chromium 或 Edge。
        </output>
      ) : (
        <>
          <form
            className="industrial-lan-pairing-panel__connect-fields"
            onSubmit={(event) => {
              event.preventDefault()
              if (state === 'connecting' || state === 'claiming') return
              void connect()
            }}
          >
            <label htmlFor="lan-pairing-address">
              <span>设备地址</span>
              <Input
                id="lan-pairing-address"
                value={address}
                onChange={(event) => {
                  const next = event.target.value
                  setAddress(next)
                  storeLanAddress(next)
                }}
                inputMode="url"
                placeholder="http://192.168.1.18"
                disabled={state === 'connecting' || state === 'claiming'}
              />
            </label>
            <Button type="submit" disabled={state === 'connecting' || state === 'claiming'}>
              {state === 'connecting' ? (
                <LoaderCircle className="animate-spin" aria-hidden="true" />
              ) : null}
              连接设备
            </Button>
          </form>

          <section className="industrial-lan-pairing-panel__scan" aria-label="IP 扫描">
            <form
              className="industrial-lan-pairing-panel__scan-fields"
              onSubmit={(event) => {
                event.preventDefault()
                if (scanState === 'scanning') return
                void startScan()
              }}
            >
              <label htmlFor="lan-scan-cidr">
                <span>CIDR 网段</span>
                <Input
                  id="lan-scan-cidr"
                  value={scanCidr}
                  onChange={(event) => {
                    const next = event.target.value
                    setScanCidr(next)
                    storeLanScanCidr(next)
                  }}
                  autoComplete="off"
                  inputMode="text"
                  placeholder="192.168.1.0/24"
                  disabled={scanState === 'scanning'}
                />
              </label>
              {scanState === 'scanning' ? (
                <Button type="button" variant="outline" onClick={cancelScan}>
                  <X aria-hidden="true" />
                  取消
                </Button>
              ) : (
                <Button type="submit">
                  <ScanSearch aria-hidden="true" />
                  开始扫描
                </Button>
              )}
            </form>

            {scanState === 'scanning' ? (
              <output className="industrial-lan-pairing-panel__scan-status" aria-live="polite">
                <LoaderCircle className="animate-spin" aria-hidden="true" />
                已扫描 {scanProgress.done} / {scanProgress.total || '...'}
              </output>
            ) : null}
            {scanState === 'ready' && scanResults.length === 0 ? (
              <output className="industrial-lan-pairing-panel__scan-status" aria-live="polite">
                未发现设备
              </output>
            ) : null}
            {scanError ? (
              <output
                className="industrial-lan-pairing-panel__scan-status industrial-lan-pairing-panel__scan-status--error"
                aria-live="polite"
              >
                {scanError}
              </output>
            ) : null}
            {scanResults.length > 0 ? (
              <ul className="industrial-lan-pairing-panel__scan-results" aria-label="发现的设备">
                {scanResults.map((device) => (
                  <li
                    key={`${device.info.deviceId}:${device.baseUrl}`}
                    className={selectedScanBaseUrl === device.baseUrl ? 'is-selected' : undefined}
                  >
                    <span>
                      <strong>{device.info.hostname}</strong>
                      <small>{device.baseUrl}</small>
                    </span>
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => connectScanDevice(device)}
                      disabled={state === 'connecting' || state === 'claiming'}
                      aria-pressed={selectedScanBaseUrl === device.baseUrl}
                      aria-label={`连接 ${device.info.hostname}`}
                    >
                      连接
                    </Button>
                  </li>
                ))}
              </ul>
            ) : null}
          </section>

          {publicInfo ? (
            <dl className="industrial-lan-pairing-panel__public-info" aria-label="基础设备信息">
              <div>
                <dt>设备</dt>
                <dd>{publicInfo.hostname}</dd>
              </div>
              <div>
                <dt>固件</dt>
                <dd>{publicInfo.firmwareVersion}</dd>
              </div>
              <div>
                <dt>API</dt>
                <dd>{publicInfo.api}</dd>
              </div>
            </dl>
          ) : null}

          {publicInfo?.pairing.mode === 'required' && !pairingDialogOpen ? (
            <Button
              className="industrial-lan-pairing-panel__pairing-action"
              type="button"
              onClick={() => setPairingDialogOpen(true)}
              disabled={state === 'claiming'}
            >
              输入配对码
            </Button>
          ) : null}
        </>
      )}

      {message ? (
        <output
          className={`industrial-lan-pairing-panel__message industrial-lan-pairing-panel__message--${state}`}
          aria-live="polite"
        >
          {message}
        </output>
      ) : null}

      {supported && publicInfo?.pairing.mode === 'required' && pairingDialogOpen ? (
        <div className="industrial-lan-pairing-dialog-backdrop">
          <section
            className="industrial-lan-pairing-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="输入 LAN 配对码"
          >
            <div className="industrial-lan-pairing-dialog__heading">
              <div>
                <strong>输入配对码</strong>
                <small>{publicInfo.hostname}</small>
              </div>
              <Button
                className="industrial-lan-pairing-dialog__close"
                type="button"
                variant="ghost"
                size="icon"
                onClick={() => setPairingDialogOpen(false)}
                aria-label="关闭配对码对话框"
                title="关闭"
              >
                <X aria-hidden="true" />
              </Button>
            </div>
            <dl className="industrial-lan-pairing-dialog__identity" aria-label="已连接设备详情">
              <div>
                <dt>IP 地址</dt>
                <dd>{connectedAddress ?? '未知'}</dd>
              </div>
              <div>
                <dt>设备 ID</dt>
                <dd>{publicInfo.deviceId}</dd>
              </div>
            </dl>
            {pairingReady ? (
              <div className="industrial-lan-pairing-dialog__claim">
                <form
                  className="industrial-lan-pairing-dialog__fields"
                  onSubmit={(event) => {
                    event.preventDefault()
                    if (state === 'claiming' || code.length !== 4) return
                    void claim(connectedBaseUrl ?? address, code)
                  }}
                >
                  <label htmlFor="lan-pairing-code">
                    <span>四位配对码</span>
                    <Input
                      id="lan-pairing-code"
                      value={code}
                      onChange={(event) => {
                        setCode(event.target.value.replace(/\D/g, '').slice(0, 4))
                        setPairingMessage('')
                      }}
                      inputMode="numeric"
                      autoComplete="off"
                      maxLength={4}
                      placeholder="0000"
                      disabled={state === 'claiming'}
                    />
                  </label>
                  <Button type="submit" disabled={state === 'claiming' || code.length !== 4}>
                    {state === 'claiming' ? (
                      <LoaderCircle className="animate-spin" aria-hidden="true" />
                    ) : null}
                    配对设备
                  </Button>
                </form>
                {pairingMessage ? (
                  <output className="industrial-lan-pairing-dialog__error" aria-live="polite">
                    {pairingMessage}
                  </output>
                ) : null}
              </div>
            ) : (
              <div className="industrial-lan-pairing-dialog__inactive">
                <p>请在设备上打开 WiFi Info 页面后重新检查配对码。</p>
                <Button
                  type="button"
                  onClick={() => void refreshPairing()}
                  disabled={state === 'connecting'}
                >
                  {state === 'connecting' ? (
                    <LoaderCircle className="animate-spin" aria-hidden="true" />
                  ) : null}
                  检查配对码
                </Button>
              </div>
            )}
          </section>
        </div>
      ) : null}
    </section>
  )
}

function defaultScanCidr(address: string) {
  try {
    const hostname = new URL(normalizeLanBaseUrl(address)).hostname
    const octets = hostname.split('.')
    if (octets.length === 4 && octets.every((value) => /^\d+$/.test(value))) {
      return `${octets[0]}.${octets[1]}.${octets[2]}.0/24`
    }
  } catch {
    // Fall back to the common private subnet example for manual editing.
  }
  return ''
}

async function pairLanDevice(address: string, code?: string): Promise<LanPairingResult> {
  const session = await claimLanPairing(address, code)
  try {
    // The pairing claim owns the firmware's heavy request workspace. Run the
    // first authenticated probe serially so its TCP connection has fully
    // returned before ordinary concurrent snapshot reads resume.
    const probe = await probeLanDevice(session, undefined, 'serial')
    return { session, probe }
  } catch (error) {
    if (
      error instanceof ControlPlaneClientError &&
      error.code === 'lan_private_network_unavailable'
    ) {
      throw new ControlPlaneClientError(
        'Paired device probe was interrupted.',
        'lan_probe_unavailable',
        true
      )
    }
    throw error
  }
}
