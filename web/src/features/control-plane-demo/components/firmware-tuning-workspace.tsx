import { Clipboard, Copy, Download, LoaderCircle, Save, ShieldAlert, Square, Upload } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import type {
  ThermalControlRuntime,
  ThermalTuningCommand,
  ThermalTuningEvent,
  ThermalTuningStatus,
} from '../contracts'
import {
  loadThermalTuningRecording,
  persistThermalTuningRecording,
} from '../thermal-tuning-browser-journal'
import {
  nextTuningReadCommand,
  recordThermalTuningPage,
  type ThermalTuningRecording,
} from '../thermal-tuning-recorder'
import { exportThermalTuningV2 } from '../thermal-tuning-v2-export'
import { ControlPlaneClientError } from '../transport-client'
import './firmware-tuning-workspace.css'

const EXECUTION_ORDER = [60, 220, 120, 80, 100, 160, 140, 180]
const SETTINGS_FIELDS = [
  'tempFilterAlphaPermille',
  'warmupReenterCentiC',
  'holdEntryCentiC',
  'holdExitCentiC',
  'holdOnCentiC',
  'holdOffCentiC',
  'overshootCutoffCentiC',
  'approachMaxTicks',
  'approachMinPowerRatioPermille',
  'holdKpPermillePerC',
  'holdKiPermillePerCTick',
  'holdBlendTicks',
  'holdReheatPowerPermille',
  'approachLeadTicks',
  'holdLeadTicks',
  'autoAdjustableWorkingFloorMv',
  'heaterCurrentReserveMa',
] as const
const POINT_FIELDS = [
  'targetTempC',
  'brakeDistanceCentiC',
  'warmupPowerPermille',
  'warmupReenterCentiC',
  'approachPowerPermille',
  'approachFloorPowerPermille',
  'approachDampingExponentPermille',
  'approachTailWindowCentiC',
  'holdPowerPermille',
  'holdReheatPowerPermille',
  'holdEntryCentiC',
  'holdExitCentiC',
  'holdOnCentiC',
  'holdOffCentiC',
  'overshootCutoffCentiC',
  'holdKpPermillePerC',
  'holdKiPermillePerCTick',
  'holdBlendTicks',
  'approachLeadTicks',
  'holdLeadTicks',
] as const
type DeviceCommand = Omit<ThermalTuningCommand, 'leaseId'>

interface ImportedCandidate {
  hash: string
  profile: Record<string, unknown>
  name: string
}

