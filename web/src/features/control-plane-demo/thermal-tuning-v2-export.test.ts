import { strFromU8, unzipSync } from 'fflate'
import { describe, expect, it } from 'vitest'
import type { ThermalTuningEvent, ThermalTuningStatus } from './contracts'
import { digestEventChain } from './thermal-tuning-device-digest'
import type { ThermalTuningRecording } from './thermal-tuning-recorder'
import { exportThermalTuningV2, timingGateValid } from './thermal-tuning-v2-export'

const TARGETS = [60, 80, 100, 120, 140, 160, 180, 220, 240]
const EXECUTION_ORDER = [60, 240, 140, 100, 80, 120, 180, 160, 220]
const CANDIDATE_HASH = `sha256:${'a'.repeat(64)}`

function event(
  sequence: number,
  kind: ThermalTuningEvent['kind'],
  targetC: number
): ThermalTuningEvent {
  return {
    sequence,
    elapsedMs: sequence * 1_000,
    kind,
    phase: kind === 'safety' ? 'failed' : 'hold',
    targetC,
    trialIndex: 0,
    sample: null,
    reason: null,
    trial: null,
    decision: null,
  }
}

function recording(
  events: ThermalTuningEvent[],
  overrides: Partial<ThermalTuningStatus> = {}
): ThermalTuningRecording {
  const lastSequence = events.at(-1)?.sequence ?? null
  const trace = {
    firstSequence: 0,
    lastSequence,
    complete: true,
    gap: false,
    digest: '',
  }
  const status: ThermalTuningStatus = {
    phase: 'failed',
    trialPurpose: 'scout',
    terminalDisposition: 'failed',
    reviewDisposition: 'review_incomplete',
    powerClass: 'pps5a',
    runId: 'run-web-1',
    candidateId: 'candidate-web-1',
    candidateHash: CANDIDATE_HASH,
    targetsC: EXECUTION_ORDER,
    targetIndex: 0,
    armed: false,
    sealed: false,
    saveConfirmed: false,
    candidateProfile: null,
    previewReceipt: null,
    trace,
    events: { events: [], nextSequence: null, trace },
    saveReceipt: null,
    ...overrides,
  }
  return {
    runId: status.runId,
    status,
    events,
    acknowledgedThrough: lastSequence,
    confirmedThrough: lastSequence,
    gap: false,
    gapReason: null,
    complete: true,
  }
}

