import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import type { ThermalTuningRunSnapshot } from '../contracts'
import {
  buildThermalTuningBundle,
  thermalTuningRunStorageKey,
  thermalTuningTraceHealth,
} from '../thermal-tuning-recorder'
import {
  applyMockThermalTuningCommand,
  createDefaultThermalTuningSnapshot,
  ThermalTuningRunCard,
} from './thermal-tuning-card'

describe('thermal tuning calibration surface', () => {
  it('offers only explicit PPS classes and no source controls', () => {
    const markup = renderToStaticMarkup(
      createElement(ThermalTuningRunCard, {
        snapshot: createDefaultThermalTuningSnapshot(),
        onCommand: () => undefined,
      })
    )
    expect(markup).toContain('PPS 3A')
    expect(markup).toContain('PPS 5A')
    expect(markup).not.toContain('source')
    expect(markup).not.toContain('VBUS')
    expect(markup).not.toContain('确认 trace')
    expect(markup).not.toContain('封存审查')
  })

  it('keeps mock lifecycle explicit and requires review before save', () => {
    const idle = createDefaultThermalTuningSnapshot()
    const running = applyMockThermalTuningCommand(idle, { op: 'start', powerClass: 'pps5a' })
    expect(running.run.state).toBe('running')
    expect(running.run.powerClass).toBe('pps5a')
    expect(running.run.candidate.promotionState).toBe('awaiting_review')
    const markup = renderToStaticMarkup(
      createElement(ThermalTuningRunCard, { snapshot: running, onCommand: () => undefined })
    )
    expect(markup).toContain('保存候选')
    expect(markup).toContain('disabled')
  })

  it('does not offer review sealing for a non-promotable terminal run', () => {
    const canceled = applyMockThermalTuningCommand(
      applyMockThermalTuningCommand(createDefaultThermalTuningSnapshot(), {
        op: 'start',
        powerClass: 'pps3a',
      }),
      { op: 'cancel', runId: 'ignored' }
    )
    const markup = renderToStaticMarkup(
      createElement(ThermalTuningRunCard, { snapshot: canceled, onCommand: () => undefined })
    )
    expect(markup).not.toContain('封存审查')
  })

  it('marks trace gaps as review incomplete', () => {
    const snapshot = createDefaultThermalTuningSnapshot()
    const withGap = {
      ...snapshot,
      page: {
        ...snapshot.page,
        emittedThrough: 3,
        events: [
          { sequence: 1, elapsedMs: 0, kind: 'sample' as const },
          { sequence: 3, elapsedMs: 2, kind: 'decision' as const },
        ],
      },
    }
    expect(thermalTuningTraceHealth(withGap).reviewIncomplete).toBe(true)
  })

  it('accepts sequence zero as the first trace event', () => {
    const snapshot = createDefaultThermalTuningSnapshot()
    const firstEvent = {
      ...snapshot,
      page: {
        ...snapshot.page,
        earliestSequence: 0,
        emittedThrough: 0,
        events: [{ sequence: 0, elapsedMs: 0, kind: 'sample' as const }],
      },
    }
    expect(thermalTuningTraceHealth(firstEvent).reviewIncomplete).toBe(false)
    expect(thermalTuningTraceHealth(firstEvent).expectedNextSequence).toBe(1)
    expect(thermalTuningTraceHealth(firstEvent, 0).reviewIncomplete).toBe(false)
  })

  it('exports the five-file thermal-tuning-v2 bundle', () => {
    const files = buildThermalTuningBundle('fp-1', createDefaultThermalTuningSnapshot())
    expect(Object.keys(files).sort()).toEqual([
      'decision-ledger.ndjson',
      'index.html',
      'run.bundle.json',
      'samples.ndjson',
      'thermal-profile.candidate.json',
    ])
    expect(files['run.bundle.json']).toContain('thermal-tuning-v2')
    expect(thermalTuningRunStorageKey('fp-1', 'run-1')).toBe('fp-1:run-1')
  })

  it('archives the actual PID phase and decodes the complete firmware candidate point', () => {
    const idle = createDefaultThermalTuningSnapshot()
    const snapshot: ThermalTuningRunSnapshot = {
      ...idle,
      run: {
        ...idle.run,
        runId: 'run-phase-audit',
        state: 'terminal',
        powerClass: 'pps5a',
        terminalDisposition: 'completed',
      },
      page: {
        ...idle.page,
        emittedThrough: 3,
        events: [
          {
            sequence: 0,
            elapsedMs: 0,
            kind: 'candidate_trial',
            eventReason: 'started',
            targetC: 60,
            trialIndex: 0,
            candidateHash: 'candidate-60',
            canonicalCandidatePointHex:
              '3c000100020003000400050006000700080009000a000b000c000d000e000f001000110012001300',
          },
          {
            sequence: 1,
            elapsedMs: 500,
            kind: 'sample',
            phase: 'scout',
            heaterPhase: 'warmup',
            targetC: 60,
            trialIndex: 0,
            candidateHash: 'candidate-60',
            temperatureCentiC: 4500,
            heaterOutputPermille: 1000,
            measurementValid: true,
          },
          {
            sequence: 2,
            elapsedMs: 1000,
            kind: 'candidate_trial',
            eventReason: 'completed',
            targetC: 60,
            trialIndex: 0,
            candidateHash: 'candidate-60',
            canonicalCandidatePointHex:
              '3c000100020003000400050006000700080009000a000b000c000d000e000f001000110012001300',
            trialStartSequence: 0,
            trialEndSequence: 2,
            trialStartElapsedMs: 0,
            trialEndElapsedMs: 1000,
            gates: 15,
          },
          {
            sequence: 3,
            elapsedMs: 1000,
            kind: 'decision',
            targetC: 60,
            candidateHash: 'candidate-60',
            disposition: 'accepted',
            gates: 15,
          },
        ],
      },
    }

    const files = buildThermalTuningBundle('fp-1', snapshot)
    expect(files['index.html']).toContain('"heaterPhase":"warmup"')
    expect(files['index.html']).toContain('"evidenceValid":true')
    expect(files['index.html']).toContain('"warmupReenterCentiC":3')
    expect(files['index.html']).toContain('"approachDampingExponentPermille":6')
    expect(files['index.html']).toContain('"holdLeadTicks":19')
  })
})