export function FirmwareTuningWorkspace({
  supported,
  deviceId,
  transport,
  parameterSnapshot,
  readStatus,
  sendCommand,
}: {
  supported: boolean
  deviceId: string
  transport: string
  parameterSnapshot?: {
    mode?: string
    bank?: string
    control?: ThermalControlRuntime
  }
  readStatus: () => Promise<ThermalTuningStatus>
  sendCommand: (command: DeviceCommand) => Promise<ThermalTuningStatus>
}) {
  const [recording, setRecording] = useState<ThermalTuningRecording | null>(null)
  const recordingRef = useRef<ThermalTuningRecording | null>(null)
  const readStatusRef = useRef(readStatus)
  const sendCommandRef = useRef(sendCommand)
  readStatusRef.current = readStatus
  sendCommandRef.current = sendCommand
  const [journalReady, setJournalReady] = useState(false)
  const [identityVerified, setIdentityVerified] = useState(false)
  const [candidate, setCandidate] = useState<ImportedCandidate | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pollAttempt, setPollAttempt] = useState(0)
  const [copiedIdentifier, setCopiedIdentifier] = useState<'candidate' | 'hash' | null>(null)
  const [readState, setReadState] = useState<
    'idle' | 'reading' | 'ready' | 'failed' | 'unsupported'
  >(supported ? 'idle' : 'unsupported')
  const fileInput = useRef<HTMLInputElement>(null)
  const canceledGapRun = useRef<string | null>(null)
  const status = recording?.status
  const armed = status?.armed === true
  // Firmware keeps the terminal disposition at `running` in its idle wire
  // status for compatibility.  A run is only active when it has been armed
  // or carries a run identity; otherwise the workbench must show standby.
  const terminal = visibleTerminalDisposition(status)
  const powerClass = resolveTuningPowerClass(parameterSnapshot?.bank, status?.powerClass)

  useEffect(() => {
    let cancelled = false
    recordingRef.current = null
    setRecording(null)
    setJournalReady(false)
    setIdentityVerified(false)
    setReadState(supported ? 'idle' : 'unsupported')
    void loadThermalTuningRecording(deviceId)
      .then((restored) => {
        if (cancelled) return
        recordingRef.current = restored
        setRecording(restored)
        setJournalReady(true)
      })
      .catch((failure) => {
        if (cancelled) return
        setError(messageOf(failure))
        if (supported) {
          void readStatusRef
            .current()
            .then((current) => {
              if (current.armed)
                return sendCommandRef.current({ op: 'cancel', runId: current.runId ?? undefined })
            })
            .catch(() => {})
        }
      })
    return () => {
      cancelled = true
    }
  }, [deviceId, supported])

  const recordStatus = useCallback(
    async (next: ThermalTuningStatus, confirmedSequence: number | null = null) => {
      const updated = recordThermalTuningPage(recordingRef.current, next, confirmedSequence)
      try {
        if (updated.runId) await persistThermalTuningRecording(deviceId, updated)
      } catch (failure) {
        if (updated.status.armed && updated.runId) {
          try {
            await sendCommand({ op: 'cancel', runId: updated.runId })
          } catch {
            // The original persistence failure still blocks event acknowledgement.
          }
        }
        throw failure
      }
      recordingRef.current = updated
      setRecording(updated)
    },
    [deviceId, sendCommand]
  )

  useEffect(() => {
    if (
      !supported ||
      !journalReady ||
      pending ||
      recording?.gap ||
      (recording?.complete && identityVerified) ||
      (readState === 'failed' && pollAttempt >= 3)
    )
      return
    let cancelled = false
    const timer = window.setTimeout(
      async () => {
        setReadState('reading')
        try {
          const command = nextTuningReadCommand(recording, identityVerified)
          const confirmedSequence = command?.afterSequence ?? null
          const next = command ? await sendCommand(command) : await readStatus()
          if (!cancelled) {
            await recordStatus(next, confirmedSequence)
            setIdentityVerified(true)
            setError(null)
            setPollAttempt(0)
            setReadState('ready')
          }
        } catch (failure) {
          if (!cancelled) {
            setError(messageOf(failure))
            setReadState('failed')
            setPollAttempt((attempt) =>
              failure instanceof ControlPlaneClientError && !failure.retryable
                ? 3
                : Math.min(3, attempt + 1)
            )
          }
        }
      },
      Math.min(2_000, (armed ? 75 : 500) + pollAttempt * 150)
    )
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [
    armed,
    identityVerified,
    journalReady,
    pending,
    pollAttempt,
    readState,
    readStatus,
    recordStatus,
    recording,
    sendCommand,
    supported,
  ])

  useEffect(() => {
    if (!recording?.gap || !recording.status.armed || !recording.runId) return
    if (canceledGapRun.current === recording.runId) return
    canceledGapRun.current = recording.runId
    void sendCommand({ op: 'cancel', runId: recording.runId })
      .then((next) => recordStatus(next))
      .catch((failure) => setError(messageOf(failure)))
  }, [recordStatus, recording, sendCommand])

  const apply = async (command: DeviceCommand) => {
    if (!journalReady || !identityVerified) return
    setPending(true)
    setError(null)
    try {
      const next = await sendCommand(command)
      await recordStatus(next)
    } catch (failure) {
      setError(messageOf(failure))
    } finally {
      setPending(false)
    }
  }

  const retryRead = () => {
    setError(null)
    setReadState('idle')
    setPollAttempt(0)
    setIdentityVerified(false)
  }

  const copyIdentifier = async (kind: 'candidate' | 'hash', value: string | null | undefined) => {
    if (!value || !navigator.clipboard) return
    try {
      await navigator.clipboard.writeText(value)
      setCopiedIdentifier(kind)
      window.setTimeout(() => setCopiedIdentifier((current) => (current === kind ? null : current)), 1600)
    } catch {
      setCopiedIdentifier(null)
    }
  }

  const importCandidate = async (file: File | undefined) => {
    if (!file) return
    try {
      const value: unknown = JSON.parse(await file.text())
      setCandidate(await validateCandidate(value, file.name))
      setError(null)
    } catch (failure) {
      setCandidate(null)
      setError(messageOf(failure))
    }
  }

  const start = () => {
    if (!powerClass) return
    const runId = `web-${powerClass}-${crypto.randomUUID()}`
    void apply({
      op: 'start',
      runId,
      candidateId: candidate?.hash ? runId : undefined,
      candidateHash: candidate?.hash,
      powerClass,
      targetsC: EXECUTION_ORDER,
      candidateProfile: candidate
        ? { op: 'preview', bank: 'pps5a', profile: candidate.profile }
        : undefined,
    })
  }

  const exportReport = async () => {
    if (!recording?.runId) return
    setPending(true)
    setError(null)
    try {
      const report = await exportThermalTuningV2(recording, deviceId, transport)
      const blob = new Blob([Uint8Array.from(report.archive)], { type: 'application/zip' })
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = `thermal-tuning-v2-${recording.runId.replace(/[^a-zA-Z0-9_-]/g, '_')}.zip`
      anchor.click()
      window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
    } catch (failure) {
      setError(messageOf(failure))
    } finally {
      setPending(false)
    }
  }

  const identity = {
    runId: status?.runId ?? undefined,
    candidateId: status?.candidateId ?? undefined,
    candidateHash: status?.candidateHash ?? undefined,
    powerClass: status?.powerClass ?? undefined,
  }
  const canSeal =
    identityVerified && terminal === 'candidate_ready' && recording?.complete && !status?.sealed
  const canSave =
    identityVerified &&
    terminal === 'candidate_ready' &&
    recording?.complete &&
    status?.sealed &&
    status.previewReceipt != null &&
    !status.saveConfirmed

  return (
    <section className="firmware-tuning" aria-label="固件热控调优">
      <header className="firmware-tuning__header">
        <p className="firmware-tuning__context">
          {powerClass ? `${powerClassLabel(powerClass)} · ` : '功率档位未知 · '}
          {status?.runId ?? '未运行'}
        </p>
        <span
          className={`firmware-tuning__outcome firmware-tuning__outcome--${terminal ?? 'idle'}`}
        >
          {terminalLabel(terminal)}
        </span>
      </header>

      <div className="firmware-tuning__toolbar">
        <div className="firmware-tuning__primary-actions" aria-label="调优操作">
          <Button
            type="button"
            disabled={
              !supported ||
              !journalReady ||
              !identityVerified ||
              !powerClass ||
              armed ||
              terminal === 'candidate_ready' ||
              pending
            }
            onClick={start}
          >
            {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            开始调优
          </Button>
          <Button
            type="button"
            variant="destructive"
            disabled={!supported || !identityVerified || !armed || pending}
            onClick={() => void apply({ op: 'cancel', runId: status?.runId ?? undefined })}
          >
            <Square aria-hidden="true" /> 停止
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={!canSeal || pending}
            onClick={() => void apply({ op: 'seal', ...identity })}
          >
            封存候选
          </Button>
          <Button
            type="button"
            disabled={!canSave || pending}
            onClick={() => void apply({ op: 'confirm_save', ...identity })}
          >
            <Save aria-hidden="true" /> 确认保存
          </Button>
          <Button
            type="button"
            variant="outline"
            disabled={!recording?.runId || pending}
            onClick={() => void exportReport()}
          >
            <Download aria-hidden="true" /> 导出报告
          </Button>
        </div>
        <details className="firmware-tuning__advanced">
          <summary>高级选项：导入候选</summary>
          <div className="firmware-tuning__advanced-content">
            <input
              ref={fileInput}
              type="file"
              accept=".json,application/json"
              hidden
              onChange={(event) => void importCandidate(event.currentTarget.files?.[0])}
            />
            <Button type="button" variant="outline" onClick={() => fileInput.current?.click()}>
              <Upload aria-hidden="true" /> 导入候选
            </Button>
            <span className="firmware-tuning__candidate" title={candidate?.hash}>
              {candidate?.name ?? '未选择高级候选，使用固件工厂缺省基线'}
            </span>
          </div>
        </details>
      </div>

      {!supported ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 当前设备固件不支持调优事件协议。
        </p>
      ) : null}
      {supported && readState === 'failed' ? (
        <div className="firmware-tuning__warning">
          <span>{error ?? '固件调优读取失败。'}</span>
          <Button type="button" variant="outline" size="sm" onClick={retryRead} disabled={pending}>
            重试读取
          </Button>
        </div>
      ) : null}
      {supported && !powerClass ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 当前设备尚未解析出 PPS 功率档位，无法安全启动调优。
        </p>
      ) : null}
      {recording?.gap ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 事件缺口：{recording.gapReason}
        </p>
      ) : null}
      {error && readState !== 'failed' ? <p className="firmware-tuning__warning">{error}</p> : null}

      {parameterSnapshot?.control ? (
        <section className="firmware-tuning__parameters" aria-label="当前热控参数">
          <header>
            <div>
              <h3>当前参数</h3>
              <p>
                {parameterSourceLabel(parameterSnapshot.control.profileSource)} ·{' '}
                {parameterSnapshot.bank ?? '未解析功率档位'}
                {parameterSnapshot.mode ? ` · ${parameterSnapshot.mode}` : ''}
              </p>
            </div>
            <span>
              {parameterSnapshot.control.profileCoversTarget ? '覆盖当前目标' : '工厂缺省基线'}
            </span>
          </header>
          <div className="firmware-tuning__parameter-grid">
            <Parameter label="目标温度" value={`${parameterSnapshot.control.targetTempC}°C`} />
            <Parameter
              label="预热功率"
              value={permillePercent(parameterSnapshot.control.warmupPowerPermille)}
            />
            <Parameter
              label="接近功率"
              value={permillePercent(parameterSnapshot.control.approachPowerPermille)}
            />
            <Parameter
              label="保持功率"
              value={permillePercent(parameterSnapshot.control.holdPowerPermille)}
            />
            <Parameter
              label="刹车距离"
              value={`${(parameterSnapshot.control.brakeDistanceCentiC / 100).toFixed(1)}°C`}
            />
            <Parameter
              label="温度滤波"
              value={permillePercent(parameterSnapshot.control.tempFilterAlphaPermille)}
            />
          </div>
        </section>
      ) : null}

      <div className="firmware-tuning__facts">
        <Fact
          label="目标"
          value={
            status?.targetsC[status.targetIndex] != null
              ? `${status.targetsC[status.targetIndex]}°C`
              : '—'
          }
        />
        <Fact label="阶段" value={status?.phase ?? '—'} />
        <Fact label="轮次" value={status?.trialPurpose ?? '—'} />
        <Fact label="设备事件" value={`${recording?.events.length ?? 0}`} />
        <Fact
          label="审查"
          value={reviewDispositionLabel(status?.reviewDisposition, Boolean(recording?.runId))}
        />
        <Fact
          label="追踪"
          value={trackingLabel(recording)}
        />
      </div>

      <div className="firmware-tuning__detail">
        <section>
          <h3>候选与保存</h3>
          <dl>
            <IdentifierValue
              kind="candidate"
              value={status?.candidateId}
              copied={copiedIdentifier === 'candidate'}
              onCopy={copyIdentifier}
            />
            <IdentifierValue
              kind="hash"
              value={status?.candidateHash}
              copied={copiedIdentifier === 'hash'}
              onCopy={copyIdentifier}
            />
            <div className="firmware-tuning__detail-row">
              <dt>RAM preview</dt>
              <dd>{status?.previewReceipt ? '已核对' : '未核对'}</dd>
            </div>
            <div className="firmware-tuning__detail-row">
              <dt>EEPROM</dt>
              <dd>
                {status?.saveReceipt?.readbackVerified
                  ? `已读回 · revision ${status.saveReceipt.bankRevision}`
                  : '未保存'}
              </dd>
            </div>
          </dl>
        </section>
        <section>
          <h3>设备事件</h3>
          <ScrollArea className="firmware-tuning__event-scroll" autoHide>
            <table>
              <thead>
                <tr>
                  <th>序号</th>
                  <th>时间</th>
                  <th>类型</th>
                  <th>目标</th>
                  <th>记录</th>
                </tr>
              </thead>
              <tbody>
                {recording?.events.slice(-80).map((event) => (
                  <EventRow key={event.sequence} event={event} />
                ))}
              </tbody>
            </table>
          </ScrollArea>
        </section>
      </div>
    </section>
  )
}

