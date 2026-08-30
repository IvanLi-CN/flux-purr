import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CircleStop,
  Download,
  Eye,
  Save,
  ShieldCheck,
  Trash2,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import type {
  ThermalTuningPowerClass,
  ThermalTuningRunRequest,
  ThermalTuningRunSnapshot,
} from '../contracts'
import {
  downloadThermalTuningBundle,
  persistThermalTuningSnapshot,
  thermalTuningTraceHealth,
} from '../thermal-tuning-recorder'
import { verifyThermalTuningCandidate } from '../thermal-tuning-wasm'

const TARGET_ORDER = [60, 80, 100, 120, 140, 160, 180, 220, 240]

export interface ThermalTuningRunCardProps {
  deviceId?: string
  snapshot: ThermalTuningRunSnapshot
  unsupported?: boolean
  disabled?: boolean
  onCommand: (
    request: Omit<ThermalTuningRunRequest, 'leaseId'>
  ) => Promise<ThermalTuningRunSnapshot | undefined> | ThermalTuningRunSnapshot | undefined
}

const powerClassLabel: Record<ThermalTuningPowerClass, string> = {
  pps3a: 'PPS 3A · 65W 级',
  pps5a: 'PPS 5A · 100W 级',
}

export function createDefaultThermalTuningSnapshot(): ThermalTuningRunSnapshot {
  return {
    schema: 'thermal_tuning_run_v1',
    run: {
      runId: 'mock-tuning-001',
      state: 'idle',
      powerClass: null,
      phase: 'idle',
      currentTargetC: null,
      targetProgress: { acceptedC: [], failedC: [], skippedC: [] },
      terminalDisposition: null,
      eligibility: {
        ready: true,
        reasons: [],
        activeOwner: null,
      },
      review: {
        state: 'not_applicable',
        reason: null,
        acknowledgedThrough: null,
        terminalSequence: null,
        traceDigest: null,
      },
      candidate: {
        candidateId: null,
        candidateHash: null,
        powerClass: null,
        promotionState: 'unavailable',
      },
      journal: { lastRunId: null, lastDisposition: null },
    },
    page: {
      earliestSequence: 0,
      emittedThrough: null,
      nextAfterSequence: 0,
      acknowledgedThrough: null,
      digestThroughPage: null,
      events: [],
    },
  }
}

export function applyMockThermalTuningCommand(
  snapshot: ThermalTuningRunSnapshot,
  request: Omit<ThermalTuningRunRequest, 'leaseId'>
): ThermalTuningRunSnapshot {
  if (request.op === 'start') {
    const powerClass = request.powerClass ?? 'pps3a'
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        state: 'running',
        runId: `mock-tuning-${Date.now()}`,
        powerClass,
        phase: 'scout',
        eligibility: { ...snapshot.run.eligibility, ready: true, reasons: [] },
        review: { ...snapshot.run.review, state: 'recording', reason: null },
        candidate: {
          ...snapshot.run.candidate,
          powerClass,
          promotionState: 'awaiting_review',
        },
      },
      page: { ...snapshot.page, events: [], nextAfterSequence: 1, emittedThrough: 0 },
    }
  }
  if (request.op === 'cancel') {
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        state: 'terminal',
        phase: 'terminal',
        terminalDisposition: 'cancelled',
        review: { ...snapshot.run.review, state: 'incomplete', reason: 'cancelled' },
      },
    }
  }
  if (request.op === 'ack_trace') {
    return {
      ...snapshot,
      page: {
        ...snapshot.page,
        acknowledgedThrough: Math.max(
          snapshot.page.acknowledgedThrough ?? 0,
          request.throughSequence ?? 0
        ),
        digestThroughPage: request.traceDigest ?? snapshot.page.digestThroughPage,
      },
      run: {
        ...snapshot.run,
        review: {
          ...snapshot.run.review,
          acknowledgedThrough: Math.max(
            snapshot.run.review.acknowledgedThrough ?? 0,
            request.throughSequence ?? 0
          ),
        },
      },
    }
  }
  if (request.op === 'seal_review') {
    const health = thermalTuningTraceHealth(snapshot)
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        review: {
          ...snapshot.run.review,
          state: health.reviewIncomplete ? 'incomplete' : 'complete',
          reason: health.reviewIncomplete ? 'trace_gap' : null,
        },
        candidate: {
          ...snapshot.run.candidate,
          promotionState: health.reviewIncomplete ? 'unavailable' : 'ready',
        },
      },
    }
  }
  if (request.op === 'preview') {
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        candidate: { ...snapshot.run.candidate, promotionState: 'previewed' },
      },
    }
  }
  if (request.op === 'discard_preview') {
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        candidate: { ...snapshot.run.candidate, promotionState: 'ready' },
      },
    }
  }
  if (request.op === 'save') {
    return {
      ...snapshot,
      run: {
        ...snapshot.run,
        candidate: { ...snapshot.run.candidate, promotionState: 'saved' },
      },
    }
  }
  return snapshot
}

