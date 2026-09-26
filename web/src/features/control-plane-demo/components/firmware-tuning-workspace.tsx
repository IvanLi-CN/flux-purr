import { LoaderCircle, Save, ShieldAlert, Square, Upload } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import type { ThermalTuningCommand, ThermalTuningEvent, ThermalTuningStatus } from '../contracts'
import { recordThermalTuningPage, type ThermalTuningRecording } from '../thermal-tuning-recorder'
import './firmware-tuning-workspace.css'

const EXECUTION_ORDER = [60, 240, 140, 100, 80, 120, 180, 160, 220]
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
  readStatus,
  sendCommand,
}: {
  supported: boolean
  readStatus: () => Promise<ThermalTuningStatus>
  sendCommand: (command: DeviceCommand) => Promise<ThermalTuningStatus>
}) {
  const [recording, setRecording] = useState<ThermalTuningRecording | null>(null)
  const [candidate, setCandidate] = useState<ImportedCandidate | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pollAttempt, setPollAttempt] = useState(0)
  const fileInput = useRef<HTMLInputElement>(null)
  const canceledGapRun = useRef<string | null>(null)
  const status = recording?.status
  const armed = status?.armed === true
  const terminal = status?.terminalDisposition

  useEffect(() => {
    if (!supported || pending || recording?.gap || recording?.complete) return
    let cancelled = false
    const timer = window.setTimeout(
      async () => {
        try {
          const confirmedSequence = recording?.acknowledgedThrough ?? null
          const next =
            recording?.runId && confirmedSequence != null
              ? await sendCommand({
                  op: 'read',
                  runId: recording.runId,
                  afterSequence: confirmedSequence,
                })
              : await readStatus()
          if (!cancelled) {
            setRecording((current) => recordThermalTuningPage(current, next, confirmedSequence))
            setError(null)
            setPollAttempt(0)
          }
        } catch (failure) {
          if (!cancelled) {
            setError(messageOf(failure))
            setPollAttempt((attempt) => attempt + 1)
          }
        }
      },
      Math.min(2_000, (armed ? 75 : 500) + pollAttempt * 150)
    )
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [armed, pending, pollAttempt, readStatus, recording, sendCommand, supported])

  useEffect(() => {
    if (!recording?.gap || !recording.status.armed || !recording.runId) return
    if (canceledGapRun.current === recording.runId) return
    canceledGapRun.current = recording.runId
    void sendCommand({ op: 'cancel', runId: recording.runId })
      .then((next) => setRecording((current) => recordThermalTuningPage(current, next)))
      .catch((failure) => setError(messageOf(failure)))
  }, [recording, sendCommand])

  const apply = async (command: DeviceCommand) => {
    setPending(true)
    setError(null)
    try {
      const next = await sendCommand(command)
      setRecording((current) => recordThermalTuningPage(current, next))
    } catch (failure) {
      setError(messageOf(failure))
    } finally {
      setPending(false)
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
    if (!candidate) return
    const runId = `web-5a-${crypto.randomUUID()}`
    void apply({
      op: 'start',
      runId,
      candidateId: runId,
      candidateHash: candidate.hash,
      powerClass: 'pps5a',
      targetsC: EXECUTION_ORDER,
      candidateProfile: { op: 'preview', bank: 'pps5a', profile: candidate.profile },
    })
  }

  const identity = {
    runId: status?.runId ?? undefined,
    candidateId: status?.candidateId ?? undefined,
    candidateHash: status?.candidateHash ?? undefined,
    powerClass: status?.powerClass ?? undefined,
  }
  const canSeal = terminal === 'candidate_ready' && recording?.complete && !status?.sealed
  const canSave =
    terminal === 'candidate_ready' &&
    recording?.complete &&
    status?.sealed &&
    status.previewReceipt != null &&
    !status.saveConfirmed

  return (
    <section className="firmware-tuning" aria-label="固件热控调优">
      <header className="firmware-tuning__header">
        <div>
          <h2>固件热控调优</h2>
          <p>PPS 5A · {status?.runId ?? '未运行'}</p>
        </div>
        <span
          className={`firmware-tuning__outcome firmware-tuning__outcome--${terminal ?? 'idle'}`}
        >
          {terminalLabel(terminal)}
        </span>
      </header>

      <div className="firmware-tuning__toolbar">
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
          {candidate?.name ?? '未选择候选'}
        </span>
        <Button
          type="button"
          disabled={!supported || !candidate || armed || terminal === 'candidate_ready' || pending}
          onClick={start}
        >
          {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
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
      </div>

      {!supported ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 当前设备固件不支持调优事件协议。
        </p>
      ) : null}
      {recording?.gap ? (
        <p className="firmware-tuning__warning">
          <ShieldAlert aria-hidden="true" /> 事件缺口：{recording.gapReason}
        </p>
      ) : null}
      {error ? <p className="firmware-tuning__warning">{error}</p> : null}

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
        <Fact label="审查" value={status?.reviewDisposition ?? '—'} />
        <Fact
          label="追踪"
          value={recording?.complete ? '完整' : recording?.gap ? '缺口' : '收集中'}
        />
      </div>

      <div className="firmware-tuning__detail">
        <section>
          <h3>候选与保存</h3>
          <dl>
            <dt>Candidate</dt>
            <dd>{status?.candidateId ?? '—'}</dd>
            <dt>Hash</dt>
            <dd>{status?.candidateHash ?? '—'}</dd>
            <dt>RAM preview</dt>
            <dd>{status?.previewReceipt ? '已核对' : '未核对'}</dd>
            <dt>EEPROM</dt>
            <dd>
              {status?.saveReceipt?.readbackVerified
                ? `已读回 · revision ${status.saveReceipt.bankRevision}`
                : '未保存'}
            </dd>
          </dl>
        </section>
        <section>
          <h3>设备事件</h3>
          <div className="firmware-tuning__event-scroll">
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
          </div>
        </section>
      </div>
    </section>
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
    throw new Error('候选必须覆盖完整九目标 PPS 5A 范围。')
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

function messageOf(error: unknown) {
  return error instanceof Error ? error.message : '设备操作失败。'
}
