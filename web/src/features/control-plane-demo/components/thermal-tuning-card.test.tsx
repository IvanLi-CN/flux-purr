import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
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
})