function parameterSourceLabel(source: string) {
  switch (source) {
    case 'default':
      return '固件工厂缺省基线'
    case 'saved':
      return '设备已保存参数'
    case 'preview':
      return '当前预览参数'
    default:
      return source || '参数来源未知'
  }
}

function powerClassLabel(powerClass: 'pps3a' | 'pps5a') {
  return powerClass === 'pps3a' ? 'PPS 3A' : 'PPS 5A'
}

export function compactIdentifier(value: string | null | undefined) {
  if (!value) return '—'
  if (value.length <= 24) return value
  return `${value.slice(0, 12)}…${value.slice(-8)}`
}

export function resolveTuningPowerClass(
  bank: string | undefined,
  statusPowerClass: ThermalTuningStatus['powerClass'] | undefined
): ThermalTuningStatus['powerClass'] {
  if (bank === 'pps3a' || bank === 'pps5a') return bank
  if (statusPowerClass === 'pps3a' || statusPowerClass === 'pps5a') return statusPowerClass
  return null
}

function IdentifierValue({
  kind,
  value,
  copied,
  onCopy,
}: {
  kind: 'candidate' | 'hash'
  value: string | null | undefined
  copied: boolean
  onCopy: (kind: 'candidate' | 'hash', value: string | null | undefined) => void
}) {
  const label = kind === 'candidate' ? 'Candidate' : 'Hash'
  const summary = compactIdentifier(value)
  return (
    <TooltipProvider>
      <div className="firmware-tuning__identifier-field">
        <div className="firmware-tuning__identifier-header">
          <span>{label}</span>
          <button
            type="button"
            className="firmware-tuning__identifier-copy"
            aria-label={`复制完整${label}`}
            title={copied ? '已复制' : `复制完整${label}`}
            disabled={!value}
            onClick={() => onCopy(kind, value)}
          >
            {copied ? <Clipboard aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </button>
        </div>
        {value ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="firmware-tuning__identifier-summary">{summary}</span>
            </TooltipTrigger>
            <TooltipContent className="firmware-tuning__identifier-tooltip">
              {value}
            </TooltipContent>
          </Tooltip>
        ) : (
          <span className="firmware-tuning__identifier-summary firmware-tuning__identifier-empty">
            {summary}
          </span>
        )}
      </div>
    </TooltipProvider>
  )
}