function runLabel(snapshot: ThermalTuningRunSnapshot) {
  if (snapshot.run.state === 'running') return '运行中'
  if (snapshot.run.state === 'terminal') return snapshot.run.terminalDisposition ?? '已结束'
  return '待开始'
}

function phaseLabel(snapshot: ThermalTuningRunSnapshot) {
  const labels: Record<ThermalTuningRunSnapshot['run']['phase'], string> = {
    idle: '待开始',
    cooldown_wait: '冷却至基线',
    scout: '目标扫描',
    retune: '参数重整',
    hold_confirm: '稳定确认',
    terminal: '完成或安全收口',
  }
  return labels[snapshot.run.phase]
}

export function ThermalTuningRunCard({
  deviceId = 'device',
  snapshot,
  unsupported = false,
  disabled = false,
  onCommand,
}: ThermalTuningRunCardProps) {
  const [powerClass, setPowerClass] = useState<ThermalTuningPowerClass>('pps3a')
  const [pending, setPending] = useState<string | null>(null)
  const [confirmAction, setConfirmAction] = useState<'start' | 'cancel' | null>(null)
  const [confirmSave, setConfirmSave] = useState(false)
  const [wasmVerification, setWasmVerification] = useState<
    'idle' | 'checking' | 'valid' | 'invalid' | 'unavailable'
  >('idle')
  const health = useMemo(() => thermalTuningTraceHealth(snapshot), [snapshot])
  const canStart = !disabled && snapshot.run.state !== 'running' && snapshot.run.eligibility.ready
  const reviewReady =
    snapshot.run.review.state === 'complete' &&
    snapshot.run.candidate.promotionState === 'ready' &&
    !health.reviewIncomplete
  const candidateHash = snapshot.run.candidate.candidateHash
  const candidateProfileHex = snapshot.run.candidate.canonicalProfileHex
  const candidatePowerClass = snapshot.run.candidate.powerClass
  const traceAckThrough = snapshot.page.events.at(-1)?.sequence
  const hasUnacknowledgedTrace =
    traceAckThrough != null &&
    snapshot.page.digestThroughPage != null &&
    (snapshot.page.acknowledgedThrough == null ||
      traceAckThrough > snapshot.page.acknowledgedThrough)
  const canSealReview =
    pending == null &&
    snapshot.run.state === 'terminal' &&
    snapshot.run.terminalDisposition === 'completed' &&
    snapshot.run.review.state === 'awaiting_seal' &&
    !health.reviewIncomplete

  useEffect(() => {
    let cancelled = false
    if (!candidateHash || !candidatePowerClass) {
      setWasmVerification('idle')
      return () => {
        cancelled = true
      }
    }
    if (!candidateProfileHex) {
      setWasmVerification('unavailable')
      return () => {
        cancelled = true
      }
    }
    setWasmVerification('checking')
    void verifyThermalTuningCandidate(candidatePowerClass, candidateProfileHex, candidateHash).then(
      (valid) => {
        if (!cancelled)
          setWasmVerification(valid == null ? 'unavailable' : valid ? 'valid' : 'invalid')
      }
    )
    return () => {
      cancelled = true
    }
  }, [candidateHash, candidatePowerClass, candidateProfileHex])

  const command = async (request: Omit<ThermalTuningRunRequest, 'leaseId'>) => {
    if (pending) return
    setPending(request.op)
    try {
      const next = await onCommand(request)
      if (next) await persistThermalTuningSnapshot(deviceId, next)
    } finally {
      setPending(null)
    }
  }

  if (unsupported) {
    return (
      <article
        className="thermal-tuning-card thermal-tuning-card--unsupported"
        aria-label="热控调优"
      >
        <div className="thermal-tuning-card__empty">
          <AlertTriangle aria-hidden="true" />
          <div>
            <h2>固件不支持热控调优</h2>
            <p>设备未发布 thermal_tuning_run_v1 capability，当前页面不会启用备用算法。</p>
          </div>
        </div>
      </article>
    )
  }

  return (
    <article className="thermal-tuning-card" aria-label="热控调优工作面">
      <header className="thermal-tuning-card__header">
        <div>
          <p className="thermal-tuning-card__eyebrow">FIRMWARE AUTHORITY · NINE TARGETS</p>
          <h2>热控调优</h2>
          <p className="thermal-tuning-card__lede">
            核心调优在设备内运行，主机只负责记录、审查与导出。
          </p>
        </div>
        <div
          className={`thermal-tuning-card__status thermal-tuning-card__status--${snapshot.run.state}`}
        >
          <span aria-hidden="true" />
          {runLabel(snapshot)}
        </div>
      </header>

      <section className="thermal-tuning-card__eligibility" aria-label="调优前置条件">
        <div className="thermal-tuning-card__section-title">
          <ShieldCheck aria-hidden="true" />
          <h3>开始检查</h3>
          <span>{snapshot.run.eligibility.ready ? '允许开始' : '需要处理'}</span>
        </div>
        {snapshot.run.eligibility.ready ? (
          <p className="thermal-tuning-card__ok">热模型、曲线覆盖、PPS 合同与安全状态已满足。</p>
        ) : (
          <ul>
            {snapshot.run.eligibility.reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        )}
      </section>

      <section className="thermal-tuning-card__controls" aria-label="调优参数">
        <div className="thermal-tuning-card__section-title">
          <Activity aria-hidden="true" />
          <h3>PPS 功率级别</h3>
          <span>仅支持 PPS</span>
        </div>
        <fieldset className="thermal-tuning-card__segmented">
          <legend>PPS 功率级别</legend>
          {(Object.keys(powerClassLabel) as ThermalTuningPowerClass[]).map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={powerClass === value}
              disabled={disabled || snapshot.run.state === 'running'}
              onClick={() => setPowerClass(value)}
            >
              {powerClassLabel[value]}
            </button>
          ))}
        </fieldset>
      </section>

      <section className="thermal-tuning-card__progress" aria-label="调优进度">
        <div className="thermal-tuning-card__section-title">
          <CheckCircle2 aria-hidden="true" />
          <h3>{phaseLabel(snapshot)}</h3>
          <span>
            {snapshot.run.currentTargetC ? `${snapshot.run.currentTargetC}℃` : '九点固定顺序'}
          </span>
        </div>
        <ul className="thermal-tuning-card__targets" aria-label="九个目标温度">
          {TARGET_ORDER.map((target) => {
            const accepted = snapshot.run.targetProgress.acceptedC.includes(target)
            const failed = snapshot.run.targetProgress.failedC.includes(target)
            const skipped = snapshot.run.targetProgress.skippedC.includes(target)
            return (
              <li
                key={target}
                className={
                  accepted ? 'is-accepted' : failed ? 'is-failed' : skipped ? 'is-skipped' : ''
                }
              >
                {target}
              </li>
            )
          })}
        </ul>
      </section>

      <section className="thermal-tuning-card__trace" aria-label="主机 trace 记录健康度">
        <div className="thermal-tuning-card__section-title">
          <Activity aria-hidden="true" />
          <h3>Trace 健康度</h3>
          <span className={health.reviewIncomplete ? 'is-warning' : 'is-ok'}>
            {health.reviewIncomplete ? 'review incomplete' : '连续'}
          </span>
        </div>
        <dl>
          <div>
            <dt>设备已发出</dt>
            <dd>{snapshot.page.emittedThrough ?? 0}</dd>
          </div>
          <div>
            <dt>主机已确认</dt>
            <dd>{snapshot.page.acknowledgedThrough ?? 0}</dd>
          </div>
          <div>
            <dt>当前 digest</dt>
            <dd>
              {snapshot.page.digestThroughPage
                ? `${snapshot.page.digestThroughPage.slice(0, 12)}…`
                : '待生成'}
            </dd>
          </div>
        </dl>
        {health.reviewIncomplete ? (
          <p className="thermal-tuning-card__warning">
            检测到 trace 缺口，不能 preview 或保存候选。
          </p>
        ) : null}
      </section>

      <section className="thermal-tuning-card__candidate" aria-label="候选审查">
        <div className="thermal-tuning-card__section-title">
          <Eye aria-hidden="true" />
          <h3>候选审查</h3>
          <span>{snapshot.run.candidate.promotionState}</span>
        </div>
        <p>
          {snapshot.run.candidate.candidateId
            ? `${snapshot.run.candidate.candidateId} · ${(snapshot.run.candidate.candidateHash ?? '').slice(0, 16)}…`
            : '完成九点运行并封存 trace 后生成候选。'}
        </p>
        {wasmVerification !== 'idle' ? (
          <p className="thermal-tuning-card__ok" aria-live="polite">
            Wasm 校验：
            {wasmVerification === 'checking'
              ? '进行中'
              : wasmVerification === 'valid'
                ? '通过'
                : wasmVerification === 'invalid'
                  ? '不通过'
                  : '不可用'}
          </p>
        ) : null}
      </section>

      <footer className="thermal-tuning-card__actions" aria-live="polite">
        {snapshot.run.state === 'running' ? (
          confirmAction === 'cancel' ? (
            <fieldset className="thermal-tuning-card__confirm">
              <legend>确认停止调优</legend>
              <span>停止当前调优？设备会安全收口。</span>
              <Button
                type="button"
                size="sm"
                disabled={pending != null}
                onClick={() => {
                  setConfirmAction(null)
                  void command({ op: 'cancel', runId: snapshot.run.runId })
                }}
              >
                确认停止
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setConfirmAction(null)}
              >
                取消
              </Button>
            </fieldset>
          ) : (
            <Button
              type="button"
              variant="outline"
              disabled={pending != null}
              onClick={() => setConfirmAction('cancel')}
            >
              <CircleStop aria-hidden="true" /> {pending === 'cancel' ? '停止中…' : '停止调优'}
            </Button>
          )
        ) : confirmAction === 'start' ? (
          <fieldset className="thermal-tuning-card__confirm">
            <legend>确认开始调优</legend>
            <span>开始九点 {powerClassLabel[powerClass]} 调优？</span>
            <Button
              type="button"
              size="sm"
              disabled={pending != null}
              onClick={() => {
                setConfirmAction(null)
                void command({ op: 'start', powerClass })
              }}
            >
              确认开始
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setConfirmAction(null)}>
              取消
            </Button>
          </fieldset>
        ) : (
          <Button
            type="button"
            disabled={!canStart || pending != null}
            onClick={() => setConfirmAction('start')}
          >
            <Activity aria-hidden="true" />{' '}
            {pending === 'start' ? '启动中…' : `开始 ${powerClassLabel[powerClass]}`}
          </Button>
        )}
        <Button
          type="button"
          variant="secondary"
          disabled={!canSealReview}
          onClick={() => void command({ op: 'seal_review', runId: snapshot.run.runId })}
        >
          <CheckCircle2 aria-hidden="true" /> 封存审查
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={pending != null || health.reviewIncomplete || !hasUnacknowledgedTrace}
          onClick={() =>
            void command({
              op: 'ack_trace',
              runId: snapshot.run.runId,
              afterSequence: traceAckThrough,
              throughSequence: traceAckThrough,
              traceDigest: snapshot.page.digestThroughPage ?? undefined,
            })
          }
        >
          <CheckCircle2 aria-hidden="true" /> 确认 trace
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={pending != null || !reviewReady}
          onClick={() =>
            void command({
              op: 'preview',
              runId: snapshot.run.runId,
              candidateId: snapshot.run.candidate.candidateId ?? undefined,
              candidateHash: snapshot.run.candidate.candidateHash ?? undefined,
              powerClass: candidatePowerClass ?? undefined,
            })
          }
        >
          <Eye aria-hidden="true" /> 预览候选
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={pending != null || snapshot.run.candidate.promotionState !== 'previewed'}
          onClick={() =>
            void command({
              op: 'discard_preview',
              runId: snapshot.run.runId,
              candidateId: snapshot.run.candidate.candidateId ?? undefined,
              candidateHash: snapshot.run.candidate.candidateHash ?? undefined,
              powerClass: candidatePowerClass ?? undefined,
            })
          }
        >
          <Trash2 aria-hidden="true" /> 丢弃预览
        </Button>
        {confirmSave ? (
          <fieldset className="thermal-tuning-card__confirm">
            <legend>确认保存候选</legend>
            <span>保存这份已审查候选？</span>
            <Button
              type="button"
              size="sm"
              disabled={pending != null}
              onClick={() => {
                setConfirmSave(false)
                void command({
                  op: 'save',
                  runId: snapshot.run.runId,
                  candidateId: snapshot.run.candidate.candidateId ?? undefined,
                  candidateHash: snapshot.run.candidate.candidateHash ?? undefined,
                  powerClass: candidatePowerClass ?? undefined,
                })
              }}
            >
              确认保存
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setConfirmSave(false)}>
              取消
            </Button>
          </fieldset>
        ) : (
          <Button
            type="button"
            variant="outline"
            disabled={pending != null || snapshot.run.candidate.promotionState !== 'previewed'}
            onClick={() => setConfirmSave(true)}
          >
            <Save aria-hidden="true" /> 保存候选
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          disabled={snapshot.run.state === 'idle'}
          onClick={() => downloadThermalTuningBundle(deviceId, snapshot)}
        >
          <Download aria-hidden="true" /> 导出 bundle
        </Button>
      </footer>
    </article>
  )
}
