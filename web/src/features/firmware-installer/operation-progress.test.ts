import { describe, expect, it } from 'vitest'

import {
  parseFirmwareOperationProgressEvent,
  progressForFirmwareEvent,
  stageIndexForFirmwareEvent,
} from './operation-progress'

const executionEvent = {
  schemaVersion: 1 as const,
  operationId: 'firmware-operation-1',
  phase: 'execution' as const,
  operation: 'update' as const,
  artifactId: 'sha256:bundle',
  sequence: 3,
  event: 'stage_progress' as const,
  stage: 'write_segments',
  completedUnits: 50,
  totalUnits: 100,
  unit: 'bytes' as const,
}

describe('firmware operation progress events', () => {
  it('parses the additive devd firmware event envelope', () => {
    expect(
      parseFirmwareOperationProgressEvent(
        JSON.stringify({
          id: 'event-1',
          timestamp: '2026-08-19T10:00:00Z',
          deviceId: 'device-1',
          kind: 'firmware_operation',
          message: 'write progress',
          payload: executionEvent,
        })
      )
    ).toMatchObject({ ...executionEvent, timestamp: '2026-08-19T10:00:00Z' })
  })

  it('keeps preflight and execution progress in independent scales', () => {
    expect(
      progressForFirmwareEvent({
        ...executionEvent,
        phase: 'preflight',
        event: 'stage_completed',
        stage: 'preflight',
      })
    ).toBe(100)
    expect(progressForFirmwareEvent(executionEvent)).toBeGreaterThan(4)
    expect(progressForFirmwareEvent(executionEvent)).toBeLessThan(62)
  })

  it('maps authorization as the first execution stage', () => {
    expect(
      stageIndexForFirmwareEvent({
        ...executionEvent,
        event: 'stage_started',
        stage: 'authorization',
      })
    ).toBe(0)
  })

  it('rejects unrelated or malformed device events', () => {
    expect(
      parseFirmwareOperationProgressEvent(
        JSON.stringify({ kind: 'runtime', payload: executionEvent })
      )
    ).toBeNull()
    expect(parseFirmwareOperationProgressEvent('{')).toBeNull()
  })
})
