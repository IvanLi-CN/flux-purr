import {
  CheckCircle2,
  Clipboard,
  Copy,
  Download,
  LoaderCircle,
  Play,
  RotateCw,
  Save,
  ShieldAlert,
  Square,
  Trash2,
  Upload,
  X,
} from 'lucide-react'
import { Popover, RadioGroup } from 'radix-ui'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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
import { tuningOwnerSession } from '../tuning-owner-session'
import {
  FirmwareTuningTemperatureChart,
  tuningChartTargets,
} from './firmware-tuning-temperature-chart'
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

export function shouldPollTuningRead({
  supported,
  journalReady,
  pending,
  reading,
  gap,
  complete,
  identityVerified,
  failedAttempts,
}: {
  supported: boolean
  journalReady: boolean
  pending: boolean
  reading: boolean
  gap: boolean
  complete: boolean
  identityVerified: boolean
  failedAttempts: number
}) {
  return (
    supported &&
    journalReady &&
    !pending &&
    !reading &&
    !gap &&
    !(complete && identityVerified) &&
    failedAttempts < 3
  )
}

interface ImportedCandidate {
  powerClass: 'pps3a' | 'pps5a'
  hash: string
  profile: Record<string, unknown>
  name: string
}

export function FirmwareTuningWorkspace({
  supported,
  writesBlocked = false,
  deviceId,
  transport,
  parameterSnapshot,
  readStatus,
  sendCommand,
}: {
  supported: boolean
  writesBlocked?: boolean
  deviceId: string
  transport: string
  parameterSnapshot?: {
    mode?: string
    bank?: string
    control?: ThermalControlRuntime
  }
  readStatus: (
    cursor?: Pick<ThermalTuningCommand, 'runId' | 'afterSequence'>
  ) => Promise<ThermalTuningStatus>
  sendCommand: (command: DeviceCommand) => Promise<ThermalTuningStatus>
}) {
  const [recording, setRecording] = useState<ThermalTuningRecording | null>(null)
  const [lastLocalRun, setLastLocalRun] = useState<ThermalTuningRecording | null>(null)
  const [showEvents, setShowEvents] = useState(false)
  const recordingRef = useRef<ThermalTuningRecording | null>(null)
  const readStatusRef = useRef(readStatus)
  const sendCommandRef = useRef(sendCommand)
  readStatusRef.current = readStatus
  sendCommandRef.current = sendCommand
  const [journalReady, setJournalReady] = useState(false)
  const [identityVerified, setIdentityVerified] = useState(false)
  const [candidate, setCandidate] = useState<ImportedCandidate | null>(null)
  const [pending, setPending] = useState(false)
  const [saveFeedback, setSaveFeedback] = useState<'idle' | 'loading' | 'success'>('idle')
  const [error, setError] = useState<string | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [pollAttempt, setPollAttempt] = useState(0)
  const [copiedIdentifier, setCopiedIdentifier] = useState<'candidate' | 'hash' | null>(null)
  const [readState, setReadState] = useState<
    'idle' | 'reading' | 'ready' | 'failed' | 'unsupported'
  >(supported ? 'idle' : 'unsupported')
  const fileInput = useRef<HTMLInputElement>(null)
  const canceledGapRun = useRef<string | null>(null)
  const mountedRef = useRef(true)
  const readInFlightRef = useRef(false)
  const commandEpochRef = useRef(0)
  const status = recording?.status
  const armed = status?.armed === true
  // Firmware keeps the terminal disposition at `running` in its idle wire
  // status for compatibility.  A run is only active when it has been armed
  // or carries a run identity; otherwise the workbench must show standby.
  const terminal = visibleTerminalDisposition(status)
  const powerClass = resolveTuningPowerClass(parameterSnapshot?.bank, status?.powerClass)

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  useEffect(() => {
    if (saveFeedback !== 'success') return
    const timer = window.setTimeout(() => setSaveFeedback('idle'), 3_000)
    return () => window.clearTimeout(timer)
  }, [saveFeedback])

  useEffect(() => {
    let cancelled = false
    recordingRef.current = null
    setRecording(null)
    setLastLocalRun(null)
    setJournalReady(false)
    setIdentityVerified(false)
    setCommandError(null)
    setReadState(supported ? 'idle' : 'unsupported')
    void loadThermalTuningRecording(deviceId)
      .then((restored) => {
        if (cancelled) return
        setLastLocalRun(restored)
        // A persisted trace gap belongs to the previous run. Reusing its
        // cursor would keep every new read blocked forever; the next status
        // request must establish a fresh cursor for the current device run.
        const reusable = restored?.gap ? null : restored
        recordingRef.current = reusable
        setRecording(reusable)
        setJournalReady(true)
      })
      .catch((failure) => {
        if (cancelled) return
        setError(messageOf(failure))
        if (supported) {
          void readStatusRef
            .current()
            .then((current) => {
              if (current.armed && tuningOwnerSession.proofFor(deviceId, current.runId))
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
      const publicStatus = tuningOwnerSession.acceptResponse(deviceId, 'read', next)
      const updated = recordThermalTuningPage(recordingRef.current, publicStatus, confirmedSequence)
      // Acknowledging the device page must not wait for IndexedDB. The
      // firmware keeps a bounded event ring; delaying the next afterSequence
      // request behind a growing journal write can make that ring overflow
      // and turn a healthy run into device_trace_gap.
      recordingRef.current = updated
      setRecording(updated)
      if (updated.runId) setLastLocalRun(updated)
      // A later successful status page proves the endpoint is reachable again;
      // clear a stale transport warning left by an earlier bounded retry.
      setError(null)
      if (updated.runId) {
        // Device acknowledgement is on the hot path; IndexedDB is a journal,
        // not a prerequisite for issuing the next read. A blocked or slow
        // browser transaction must never stall the bounded firmware event ring.
        void persistThermalTuningRecording(deviceId, updated).catch(async (failure) => {
          setError(messageOf(failure))
          if (
            updated.status.armed &&
            updated.runId &&
            tuningOwnerSession.proofFor(deviceId, updated.runId)
          ) {
            try {
              await sendCommand({ op: 'cancel', runId: updated.runId })
            } catch {
              // Preserve the original persistence error in the UI.
            }
          }
        })
      }
    },
    [deviceId, sendCommand]
  )

  useEffect(() => {
    if (
      !shouldPollTuningRead({
        supported,
        journalReady,
        pending,
        reading: readState === 'reading',
        gap: recording?.gap === true,
        complete: recording?.complete === true,
        identityVerified,
        failedAttempts: pollAttempt,
      })
    )
      return
    if (readInFlightRef.current) return
    const timer = window.setTimeout(
      async () => {
        readInFlightRef.current = true
        const commandEpoch = commandEpochRef.current
        setReadState('reading')
        try {
          const command = nextTuningReadCommand(recording, identityVerified)
          const confirmedSequence = command?.afterSequence ?? null
          const next = await readStatus(command ?? undefined)
          if (mountedRef.current && commandEpoch === commandEpochRef.current) {
            await recordStatus(next, confirmedSequence)
            setIdentityVerified(true)
            setError(null)
            setPollAttempt(0)
            setReadState('ready')
          }
        } catch (failure) {
          if (mountedRef.current && commandEpoch === commandEpochRef.current) {
            setError(messageOf(failure))
            // Keep the last snapshot and cursor, but never silently reset the
            // retry budget for an active run. Manual retry starts a new round.
            setReadState('failed')
            setPollAttempt((attempt) =>
              failure instanceof ControlPlaneClientError && !failure.retryable
                ? 3
                : Math.min(3, attempt + 1)
            )
          }
        } finally {
          readInFlightRef.current = false
          if (mountedRef.current && commandEpoch !== commandEpochRef.current) {
            setReadState((current) => (current === 'reading' ? 'idle' : current))
          }
        }
      },
      // The firmware emits five sample records per second and returns at
      // most six records per page. While armed, poll often enough to keep
      // the bounded device ring below its capacity without overlapping
      // requests; the 300ms floor is far above the old 75ms starvation
      // threshold that could compete with the control/PD tasks.
      Math.min(2_000, (armed ? 300 : 500) + pollAttempt * 150)
    )
    return () => {
      window.clearTimeout(timer)
    }
  }, [
    armed,
    identityVerified,
    journalReady,
    pending,
    pollAttempt,
    readStatus,
    readState,
    recordStatus,
    recording,
    supported,
  ])

  useEffect(() => {
    if (!recording?.gap || !recording.status.armed || !recording.runId) return
    if (!tuningOwnerSession.proofFor(deviceId, recording.runId)) return
    if (canceledGapRun.current === recording.runId) return
    canceledGapRun.current = recording.runId
    void sendCommand({ op: 'cancel', runId: recording.runId })
      .then((next) => recordStatus(next))
      .catch((failure) => setError(messageOf(failure)))
  }, [deviceId, recordStatus, recording, sendCommand])

  const apply = async (command: DeviceCommand, reviewAction?: 'save' | 'discard') => {
    // Stopping is the authenticated safety exception: read/journal recovery
    // must never prevent a user from stopping the last observed active run.
    if ((command.op !== 'cancel' || reviewAction) && (!journalReady || !identityVerified)) return
    // A read already in flight belongs to the state before this command.
    // Its late response must not replace a newly started or stopped run.
    commandEpochRef.current += 1
    setPending(true)
    setCommandError(null)
    const minimumSaveTime =
      reviewAction === 'save'
        ? new Promise<void>((resolve) => window.setTimeout(resolve, 1_000))
        : null
    if (minimumSaveTime) setSaveFeedback('loading')
    let saveSucceeded = false
    try {
      if (reviewAction === 'save') {
        await saveTuningResult(
          command,
          status?.sealed === true,
          async (step) => {
            const next = await tuningOwnerSession.command(deviceId, step, sendCommand)
            await recordStatus(next)
            return next
          },
          async () => {
            // Sealing adds an event; the device requires it to be acknowledged before saving.
            for (let attempt = 0; attempt < 4; attempt += 1) {
              const current = recordingRef.current
              if (!current || current.runId !== command.runId || current.gap) break
              if (current.complete) return current.status
              const cursor = nextTuningReadCommand(current, true)
              if (!cursor) break
              const next = await readStatus(cursor)
              await recordStatus(next, cursor.afterSequence)
            }
            const current = recordingRef.current
            if (current?.complete && current.runId === command.runId) return current.status
            throw new Error('本轮调优记录尚未读完，未保存。请稍后重试。')
          }
        )
        saveSucceeded = true
        setReadState('ready')
        return
      }
      if (reviewAction === 'discard') {
        const current = await readStatus()
        await recordStatus(current)
        if (
          current.runId !== command.runId ||
          current.terminalDisposition !== 'candidate_ready' ||
          current.armed ||
          current.saveConfirmed
        ) {
          throw new Error('调优结果已变化，请查看当前状态。')
        }
      }
      const next = await tuningOwnerSession.command(deviceId, command, sendCommand)
      await recordStatus(next)
      setReadState('ready')
    } catch (failure) {
      if (minimumSaveTime) await minimumSaveTime
      if (!mountedRef.current) return
      setCommandError(messageOf(failure, 'write'))
      if (reviewAction) {
        setIdentityVerified(false)
        setPollAttempt(0)
        setReadState('idle')
      }
    } finally {
      if (minimumSaveTime) await minimumSaveTime
      if (mountedRef.current) {
        if (minimumSaveTime) setSaveFeedback(saveSucceeded ? 'success' : 'idle')
        setPending(false)
      }
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
      window.setTimeout(
        () => setCopiedIdentifier((current) => (current === kind ? null : current)),
        1600
      )
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
    try {
      void apply(buildTuningStartCommand(runId, powerClass, candidate))
    } catch (failure) {
      setError(messageOf(failure, 'write'))
    }
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
  const chartRecording = recording?.runId ? recording : lastLocalRun
  const saveVerified =
    status?.saveConfirmed === true && status.saveReceipt?.readbackVerified === true
  const reviewing = terminal === 'candidate_ready' && !armed && !status?.saveConfirmed
  const showingSave = saveFeedback !== 'idle'
  const savedRunInStandby = saveVerified && !armed && !showingSave
  const currentRun = savedRunInStandby ? null : recording
  const currentStatus = currentRun?.status
  const targetSequence = EXECUTION_ORDER
  const availableTuningTargets = useMemo(
    () => tuningChartTargets(chartRecording?.events ?? []),
    [chartRecording?.events]
  )
  const currentRunTarget =
    currentStatus?.targetsC[currentStatus.targetIndex] ?? currentStatus?.targetsC.at(-1)
  const [requestedChartTarget, setRequestedChartTarget] = useState<number | null>(null)
  // Only recorded targets are selectable, so a request that no longer has data
  // (for example after switching runs) must fall back instead of pinning an
  // empty chart.
  const chartTarget =
    requestedChartTarget != null && availableTuningTargets.includes(requestedChartTarget)
      ? requestedChartTarget
      : currentRunTarget != null && availableTuningTargets.includes(currentRunTarget)
        ? currentRunTarget
        : (availableTuningTargets[0] ?? EXECUTION_ORDER[0])
  const canStart =
    supported &&
    journalReady &&
    identityVerified &&
    Boolean(powerClass) &&
    !armed &&
    !writesBlocked &&
    terminal !== 'candidate_ready' &&
    !showingSave &&
    !pending
  const ownsResult = tuningOwnerSession.proofFor(deviceId, status?.runId) != null
  const canSave =
    journalReady &&
    identityVerified &&
    ownsResult &&
    reviewing &&
    (status?.sealed || recording?.complete) &&
    status?.previewReceipt != null

  return (
    <section className="firmware-tuning" aria-label="固件热控调优">
      <div className="firmware-tuning__toolbar">
        <fieldset className="firmware-tuning__primary-actions" aria-label="调优操作">
          {reviewing || showingSave ? (
            <>
              <Button
                type="button"
                className="firmware-tuning__save-button"
                data-saved={saveFeedback === 'success' || undefined}
                disabled={!supported || !canSave || writesBlocked || pending || showingSave}
                aria-busy={saveFeedback === 'loading'}
                aria-describedby={
                  reviewing && !showingSave && identityVerified && !ownsResult
                    ? 'tuning-save-owner'
                    : undefined
                }
                onClick={() => void apply({ op: 'confirm_save', ...identity }, 'save')}
              >
                {saveFeedback === 'loading' ? (
                  <>
                    <LoaderCircle className="motion-safe:animate-spin" aria-hidden="true" /> 保存中
                  </>
                ) : saveFeedback === 'success' ? (
                  <>
                    <CheckCircle2 aria-hidden="true" /> 已保存
                  </>
                ) : (
                  <>
                    <Save aria-hidden="true" /> 保存
                  </>
                )}
              </Button>
              <Popover.Root modal>
                <Popover.Trigger asChild>
                  <Button
                    type="button"
                    variant="outline"
                    disabled={
                      !supported ||
                      !journalReady ||
                      !identityVerified ||
                      writesBlocked ||
                      pending ||
                      showingSave
                    }
                  >
                    <Trash2 aria-hidden="true" /> 放弃
                  </Button>
                </Popover.Trigger>
                <Popover.Portal>
                  <Popover.Content
                    className="firmware-tuning__discard-popover"
                    align="start"
                    sideOffset={8}
                    aria-label="放弃本轮调优结果？"
                  >
                    <p>放弃本轮调优结果？</p>
                    <div>
                      <Popover.Close asChild>
                        <Button type="button" variant="outline" size="sm">
                          <X aria-hidden="true" /> 取消
                        </Button>
                      </Popover.Close>
                      <Popover.Close asChild>
                        <Button
                          type="button"
                          variant="destructive"
                          size="sm"
                          onClick={() =>
                            void apply({ op: 'cancel', runId: identity.runId }, 'discard')
                          }
                        >
                          <Trash2 aria-hidden="true" /> 确认放弃
                        </Button>
                      </Popover.Close>
                    </div>
                  </Popover.Content>
                </Popover.Portal>
              </Popover.Root>
            </>
          ) : (
            <>
              <Button type="button" disabled={!canStart} onClick={start}>
                {pending ? (
                  <LoaderCircle className="motion-safe:animate-spin" aria-hidden="true" />
                ) : (
                  <Play aria-hidden="true" />
                )}
                开始调优
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={!supported || !armed || pending}
                onClick={() => void apply({ op: 'cancel', runId: status?.runId ?? undefined })}
              >
                <Square aria-hidden="true" /> 停止
              </Button>
            </>
          )}
          <Button
            type="button"
            variant="outline"
            disabled={!recording?.runId || pending}
            onClick={() => void exportReport()}
          >
            <Download aria-hidden="true" /> 导出报告
          </Button>
          {saveFeedback === 'success' ? (
            <output className="firmware-tuning__save-success">调优参数已保存到设备</output>
          ) : null}
        </fieldset>
        {reviewing && !showingSave && identityVerified && !ownsResult ? (
          <p id="tuning-save-owner" className="firmware-tuning__warning">
            请在发起本轮调优的页面保存。
          </p>
        ) : null}
        {canStart ? (
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
        ) : null}
      </div>

      {!supported ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 当前设备固件不支持调优事件协议。
        </p>
      ) : null}
      {supported && readState === 'failed' ? (
        <div className="firmware-tuning__warning">
          <span>
            {saveVerified
              ? '调优记录读取失败，已保存的参数不受影响。'
              : (error ?? '固件调优读取失败。')}
          </span>
          <Button type="button" variant="outline" size="sm" onClick={retryRead} disabled={pending}>
            <RotateCw aria-hidden="true" /> 重试读取
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
      {commandError ? (
        <p className="firmware-tuning__warning" role="alert">
          {commandError}
        </p>
      ) : null}

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
            currentStatus?.targetsC[currentStatus.targetIndex] != null
              ? `${currentStatus.targetsC[currentStatus.targetIndex]}°C`
              : '—'
          }
        />
        <Fact
          label="状态"
          value={
            savedRunInStandby
              ? terminalLabel(undefined)
              : saveVerified && saveFeedback === 'success'
                ? '已保存'
                : terminal === 'running'
                  ? (status?.phase ?? terminalLabel(terminal))
                  : terminalLabel(terminal)
          }
        />
        <Fact label="轮次" value={currentStatus?.trialPurpose ?? '—'} />
        <Fact label="设备事件" value={`${currentRun?.events.length ?? 0}`} />
        <Fact
          label="审查"
          value={reviewDispositionLabel(
            currentStatus?.reviewDisposition,
            Boolean(currentRun?.runId)
          )}
        />
        <Fact label="追踪" value={trackingLabel(currentRun)} />
      </div>

      <div className="firmware-tuning__detail">
        <section>
          <h3>{savedRunInStandby ? '上次保存结果' : '候选与保存'}</h3>
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
          <div className="firmware-tuning__target-sequence">
            <div className="firmware-tuning__target-sequence-heading">
              <h4>调优目标顺序</h4>
              <span>
                {availableTuningTargets.length > 0
                  ? `当前 ${chartTarget}°C · 共 ${targetSequence.length} 个目标`
                  : `${targetSequence.length} 个目标`}
              </span>
            </div>
            <RadioGroup.Root
              className="firmware-tuning__target-grid"
              aria-label="曲线目标温度"
              value={availableTuningTargets.length > 0 ? String(chartTarget) : ''}
              onValueChange={(value) => setRequestedChartTarget(Number(value))}
            >
              {targetSequence.map((targetC) => {
                // Only targets present in the current trace can be plotted; the
                // rest are shown but disabled so the grid keeps the firmware's
                // eight-target shape without offering dead options.
                const recorded = availableTuningTargets.includes(targetC)
                return (
                  <RadioGroup.Item
                    key={targetC}
                    value={String(targetC)}
                    aria-label={`${targetC}°C`}
                    disabled={!recorded}
                    checked={recorded ? chartTarget === targetC : false}
                    className={recorded && chartTarget === targetC ? 'is-selected' : undefined}
                    data-recorded={recorded}
                  >
                    <strong>{targetC}°C</strong>
                    {recorded ? null : <small>无采样</small>}
                  </RadioGroup.Item>
                )
              })}
            </RadioGroup.Root>
          </div>
        </section>
        <section className="firmware-tuning__results">
          <header className="firmware-tuning__detail-toolbar">
            <h3>
              {showEvents
                ? '设备事件'
                : chartRecording && !currentRun?.runId
                  ? '上次调优曲线'
                  : '温度曲线'}
            </h3>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setShowEvents(!showEvents)}
            >
              {showEvents ? '温度曲线' : '设备事件'}
            </Button>
          </header>
          {showEvents ? (
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
                  {chartRecording?.events.slice(-80).map((event) => (
                    <EventRow key={event.sequence} event={event} />
                  ))}
                </tbody>
              </table>
            </ScrollArea>
          ) : (
            <FirmwareTuningTemperatureChart
              events={chartRecording?.events ?? []}
              selectedTarget={chartTarget}
              traceGap={chartRecording?.gap === true}
            />
          )}
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
            <TooltipContent className="firmware-tuning__identifier-tooltip">{value}</TooltipContent>
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

export function buildTuningStartCommand(
  runId: string,
  powerClass: 'pps3a' | 'pps5a',
  candidate: ImportedCandidate | null
): DeviceCommand {
  if (candidate && candidate.powerClass !== powerClass) {
    throw new Error('候选功率档位与设备当前档位不一致，请重新导入匹配的候选。')
  }
  return {
    op: 'start',
    runId,
    candidateId: candidate ? runId : undefined,
    candidateHash: candidate?.hash,
    powerClass,
    targetsC: EXECUTION_ORDER,
    candidateProfile: candidate
      ? { op: 'preview', bank: powerClass, profile: candidate.profile }
      : undefined,
  }
}

export async function validateCandidate(value: unknown, name: string): Promise<ImportedCandidate> {
  if (!value || typeof value !== 'object') throw new Error('候选文件不是 JSON 对象。')
  const candidate = value as Record<string, unknown>
  const packageProfile = candidate.profile
  if (
    (candidate.powerClass !== 'pps3a' && candidate.powerClass !== 'pps5a') ||
    typeof candidate.candidateHash !== 'string'
  ) {
    throw new Error('候选必须包含 pps3a 或 pps5a powerClass 和 candidateHash。')
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
  if (wrapped.bank !== undefined && wrapped.bank !== candidate.powerClass) {
    throw new Error('候选 profile bank 与 powerClass 的功率档位不一致。')
  }
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
    throw new Error('候选必须覆盖完整八目标调优范围。')
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
  return {
    powerClass: candidate.powerClass,
    hash: candidate.candidateHash,
    profile: profile as Record<string, unknown>,
    name,
  }
}

async function candidateProfileHash(values: number[]) {
  // Firmware retains this historical content-hash encoding for both banks;
  // power class is validated separately before importing or starting a run.
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
      <td>{eventReasonLabel(event.reason ?? event.decision?.stopReason?.toString())}</td>
    </tr>
  )
}

function eventReasonLabel(reason: string | undefined) {
  switch (reason) {
    case 'pd_contract_lost':
      return '未建立有效 PPS 电源合同，固件已安全停止加热'
    case 'pps_request_timeout':
      return 'PPS 电源合同协商超时，固件已安全停止加热'
    case 'unsafe_sample':
      return '采样未满足 PPS 安全条件，固件已安全停止加热'
    case 'invalid_pps_voltage_ceiling':
      return 'PPS 电压请求超出当前安全上限，固件已安全停止加热'
    default:
      return reason ?? '—'
  }
}

function terminalLabel(value: ThermalTuningStatus['terminalDisposition'] | undefined) {
  switch (value) {
    case 'running':
      return '运行中'
    case 'candidate_ready':
      return '待确认'
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

async function saveTuningResult(
  identity: Omit<DeviceCommand, 'op'>,
  sealed: boolean,
  send: (command: DeviceCommand) => Promise<ThermalTuningStatus>,
  acknowledge: () => Promise<ThermalTuningStatus>
) {
  if (!identity.runId || !identity.candidateId || !identity.candidateHash || !identity.powerClass) {
    throw new Error('无法确认本轮调优结果，请重新读取。')
  }
  if (!sealed) await send({ ...identity, op: 'seal' })
  const verified = await acknowledge()
  if (
    !verified.sealed ||
    verified.armed ||
    verified.saveConfirmed ||
    verified.terminalDisposition !== 'candidate_ready' ||
    verified.previewReceipt == null ||
    verified.runId !== identity.runId ||
    verified.candidateId !== identity.candidateId ||
    verified.candidateHash !== identity.candidateHash ||
    verified.powerClass !== identity.powerClass
  ) {
    throw new Error('设备未能校验本轮调优结果，未保存。')
  }
  const result = await send({ ...identity, op: 'confirm_save' })
  if (
    result.runId !== identity.runId ||
    !result.saveConfirmed ||
    result.saveReceipt?.readbackVerified !== true
  ) {
    throw new Error('未收到设备的保存确认，请查看当前状态。')
  }
}

export function messageOf(error: unknown, operation: 'read' | 'write' = 'read') {
  if (error instanceof ControlPlaneClientError) {
    if (operation === 'read' && error.code === 'response_too_large') {
      return '调优记录读取失败：设备返回的数据量超出上限。'
    }
    if (error.code === 'devd_lease_unavailable') return error.message
    if (error.code === 'lease_busy' || error.code === 'lease_conflict') {
      return '设备写入租约暂被占用；设备仍已连接，请在租约释放后重试操作。'
    }
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
      return operation === 'write'
        ? '设备已连接，但固件热控调优操作未完成，请核对运行状态后重试。'
        : '设备已连接，但固件热控调优读取暂时失败，请稍后重试。'
    }
  }
  return error instanceof Error ? error.message : '设备操作失败。'
}