function reviewDispositionLabel(
  disposition: ThermalTuningStatus['reviewDisposition'] | undefined,
  hasRun: boolean
) {
  if (!hasRun) return '未开始'
  switch (disposition) {
    case 'complete':
      return '已完成'
    case 'review_incomplete':
      return '审查不完整'
    case 'pending':
      return '待审查'
    default:
      return '未开始'
  }
}

function trackingLabel(next: ThermalTuningRecording | null) {
  if (!next?.runId) return '未开始'
  if (next.complete) return '完整'
  if (next.gap) return '缺口'
  return '收集中'
}

function permillePercent(value: number) {
  return `${(value / 10).toFixed(1)}%`
}

function Parameter({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  )
}

export async function validateCandidate(value: unknown, name: string): Promise<ImportedCandidate> {
  if (!value || typeof value !== 'object') throw new Error('候选文件不是 JSON 对象。')
  const candidate = value as Record<string, unknown>
  const packageProfile = candidate.profile
  if (candidate.powerClass !== 'pps5a' || typeof candidate.candidateHash !== 'string') {
    throw new Error('候选必须包含 pps5a powerClass 和 candidateHash。')
  }
  if (!/^sha256:[0-9a-f]{64}$/.test(candidate.candidateHash)) {
    throw new Error('候选 hash 不是完整的 SHA-256。')
  }
  if (
    (typeof candidate.runId === 'string' && candidate.runId.startsWith('mock-')) ||
    (typeof candidate.candidateId === 'string' && candidate.candidateId.startsWith('mock-'))
  ) {
    throw new Error('Mock 报告候选不得用于真实加热。')
  }
  if (!packageProfile || typeof packageProfile !== 'object')
    throw new Error('候选缺少完整 profile。')
  const wrapped = packageProfile as Record<string, unknown>
  const profile = wrapped.value ?? packageProfile
  if (!profile || typeof profile !== 'object') throw new Error('候选缺少设备 profile。')
  const importedProfile = profile as Record<string, unknown>
  if (!importedProfile.settings || typeof importedProfile.settings !== 'object') {
    throw new Error('候选缺少完整 settings。')
  }
  const settings = importedProfile.settings as Record<string, unknown>
  const points = importedProfile.points
  const populatedPoints = Array.isArray(points)
    ? points
        .filter((point) => point != null)
        .sort((left, right) => left.targetTempC - right.targetTempC)
    : []
  if (
    !Array.isArray(points) ||
    populatedPoints.length !== EXECUTION_ORDER.length ||
    !EXECUTION_ORDER.every((target) =>
      populatedPoints.some((point) => point?.targetTempC === target)
    )
  ) {
    throw new Error('候选必须覆盖完整八目标 PPS 5A 范围。')
  }
  const values = [
    ...SETTINGS_FIELDS.map((field) => settings[field]),
    ...populatedPoints.flatMap((point) => POINT_FIELDS.map((field) => point[field])),
  ]
  if (values.some((item) => !Number.isInteger(item) || Number(item) < 0 || Number(item) > 65535)) {
    throw new Error('候选存在缺失或非法的 point/settings 参数。')
  }
  const hash = await candidateProfileHash(values as number[])
  if (hash !== candidate.candidateHash) throw new Error('候选 hash 与设备参数不一致。')
  return { hash: candidate.candidateHash, profile: profile as Record<string, unknown>, name }
}