describe('Web thermal-tuning-v2 export', () => {
  it('requires bound floor evidence and a complete stable window for extended timing', () => {
    const trial = {
      targetC: 60,
      canonicalCandidate: { point: { approachFloorPowerPermille: 100, holdPowerPermille: 100 } },
      fullSpeedToStable: { settleTimeMs: 12_000, limitMs: 30_000 },
      timing: {
        policyVersion: 1,
        originalLimitMs: 10_000,
        extensionLimitMs: 30_000,
        allowedApproachFloorPermille: 100,
        effectiveApproachFloorPermille: 100,
        floorObservedAtMs: 2_000,
        extendedAtMs: 11_000,
        warmupExitedAtMs: 1_000,
        stableWindowStartedAtMs: 13_000,
        stableWindowVerifiedAtMs: 23_000,
      },
    }
    expect(timingGateValid(trial)).toBe(true)
    for (const change of [
      { floorObservedAtMs: null },
      { extendedAtMs: 10_000 },
      { stableWindowVerifiedAtMs: 22_999 },
      { extensionLimitMs: 180_000 },
      { effectiveApproachFloorPermille: 160 },
      { floorObservedAtMs: 2_000.5 },
      { floorObservedAtMs: -1 },
      { stableWindowVerifiedAtMs: Number.MAX_SAFE_INTEGER + 1 },
    ]) {
      expect(timingGateValid({ ...trial, timing: { ...trial.timing, ...change } })).toBe(false)
    }
    expect(
      timingGateValid({ ...trial, timing: { availability: 'mismatched_device_timing_evidence' } })
    ).toBe(false)
  })
  it('exports all five standard files for a failed device run', async () => {
    const events = [event(0, 'safety', 60)]
    events[0].reason = 'sensor_fault'
    const captured = recording(events, {
      ppsControlPolicyVersion: 'hold-pps-governor-v1',
      ppsVoltageBounds: {
        absoluteMinMv: 5_000,
        absoluteMaxMv: 28_000,
        policyFloorMv: 12_000,
        apdoMinMv: 11_000,
        apdoMaxMv: 21_000,
      },
    })
    captured.status.trace.digest = await digestEventChain(events)
    const exported = await exportThermalTuningV2(captured, 'device-1', 'web_serial')
    const archive = unzipSync(exported.archive)
    expect(Object.keys(archive).sort()).toEqual([
      'decision-ledger.ndjson',
      'index.html',
      'run.bundle.json',
      'samples.ndjson',
      'thermal-profile.candidate.json',
    ])
    const bundle = JSON.parse(strFromU8(archive['run.bundle.json']))
    expect(exported.ownerOutcome).toBe('failed')
    expect(bundle.trace.complete).toBe(true)
    expect(bundle.ownerOutcome).toBe('failed')
    expect(bundle.generatedAt).toEqual(expect.any(Number))
    expect(bundle.generatedAt).toBeGreaterThan(0)
    expect(bundle.deviceTraceVerification.verified).toBe(true)
    expect(bundle.ppsControlPolicyVersion).toBe('hold-pps-governor-v1')
    expect(bundle.ppsVoltageBounds).toMatchObject({
      apdoMaxMv: 21_000,
      availability: 'device_local',
    })
    expect(strFromU8(archive['index.html'])).toContain('const REPORT=')
    expect(strFromU8(archive['index.html'])).not.toContain('__THERMAL_TUNING_V2_DATA__')
    expect(strFromU8(archive['decision-ledger.ndjson'])).toContain('sensor_fault')
    expect(bundle.safetySummary).toMatchObject({
      source: 'device_event',
      reason: 'sensor_fault',
      sequence: 0,
    })
    expect(bundle.reportRuns[0]).toMatchObject({
      executionStatus: 'not_attempted',
      stopReason: 'run_terminated_before_target',
      blockedBy: { targetC: null, reason: 'sensor_fault' },
    })
    expect(strFromU8(archive['index.html'])).toContain('安全终止事实')
    expect(strFromU8(archive['index.html'])).toContain('executionStatus')
    expect(strFromU8(archive['index.html'])).toContain("drawChart('currentChart',samples,")
    const embedded = JSON.parse(
      strFromU8(archive['index.html']).match(/const REPORT=(.*);\n/)?.[1] ?? 'null'
    )
    expect(embedded.ledger).toEqual(events)
    expect(embedded.samples).toEqual([])
    expect(embedded.bundle.safetySummary).toEqual(bundle.safetySummary)
  })

  it('keeps candidate review, seal, and EEPROM save as distinct states', async () => {
    const events = [event(0, 'decision', 60)]
    const captured = recording(events, {
      phase: 'completed',
      terminalDisposition: 'candidate_ready',
      reviewDisposition: 'complete',
    })
    captured.status.trace.digest = await digestEventChain(events)

    const promotion = async (reviewDisposition = 'complete') => {
      const exported = await exportThermalTuningV2(captured, 'device-1', 'web_serial')
      const candidate = JSON.parse(exported.files['thermal-profile.candidate.json'])
      const bundle = JSON.parse(exported.files['run.bundle.json'])
      expect(exported.ownerOutcome).toBe('failed')
      expect(bundle.ownerOutcome).toBe('failed')
      expect(bundle.reviewDisposition).toBe(reviewDisposition)
      return candidate.promotion.state
    }

    expect(await promotion()).toBe('candidate_ready')
    captured.status.sealed = true
    expect(await promotion()).toBe('sealed')
    captured.status.phase = 'completed'
    captured.status.terminalDisposition = 'passed'
    captured.status.saveConfirmed = true
    captured.status.previewReceipt = {
      runId: 'run-web-1',
      candidateId: 'candidate-web-1',
      candidateHash: CANDIDATE_HASH,
      powerClass: 'pps5a',
      ramAppliedHash: CANDIDATE_HASH,
    }
    captured.status.saveReceipt = {
      runId: 'run-web-1',
      candidateId: 'candidate-web-1',
      candidateHash: CANDIDATE_HASH,
      powerClass: 'pps5a',
      bank: 'pps5a',
      bankRevision: 3,
      bankHash: CANDIDATE_HASH,
      atomic: true,
      readbackVerified: true,
    }
    expect(await promotion()).toBe('saved')
    captured.status.saveReceipt.bankHash = `sha256:${'b'.repeat(64)}`
    expect(await promotion('review_incomplete')).toBe('rejected')
  })

  it('qualifies only a nine-target saved run with full receipts and trial evidence', async () => {
    const events: ThermalTuningEvent[] = []
    for (const targetC of TARGETS) {
      const trial = event(events.length, 'candidate_trial', targetC)
      trial.trial = {
        purpose: 'hold_confirm',
        startSequence: trial.sequence,
        endSequence: trial.sequence + 4,
        sampleStartSequence: trial.sequence + 1,
        sampleEndSequence: trial.sequence + 3,
        candidate: {
          candidateId: 'candidate-web-1',
          candidateHash: CANDIDATE_HASH,
          powerClass: 'pps5a',
        },
      }
      events.push(trial)
      for (const [phaseIndex, phase] of (['warmup', 'approach', 'hold'] as const).entries()) {
        const sample = event(events.length, 'sample', targetC)
        sample.phase = phase
        sample.sample = {
          sequence: sample.sequence,
          elapsedMs: sample.elapsedMs,
          targetC,
          trialIndex: 0,
          candidateId: 'candidate-web-1',
          candidateHash: CANDIDATE_HASH,
          temperatureCentiC: targetC * 100 + phaseIndex,
          vinMv: 11_800,
          ppsContract: { voltageMv: 12_000, currentMa: 5_000, kind: 'pps5a', confirmed: true },
          requestedMv: 10_000,
          ppsPowerCeilingMv: 12_000,
          heaterRequestedOutputPermille: phase === 'warmup' ? 1_000 : 420,
          heaterOutputPermille: phase === 'warmup' ? 960 : 410,
          measurementValid: true,
          phase,
        }
        events.push(sample)
      }
      const decision = event(events.length, 'decision', targetC)
      decision.decision = {
        purpose: 'hold_confirm',
        disposition: 'adopted',
        maxOvershootCentiC: 120,
        holdPeakToPeakCentiC: 150,
        settleTimeMs: 8_000,
        hardGateFullSpeedToStableMs: 12_000,
        holdConfirm: true,
      }
      events.push(decision)
    }
    const captured = recording(events, {
      phase: 'completed',
      terminalDisposition: 'passed',
      reviewDisposition: 'complete',
      sealed: true,
      saveConfirmed: true,
      candidateProfile: {
        settings: {},
        points: TARGETS.map((targetTempC) => ({ targetTempC })),
      },
      previewReceipt: {
        runId: 'run-web-1',
        candidateId: 'candidate-web-1',
        candidateHash: CANDIDATE_HASH,
        powerClass: 'pps5a',
        ramAppliedHash: CANDIDATE_HASH,
      },
      saveReceipt: {
        runId: 'run-web-1',
        candidateId: 'candidate-web-1',
        candidateHash: CANDIDATE_HASH,
        powerClass: 'pps5a',
        bank: 'pps5a',
        bankRevision: 3,
        bankHash: CANDIDATE_HASH,
        atomic: true,
        readbackVerified: true,
      },
    })
    captured.status.trace.digest = await digestEventChain(events)
    const exported = await exportThermalTuningV2(captured, 'device-1', 'web_serial')
    const bundle = JSON.parse(exported.files['run.bundle.json'])
    expect(exported.ownerOutcome).toBe('passed')
    expect(bundle.reportRuns).toHaveLength(9)
    expect(
      bundle.reportRuns.every((run: { reviewOutcome: string }) => run.reviewOutcome === 'passed')
    ).toBe(true)
    expect(bundle.deviceTraceVerification.verified).toBe(true)

    const firstTrial = events.find((item) => item.trial != null)?.trial
    const firstDecision = events.find((item) => item.decision != null)?.decision
    if (!firstTrial || !firstDecision) throw new Error('missing qualification fixture')
    firstTrial.purpose = 'scout'
    firstDecision.purpose = 'scout'
    captured.status.trace.digest = await digestEventChain(events)
    const invalid = await exportThermalTuningV2(captured, 'device-1', 'web_serial')
    const invalidBundle = JSON.parse(invalid.files['run.bundle.json'])
    expect(invalidBundle.deviceTraceVerification.verified).toBe(true)
    expect(invalid.ownerOutcome).toBe('failed')
    expect(invalidBundle.reportRuns[0].reviewOutcome).toBe('failed')
    expect(invalidBundle.reportRuns[0].trials[0].disposition).toBe('adopted')
    expect(invalidBundle.reportRuns[0].trials[0].metrics.holdConfirmSeconds).toBeNull()
    firstTrial.purpose = 'hold_confirm'
    firstDecision.purpose = 'hold_confirm'
    captured.status.trace.digest = await digestEventChain(events)

    captured.status.saveReceipt = null
    expect((await exportThermalTuningV2(captured, 'device-1', 'web_serial')).ownerOutcome).toBe(
      'failed'
    )
  })
})
