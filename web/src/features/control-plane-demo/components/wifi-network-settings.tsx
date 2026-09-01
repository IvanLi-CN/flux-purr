import { CircleAlert, CircleCheck, Clock3, Info, LoaderCircle, Trash2, Wifi, X } from 'lucide-react'
import { type FormEvent, useCallback, useEffect, useId, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { NetworkSummary } from '../contracts'
import type { DeviceTarget, EventLogEntry } from '../types'

const MAX_WIFI_SSID_BYTES = 32
const MAX_WIFI_PASSWORD_BYTES = 64
const DEFAULT_WIFI_FEEDBACK_DISMISS_MS = 5_000
const WIFI_CANCEL_AVAILABLE_AFTER_MS = 10_000
const WIFI_OPERATION_TIMEOUT_MS = 35_000

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
  transportRecoveryState?: 'recovering' | 'unavailable'
  operationInterruption?: number
  feedbackDismissMs?: number
  cancelAvailableAfterMs?: number
  operationTimeoutMs?: number
  configurationGeneration?: number
  transitionSequence?: number
  failureCode?: NetworkSummary['failureCode']
  onSave: (draft: WifiNetworkSettingsDraft) => Promise<NetworkSummary>
  onClear: () => Promise<NetworkSummary>
  onCancel: () => Promise<NetworkSummary>
  onOperationEvent?: (message: string, tone: EventLogEntry['tone']) => void
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

export function wifiFailureReason(failureCode?: NetworkSummary['failureCode']) {
  switch (failureCode) {
    case 'disconnect_timed_out':
      return '连接完成前设备网络断开。'
    case 'configuration_failed':
      return '设备未能应用 WiFi 配置。'
    case 'association_rejected':
      return '接入点拒绝认证，请检查 WiFi 名称和密码。'
    case 'association_timed_out':
      return '等待接入点响应超时。'
    case 'ipv4_timed_out':
      return '已连接接入点，但未能获得 IPv4 地址。'
    case 'station_disconnected':
      return '设备与接入点断开。'
    case 'lan_startup_failed':
      return '设备网络接口启动失败。'
    default:
      return '设备未返回具体失败原因。'
  }
}

export function formatWifiWaitElapsed(seconds: number) {
  const totalSeconds = Math.max(0, Math.floor(seconds))
  const minutes = Math.floor(totalSeconds / 60)
  const remainderSeconds = totalSeconds % 60
  return minutes > 0
    ? `${minutes} 分 ${String(remainderSeconds).padStart(2, '0')} 秒`
    : `${remainderSeconds} 秒`
}

export function wifiConnectionOutcome(
  state: NonNullable<DeviceTarget['networkState']>,
  failureCode?: NetworkSummary['failureCode']
) {
  if (state === 'connected') {
    return 'WiFi 已连接。'
  }
  if (state === 'error') {
    return `WiFi 连接失败：${wifiFailureReason(failureCode)}`
  }
  if (state === 'timeout') {
    return failureCode
      ? `WiFi 连接超时：${wifiFailureReason(failureCode)}`
      : 'WiFi 连接超时，请检查网络是否可用。'
  }
  return null
}

function isWifiTerminalOutcomeMessage(message: string) {
  return (
    message === 'WiFi 已连接。' ||
    message.startsWith('WiFi 连接失败：') ||
    message.startsWith('WiFi 连接超时')
  )
}

export function shouldClearStaleWifiOutcome(
  state: NonNullable<DeviceTarget['networkState']>,
  message: string
) {
  if (state === 'saving' || state === 'connecting') {
    return isWifiTerminalOutcomeMessage(message)
  }
  if (state === 'connected') {
    return message.startsWith('WiFi 连接失败：') || message.startsWith('WiFi 连接超时')
  }
  return false
}

function wifiFeedbackTone(message: string) {
  if (
    message === 'WiFi 已连接。' ||
    message === '已清除设备中的 WiFi 设置。' ||
    message === '已取消设备 WiFi 连接。'
  ) {
    return 'success'
  }
  if (message.includes('失败') || message.includes('超时') || message.startsWith('请输入')) {
    return 'error'
  }
  if (message.startsWith('再次点击') || message.includes('中断')) {
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
  transportRecoveryState,
  operationInterruption = 0,
  feedbackDismissMs = DEFAULT_WIFI_FEEDBACK_DISMISS_MS,
  cancelAvailableAfterMs = WIFI_CANCEL_AVAILABLE_AFTER_MS,
  operationTimeoutMs = WIFI_OPERATION_TIMEOUT_MS,
  configurationGeneration = 0,
  transitionSequence = 0,
  failureCode,
  onSave,
  onClear,
  onCancel,
  onOperationEvent,
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
  const [action, setAction] = useState<'idle' | 'saving' | 'clearing' | 'cancelling'>('idle')
  const [message, setMessage] = useState('')
  const [pendingSnapshot, setPendingSnapshot] = useState<WifiSnapshot | null>(null)
  const [clearConfirmationPending, setClearConfirmationPending] = useState(false)
  const [cancelAvailable, setCancelAvailable] = useState(false)
  const [hostWaitTimedOut, setHostWaitTimedOut] = useState(false)
  const [waitedSeconds, setWaitedSeconds] = useState(0)
  const previousOperationInterruption = useRef(operationInterruption)
  const operationIdRef = useRef(0)
  const operationTimerRef = useRef<number | null>(null)
  const operationTimeoutRef = useRef<number | null>(null)
  const operationElapsedTimerRef = useRef<number | null>(null)
  const operationStartedAtRef = useRef<number | null>(null)

  const isBusy = action !== 'idle'
  const cancellationInFlight = action === 'cancelling'
  const waitingForDevice = isBusy && !cancellationInFlight
  const isDisabled = disabled || readOnly || isBusy
  const isDirty = isWifiNetworkSettingsDirty({
    ssid,
    savedSsid: savedSsidValue,
    password,
    passwordMode,
    savedPasswordLength,
  })
  const deviceStateLabel = networkStateLabels[networkState]
  const stateLabel =
    transportRecoveryState === 'recovering'
      ? '配置通道恢复中'
      : transportRecoveryState === 'unavailable'
        ? '配置通道不可用'
        : hostWaitTimedOut
          ? '等待设备确认'
          : cancellationInFlight
            ? '正在取消连接'
            : waitingForDevice
              ? '连接中'
              : deviceStateLabel
  const displayedMessage = readOnly && unavailableReason === message ? '' : message
  const feedbackTone =
    pendingSnapshot || cancellationInFlight || waitingForDevice
      ? 'loading'
      : wifiFeedbackTone(displayedMessage)

  const clearOperationTimer = useCallback(() => {
    if (operationTimerRef.current !== null) {
      window.clearTimeout(operationTimerRef.current)
      operationTimerRef.current = null
    }
  }, [])

  const clearWaitElapsedTimer = useCallback(() => {
    if (operationElapsedTimerRef.current !== null) {
      window.clearInterval(operationElapsedTimerRef.current)
      operationElapsedTimerRef.current = null
    }
    operationStartedAtRef.current = null
    setWaitedSeconds(0)
  }, [])

  const startWaitElapsedTimer = useCallback(() => {
    clearWaitElapsedTimer()
    operationStartedAtRef.current = Date.now()
    setWaitedSeconds(0)
    operationElapsedTimerRef.current = window.setInterval(() => {
      const startedAt = operationStartedAtRef.current
      if (startedAt === null) {
        return
      }
      setWaitedSeconds(Math.max(0, Math.floor((Date.now() - startedAt) / 1_000)))
    }, 250)
  }, [clearWaitElapsedTimer])

  const finishOperation = useCallback(() => {
    clearOperationTimer()
    clearWaitElapsedTimer()
    if (operationTimeoutRef.current !== null) {
      window.clearTimeout(operationTimeoutRef.current)
      operationTimeoutRef.current = null
    }
    setCancelAvailable(false)
  }, [clearOperationTimer, clearWaitElapsedTimer])

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
    setHostWaitTimedOut(false)
    finishOperation()
  }, [deviceId, finishOperation, savedPasswordLength, savedSsidValue])

  useEffect(
    () => () => {
      clearOperationTimer()
      clearWaitElapsedTimer()
      if (operationTimeoutRef.current !== null) {
        window.clearTimeout(operationTimeoutRef.current)
      }
    },
    [clearOperationTimer, clearWaitElapsedTimer]
  )

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
      finishOperation()
      syncedSsid.current = ''
      hasSsidDraft.current = false
      setSsid('')
      setPassword('')
      setPasswordMode('draft')
      setPendingSnapshot(null)
      return
    }
    const outcome = wifiConnectionOutcome(networkState, pendingSnapshot.failureCode ?? failureCode)
    if (outcome) {
      setMessage(outcome)
      setAction('idle')
      finishOperation()
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
    failureCode,
    finishOperation,
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
    if (!hostWaitTimedOut || !isBusy) return
    const outcome = wifiConnectionOutcome(networkState, failureCode)
    if (!outcome && networkState !== 'disabled') return
    setHostWaitTimedOut(false)
    setAction('idle')
    finishOperation()
    setMessage(outcome ?? '已清除设备中的 WiFi 设置。')
  }, [failureCode, finishOperation, hostWaitTimedOut, isBusy, networkState])

  useEffect(() => {
    if (previousOperationInterruption.current === operationInterruption) {
      return
    }
    previousOperationInterruption.current = operationInterruption
    if (!isBusy && !pendingSnapshot) {
      return
    }
    operationIdRef.current += 1
    finishOperation()
    setPendingSnapshot(null)
    setAction('idle')
    setClearConfirmationPending(false)
    setHostWaitTimedOut(false)
    setMessage('WiFi 配置传输已中断，设备尚未确认设置；连接恢复后可重新提交。')
    onOperationEvent?.('WiFi confirmation interrupted before a terminal device state', 'warning')
  }, [finishOperation, isBusy, onOperationEvent, operationInterruption, pendingSnapshot])

  useEffect(() => {
    if (!message || pendingSnapshot || clearConfirmationPending || hostWaitTimedOut) {
      return
    }
    const dismissTimer = window.setTimeout(() => setMessage(''), feedbackDismissMs)
    return () => window.clearTimeout(dismissTimer)
  }, [clearConfirmationPending, feedbackDismissMs, hostWaitTimedOut, message, pendingSnapshot])

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

    const operationId = ++operationIdRef.current
    finishOperation()
    startWaitElapsedTimer()
    setHostWaitTimedOut(false)
    operationTimerRef.current = window.setTimeout(
      () => setCancelAvailable(true),
      cancelAvailableAfterMs
    )
    operationTimeoutRef.current = window.setTimeout(() => {
      if (operationIdRef.current !== operationId) return
      setHostWaitTimedOut(true)
      const elapsed = Math.max(
        0,
        Math.floor((Date.now() - (operationStartedAtRef.current ?? Date.now())) / 1000)
      )
      setMessage(
        `设备尚未确认 WiFi 连接结果（已等待 ${formatWifiWaitElapsed(elapsed)}）。可取消设备连接或继续等待。`
      )
      onOperationEvent?.(
        'WiFi device confirmation remains pending after the host wait threshold',
        'warning'
      )
    }, operationTimeoutMs)
    setAction('saving')
    setMessage('')
    try {
      const receipt = await onSave(draft)
      if (operationIdRef.current !== operationId) return
      setPendingSnapshot({
        state: receipt.state,
        configurationGeneration: receipt.configurationGeneration ?? 0,
        transitionSequence: receipt.transitionSequence ?? 0,
        failureCode: receipt.failureCode,
      })
      setClearConfirmationPending(false)
      if (receipt.state !== 'saving' && receipt.state !== 'connecting') {
        setAction('idle')
        finishOperation()
      }
    } catch (error) {
      if (operationIdRef.current !== operationId) return
      setPendingSnapshot(null)
      setMessage(error instanceof Error ? error.message : 'WiFi 设置未能提交。')
      setAction('idle')
      finishOperation()
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

    const operationId = ++operationIdRef.current
    finishOperation()
    startWaitElapsedTimer()
    setHostWaitTimedOut(false)
    operationTimerRef.current = window.setTimeout(
      () => setCancelAvailable(true),
      cancelAvailableAfterMs
    )
    operationTimeoutRef.current = window.setTimeout(() => {
      if (operationIdRef.current !== operationId) return
      setHostWaitTimedOut(true)
      const elapsed = Math.max(
        0,
        Math.floor((Date.now() - (operationStartedAtRef.current ?? Date.now())) / 1000)
      )
      setMessage(
        `设备尚未确认 WiFi 清除结果（已等待 ${formatWifiWaitElapsed(elapsed)}）。可取消设备连接或继续等待。`
      )
      onOperationEvent?.(
        'WiFi clear confirmation remains pending after the host wait threshold',
        'warning'
      )
    }, operationTimeoutMs)
    setAction('clearing')
    setMessage('')
    try {
      const receipt = await onClear()
      if (operationIdRef.current !== operationId) return
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
        finishOperation()
      }
    } catch (error) {
      if (operationIdRef.current !== operationId) return
      setMessage(error instanceof Error ? error.message : 'WiFi 设置未能清除。')
      setAction('idle')
      finishOperation()
    }
  }

  const cancel = async () => {
    if (!isBusy || !cancelAvailable || cancellationInFlight) return
    const operationId = ++operationIdRef.current
    finishOperation()
    setPendingSnapshot(null)
    setClearConfirmationPending(false)
    setAction('cancelling')
    setMessage('正在请求设备取消 WiFi 连接。')
    onOperationEvent?.(
      'WiFi cancellation requested through the device configuration channel',
      'info'
    )
    try {
      const receipt = await onCancel()
      if (operationIdRef.current !== operationId) return
      finishOperation()
      setAction('idle')
      if (receipt.state === 'idle') {
        setMessage('已取消设备 WiFi 连接。')
        onOperationEvent?.('WiFi station cancellation confirmed by device', 'success')
        return
      }
      const outcome = wifiConnectionOutcome(receipt.state, receipt.failureCode)
      setMessage(
        outcome ?? `设备未确认 WiFi 已取消，当前状态：${networkStateLabels[receipt.state]}。`
      )
      onOperationEvent?.('WiFi device cancellation was not confirmed', 'danger')
    } catch (error) {
      if (operationIdRef.current !== operationId) return
      finishOperation()
      setAction('idle')
      const detail = error instanceof Error ? error.message.trim() : ''
      setMessage(
        detail ? `取消 WiFi 连接失败：${detail}` : '取消 WiFi 连接失败，设备未确认连接已停止。'
      )
      onOperationEvent?.('WiFi device cancellation request failed', 'danger')
    }
  }

  const continueWaiting = () => {
    if (!isBusy || !cancelAvailable) {
      return
    }
    const elapsed = formatWifiWaitElapsed(waitedSeconds)
    setHostWaitTimedOut(false)
    setMessage(`继续等待设备响应（已等待 ${elapsed}）。`)
    onOperationEvent?.(`WiFi host wait continued after ${elapsed}`, 'info')
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
          <output aria-label="WiFi 网络状态" aria-live="polite">
            {transportRecoveryState === 'recovering' ||
            (transportRecoveryState == null &&
              (cancellationInFlight ||
                waitingForDevice ||
                networkState === 'saving' ||
                networkState === 'connecting')) ? (
              <LoaderCircle className="animate-spin" aria-hidden="true" />
            ) : null}
            {transportRecoveryState === 'unavailable' ||
            networkState === 'error' ||
            networkState === 'timeout' ? (
              <CircleAlert className="is-danger" aria-hidden="true" />
            ) : null}
            {networkState === 'connected' && transportRecoveryState == null ? (
              <CircleCheck className="is-success" aria-hidden="true" />
            ) : null}
            <strong>{stateLabel}</strong>
            {waitingForDevice ? (
              <small>
                已等待 {formatWifiWaitElapsed(waitedSeconds)}
                {hostWaitTimedOut
                  ? '，设备尚未确认，可取消或继续等待。'
                  : cancelAvailable
                    ? '，可取消或继续等待。'
                    : ''}
              </small>
            ) : null}
            {!waitingForDevice &&
            (networkState === 'error' || networkState === 'timeout') &&
            transportRecoveryState == null ? (
              <small>原因：{wifiFailureReason(failureCode)}</small>
            ) : null}
            {!waitingForDevice &&
            networkState === 'connected' &&
            wifiRssi != null &&
            transportRecoveryState == null ? (
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
            {cancellationInFlight ? (
              <Button
                className="industrial-wifi-settings__submit h-9"
                type="button"
                disabled
                aria-busy="true"
              >
                <LoaderCircle className="animate-spin" aria-hidden="true" />
                取消中
              </Button>
            ) : waitingForDevice && cancelAvailable ? (
              <>
                <Button
                  className="industrial-wifi-settings__submit h-9"
                  type="button"
                  onClick={() => void cancel()}
                >
                  <X aria-hidden="true" />
                  取消
                </Button>
                <Button className="h-9" type="button" variant="outline" onClick={continueWaiting}>
                  <Clock3 aria-hidden="true" />
                  继续等待
                </Button>
              </>
            ) : (
              <>
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
              </>
            )}
          </div>
        </form>
      )}

      {displayedMessage ? (
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
          <span>{displayedMessage}</span>
          <button type="button" aria-label="关闭通知" onClick={dismissFeedback}>
            <X aria-hidden="true" />
          </button>
        </div>
      ) : null}
    </section>
  )
}