async function candidateProfileHash(values: number[]) {
  const bytes = [...new TextEncoder().encode('thermal-tuning-v2/candidate/pps5a')]
  for (const value of values) bytes.push(value & 0xff, (value >> 8) & 0xff)
  const digest = await crypto.subtle.digest('SHA-256', Uint8Array.from(bytes))
  return `sha256:${[...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, '0')).join('')}`
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  )
}

function EventRow({ event }: { event: ThermalTuningEvent }) {
  return (
    <tr>
      <td>{event.sequence}</td>
      <td>{(event.elapsedMs / 1000).toFixed(1)}s</td>
      <td>{event.kind}</td>
      <td>{event.targetC ?? '—'}</td>
      <td>{event.reason ?? event.decision?.stopReason?.toString() ?? '—'}</td>
    </tr>
  )
}

function terminalLabel(value: ThermalTuningStatus['terminalDisposition'] | undefined) {
  switch (value) {
    case 'running':
      return '运行中'
    case 'candidate_ready':
      return '待审查候选'
    case 'passed':
      return '通过'
    case 'failed':
      return '失败'
    case 'canceled':
      return '已取消'
    default:
      return '待命'
  }
}

export function visibleTerminalDisposition(
  status: Pick<ThermalTuningStatus, 'armed' | 'runId' | 'terminalDisposition'> | undefined
) {
  return status?.armed || status?.runId ? status.terminalDisposition : undefined
}

function messageOf(error: unknown) {
  if (error instanceof ControlPlaneClientError) {
    if (
      error.category === 'thermal_tuning_unsupported' ||
      error.code === 'thermal_tuning_unsupported'
    ) {
      return '当前固件未提供热控调优接口。'
    }
    if (error.category === 'unauthorized' || error.code === 'unauthorized') {
      return '设备拒绝了当前配对凭据，请重新配对。'
    }
    if (error.category === 'transport_unavailable' || error.code === 'lan_transport_unavailable') {
      return '设备已连接，但固件热控调优读取暂时失败，请稍后重试。'
    }
  }
  return error instanceof Error ? error.message : '设备操作失败。'
}
