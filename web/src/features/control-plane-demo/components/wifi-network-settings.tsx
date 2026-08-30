import { CircleAlert, CircleCheck, Info, LoaderCircle, Trash2, Wifi, X } from 'lucide-react'
import { type FormEvent, useEffect, useId, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { NetworkSummary } from '../contracts'
import type { DeviceTarget } from '../types'

const MAX_WIFI_SSID_BYTES = 32
const MAX_WIFI_PASSWORD_BYTES = 64
const DEFAULT_WIFI_FEEDBACK_DISMISS_MS = 5_000

const networkStateLabels: Record<NonNullable<DeviceTarget['networkState']>, string> = {
  disabled: '未配置',
  idle: '待连接',
  saving: '保存中',
  connecting: '连接中',
  connected: '已连接',
  error: '连接失败',
  timeout: '连接超时',
}

export interface WifiNetworkSettingsDraft {
  ssid: string
  /** Undefined keeps the password already stored by the device. */
  password?: string
}

export type WifiPasswordInputMode = 'saved-mask' | 'draft'

export function createWifiPasswordMask(length: number) {
  return '•'.repeat(Math.max(0, Math.min(MAX_WIFI_PASSWORD_BYTES, Math.floor(length))))
}

export function isWifiNetworkSettingsDirty({
  ssid,
  savedSsid,
  password,
  passwordMode,
  savedPasswordLength,
}: {
  ssid: string
  savedSsid: string
  password: string
  passwordMode: WifiPasswordInputMode
  savedPasswordLength: number
}) {
  if (ssid !== savedSsid) {
    return true
  }
  if (savedPasswordLength > 0) {
    return passwordMode !== 'saved-mask'
  }
  return password.length > 0
}

type WifiSnapshot = Pick<
  NetworkSummary,
  'state' | 'configurationGeneration' | 'transitionSequence' | 'failureCode'
>

export function isWifiSnapshotOlder(
  snapshot: Pick<WifiSnapshot, 'configurationGeneration' | 'transitionSequence'>,
  receipt: Pick<WifiSnapshot, 'configurationGeneration' | 'transitionSequence'>
) {
  const receiptGeneration = receipt.configurationGeneration ?? 0
  const receiptSequence = receipt.transitionSequence ?? 0
  const snapshotGeneration = snapshot.configurationGeneration ?? 0
  const snapshotSequence = snapshot.transitionSequence ?? 0
  return (
    snapshotGeneration < receiptGeneration ||
    (snapshotGeneration === receiptGeneration && snapshotSequence < receiptSequence)
  )
}

interface WifiNetworkSettingsProps {
  deviceId: string
  networkState?: DeviceTarget['networkState']
  savedSsid?: string | null
  wifiRssi?: number | null
  savedPasswordLength?: number
  readOnly?: boolean
  disabled?: boolean
  unavailableReason?: string
  feedbackDismissMs?: number
  configurationGeneration?: number
  transitionSequence?: number
  failureCode?: NetworkSummary['failureCode']
  onSave: (draft: WifiNetworkSettingsDraft) => Promise<NetworkSummary>
  onClear: () => Promise<NetworkSummary>
}

export function validateWifiNetworkSettingsDraft(draft: WifiNetworkSettingsDraft) {
  if (!draft.ssid.trim()) {
    return '请输入 WiFi 名称。'
  }
  if (new TextEncoder().encode(draft.ssid).length > MAX_WIFI_SSID_BYTES) {
    return 'WiFi 名称最多 32 个字节。'
  }
  if (new TextEncoder().encode(draft.password ?? '').length > MAX_WIFI_PASSWORD_BYTES) {
    return 'WiFi 密码最多 64 个字节。'
  }
  return null
}

export function resolveWifiSettingsUnavailableReason({
  supportsWifiStateV2,
  transportIssue,
}: {
  supportsWifiStateV2: boolean
  transportIssue?: string
}) {
  if (transportIssue) {
    return transportIssue
  }
  return supportsWifiStateV2 ? undefined : '当前设备固件需要 WiFi 状态协议更新后才能提交设置。'
}

export function wifiConnectionOutcome(state: NonNullable<DeviceTarget['networkState']>) {
  if (state === 'connected') {
    return 'WiFi 已连接。'
  }
  if (state === 'error') {
    return 'WiFi 连接失败，请检查名称和密码。'
  }
  if (state === 'timeout') {
    return 'WiFi 连接超时，请检查网络是否可用。'
  }
  return null
}

export function shouldClearStaleWifiOutcome(
  state: NonNullable<DeviceTarget['networkState']>,
  message: string
) {
  const terminalMessages = [
    wifiConnectionOutcome('connected'),
    wifiConnectionOutcome('error'),
    wifiConnectionOutcome('timeout'),
  ]
  if (state === 'saving' || state === 'connecting') {
    return terminalMessages.includes(message)
  }
  if (state === 'connected') {
    return (
      message === wifiConnectionOutcome('error') || message === wifiConnectionOutcome('timeout')
    )
  }
  return false
}

function wifiFeedbackTone(message: string) {
  if (message === 'WiFi 已连接。' || message === '已清除设备中的 WiFi 设置。') {
    return 'success'
  }
  if (message.includes('失败') || message.includes('超时') || message.startsWith('请输入')) {
    return 'error'
  }
  if (message.startsWith('再次点击')) {
    return 'warning'
  }
  return 'info'
}

export function WifiNetworkSettings({
  deviceId,
  networkState = 'disabled',
  savedSsid = null,
  wifiRssi = null,
  savedPasswordLength = 0,
  readOnly = false,
  disabled = false,
  unavailableReason,
  feedbackDismissMs = DEFAULT_WIFI_FEEDBACK_DISMISS_MS,
  configurationGeneration = 0,
  transitionSequence = 0,
  onSave,
  onClear,
}: WifiNetworkSettingsProps) {
  const inputId = useId()
  const ssidId = `wifi-ssid-${inputId}`
  const passwordId = `wifi-password-${inputId}`
  const previousDeviceId = useRef(deviceId)
  const savedSsidValue = savedSsid ?? ''
  const syncedSsid = useRef(savedSsidValue)
  const hasSsidDraft = useRef(false)
  const [ssid, setSsid] = useState(savedSsidValue)
  const [password, setPassword] = useState(() => createWifiPasswordMask(savedPasswordLength))
  const [passwordMode, setPasswordMode] = useState<WifiPasswordInputMode>(
    savedPasswordLength > 0 ? 'saved-mask' : 'draft'
  )
  const [action, setAction] = useState<'idle' | 'saving' | 'clearing'>('idle')
  const [message, setMessage] = useState('')
  const [pendingSnapshot, setPendingSnapshot] = useState<WifiSnapshot | null>(null)
  const [clearConfirmationPending, setClearConfirmationPending] = useState(false)

  const isBusy = action !== 'idle'
  const isDisabled = disabled || readOnly || isBusy
  const isDirty = isWifiNetworkSettingsDirty({
    ssid,
    savedSsid: savedSsidValue,
    password,
    passwordMode,
    savedPasswordLength,
  })
  const stateLabel = networkStateLabels[networkState]
  const feedbackTone = pendingSnapshot ? 'loading' : wifiFeedbackTone(message)

  useEffect(() => {
    if (previousDeviceId.current === deviceId) {
      return
    }
    previousDeviceId.current = deviceId
    syncedSsid.current = savedSsidValue
    hasSsidDraft.current = false
    setSsid(savedSsidValue)
    setPassword(createWifiPasswordMask(savedPasswordLength))
    setPasswordMode(savedPasswordLength > 0 ? 'saved-mask' : 'draft')
    setAction('idle')
    setMessage('')
    setPendingSnapshot(null)
    setClearConfirmationPending(false)
  }, [deviceId, savedPasswordLength, savedSsidValue])

  useEffect(() => {
    if (previousDeviceId.current !== deviceId || pendingSnapshot || passwordMode === 'draft') {
      return
    }
    const nextMask = createWifiPasswordMask(savedPasswordLength)
    if (password !== nextMask) {
      setPassword(nextMask)
      if (!nextMask) {
        setPasswordMode('draft')
      }
    }
  }, [deviceId, password, passwordMode, pendingSnapshot, savedPasswordLength])

  useEffect(() => {
    if (
      previousDeviceId.current !== deviceId ||
      pendingSnapshot ||
      hasSsidDraft.current ||
      savedSsidValue === syncedSsid.current
    ) {
      return
    }
    syncedSsid.current = savedSsidValue
    setSsid(savedSsidValue)
  }, [deviceId, pendingSnapshot, savedSsidValue])

  useEffect(() => {
    if (!pendingSnapshot) {
      return
    }
    if (isWifiSnapshotOlder({ configurationGeneration, transitionSequence }, pendingSnapshot)) {
      return
    }
    if (networkState === 'saving' || networkState === 'connecting') {
      setMessage('已提交，正在等待设备连接。')
      return
    }
    if (networkState === 'disabled') {
      setMessage('已清除设备中的 WiFi 设置。')
      setAction('idle')
      syncedSsid.current = ''
      hasSsidDraft.current = false
      setSsid('')
      setPassword('')
      setPasswordMode('draft')
      setPendingSnapshot(null)
      return
    }
    const outcome = wifiConnectionOutcome(networkState)
    if (outcome) {
      setMessage(outcome)
      setAction('idle')
      if (networkState === 'connected') {
        if (savedSsid != null) {
          syncedSsid.current = savedSsid
          hasSsidDraft.current = false
          setSsid(savedSsid)
        }
        const savedPasswordMask = createWifiPasswordMask(savedPasswordLength)
        setPassword(savedPasswordMask)
        setPasswordMode(savedPasswordMask ? 'saved-mask' : 'draft')
      }
      setPendingSnapshot(null)
      return
    }
  }, [
    configurationGeneration,
    networkState,
    pendingSnapshot,
    savedPasswordLength,
    savedSsid,
    transitionSequence,
  ])

  useEffect(() => {
    // The device snapshot effect owns the loading/terminal replacement while
    // a receipt is pending. Do not let stale-outcome cleanup erase that
    // loading Toast in the same render as a new configuration generation.
    if (!pendingSnapshot && shouldClearStaleWifiOutcome(networkState, message)) {
      setMessage('')
    }
  }, [message, networkState, pendingSnapshot])

  useEffect(() => {
    if (!message || pendingSnapshot || clearConfirmationPending) {
      return
    }
    const dismissTimer = window.setTimeout(() => setMessage(''), feedbackDismissMs)
    return () => window.clearTimeout(dismissTimer)
  }, [clearConfirmationPending, feedbackDismissMs, message, pendingSnapshot])

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (isDisabled || !isDirty) {
      return
    }

    const draft: WifiNetworkSettingsDraft = {
      ssid,
      ...(passwordMode === 'draft' ? { password } : {}),
    }
    const validationError = validateWifiNetworkSettingsDraft(draft)
    if (validationError) {
      setMessage(validationError)
      return
    }

    setAction('saving')
    setMessage('')
    try {
      const receipt = await onSave(draft)
      setPendingSnapshot({
        state: receipt.state,
        configurationGeneration: receipt.configurationGeneration ?? 0,
        transitionSequence: receipt.transitionSequence ?? 0,
        failureCode: receipt.failureCode,
      })
      setClearConfirmationPending(false)
      if (receipt.state !== 'saving' && receipt.state !== 'connecting') {
        setAction('idle')
      }
    } catch (error) {
      setPendingSnapshot(null)
      setMessage(error instanceof Error ? error.message : 'WiFi 设置未能提交。')
      setAction('idle')
    }
  }

  const clear = async () => {
    if (isDisabled) {
      return
    }
    if (!clearConfirmationPending) {
      setClearConfirmationPending(true)
      setMessage('再次点击以清除设备中已保存的 WiFi。')
      return
    }

    setAction('clearing')
    setMessage('')
    try {
      const receipt = await onClear()
      setPendingSnapshot({
        state: receipt.state,
        configurationGeneration: receipt.configurationGeneration ?? 0,
        transitionSequence: receipt.transitionSequence ?? 0,
        failureCode: receipt.failureCode,
      })
      setClearConfirmationPending(false)
      setMessage('已清除设备中的 WiFi 设置。')
      if (receipt.state !== 'saving' && receipt.state !== 'connecting') {
        setAction('idle')
      }
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'WiFi 设置未能清除。')
      setAction('idle')
    }
  }

  const dismissFeedback = () => {
    setMessage('')
    if (clearConfirmationPending && action === 'idle') {
      setClearConfirmationPending(false)
    }
  }

  return (
    <section className="industrial-wifi-settings" aria-label="WiFi 设置" data-device-id={deviceId}>
      <div className="industrial-wifi-settings__header">
        <span className="industrial-wifi-settings__icon">
          <Wifi size={16} aria-hidden="true" />
        </span>
        <div>
          <h3>WiFi</h3>
          <output aria-live="polite">
            <strong>{stateLabel}</strong>
            {networkState === 'connected' && wifiRssi != null ? (
              <small>{wifiRssi} dBm</small>
            ) : null}
          </output>
        </div>
      </div>

      {readOnly ? (
        <>
          <div className="industrial-wifi-settings__support-message" role="alert">
            <CircleAlert aria-hidden="true" />
            <span>{unavailableReason ?? '当前连接仅支持查看 WiFi 信息，不能修改设备配置。'}</span>
          </div>
          <div className="industrial-wifi-settings__form industrial-wifi-settings__form--readonly">
            <Label className="industrial-wifi-settings__field" htmlFor={ssidId}>
              <span>WiFi 名称</span>
              <Input
                id={ssidId}
                className="h-9"
                value={savedSsidValue || '未配置'}
                readOnly
                aria-readonly="true"
              />
            </Label>

            <Label className="industrial-wifi-settings__field" htmlFor={passwordId}>
              <span>已保存密码</span>
              <Input
                id={passwordId}
                className="h-9"
                type={savedPasswordLength > 0 ? 'password' : 'text'}
                aria-label="密码"
                value={
                  savedPasswordLength > 0 ? createWifiPasswordMask(savedPasswordLength) : '未配置'
                }
                readOnly
                aria-readonly="true"
              />
            </Label>

            <div className="industrial-wifi-settings__field industrial-wifi-settings__field--status">
              <span>信号</span>
              <output
                id={`${inputId}-rssi`}
                className="industrial-wifi-settings__readonly-output"
                aria-label="信号"
              >
                {wifiRssi == null ? '不可用' : `${wifiRssi} dBm`}
              </output>
            </div>
          </div>
        </>
      ) : (
        <form
          className="industrial-wifi-settings__form"
          autoComplete="off"
          onSubmit={(event) => void submit(event)}
        >
          <Label className="industrial-wifi-settings__field" htmlFor={ssidId}>
            <span>WiFi 名称</span>
            <Input
              id={ssidId}
              className="h-9"
              value={ssid}
              onChange={(event) => {
                hasSsidDraft.current = true
                setSsid(event.target.value)
              }}
              autoComplete="off"
              disabled={isDisabled}
              maxLength={MAX_WIFI_SSID_BYTES}
              placeholder="输入 SSID"
            />
          </Label>

          <Label className="industrial-wifi-settings__field" htmlFor={passwordId}>
            <span>密码</span>
            <Input
              id={passwordId}
              className="h-9"
              type="password"
              aria-label="密码"
              value={password}
              onClick={(event) => event.currentTarget.select()}
              onFocus={(event) => event.currentTarget.select()}
              onKeyDown={(event) => {
                if (
                  passwordMode === 'saved-mask' &&
                  (event.key === 'Backspace' || event.key === 'Delete')
                ) {
                  event.preventDefault()
                  setPassword('')
                  setPasswordMode('draft')
                }
              }}
              onChange={(event) => {
                setPassword(event.target.value)
                setPasswordMode('draft')
              }}
              autoComplete="new-password"
              disabled={isDisabled}
              maxLength={MAX_WIFI_PASSWORD_BYTES}
            />
          </Label>

          <div className="industrial-wifi-settings__actions">
            <Button
              className="industrial-wifi-settings__submit h-9"
              type="submit"
              disabled={isDisabled || !isDirty}
              aria-busy={action === 'saving'}
            >
              {action === 'saving' ? (
                <LoaderCircle className="animate-spin" aria-hidden="true" />
              ) : null}
              {action === 'saving' ? '保存中' : '保存并连接'}
            </Button>
            <Button
              className="h-9"
              type="button"
              variant="outline"
              onClick={() => void clear()}
              disabled={isDisabled}
              aria-busy={action === 'clearing'}
            >
              {action === 'clearing' ? (
                <LoaderCircle className="animate-spin" aria-hidden="true" />
              ) : (
                <Trash2 aria-hidden="true" />
              )}
              {clearConfirmationPending ? '确认清除' : '清除 WiFi'}
            </Button>
          </div>
        </form>
      )}

      {message ? (
        <div
          className="industrial-wifi-settings__toast"
          data-tone={feedbackTone}
          role={feedbackTone === 'error' ? 'alert' : 'status'}
          aria-live={feedbackTone === 'error' ? 'assertive' : 'polite'}
          aria-busy={feedbackTone === 'loading'}
        >
          <span className="industrial-wifi-settings__toast-icon" aria-hidden="true">
            {feedbackTone === 'loading' ? <LoaderCircle className="animate-spin" /> : null}
            {feedbackTone === 'success' ? <CircleCheck /> : null}
            {feedbackTone === 'error' || feedbackTone === 'warning' ? <CircleAlert /> : null}
            {feedbackTone === 'info' ? <Info /> : null}
          </span>
          <span>{message}</span>
          <button type="button" aria-label="关闭通知" onClick={dismissFeedback}>
            <X aria-hidden="true" />
          </button>
        </div>
      ) : null}
    </section>
  )
}
