import {
  executionProgress,
  executionStages,
  type FirmwareExecutionStage,
  type FirmwarePreflightStage,
  preflightProgress,
  preflightStages,
} from './state-machine'
import type { FirmwareOperation } from './types'

export type FirmwareProgressPhase = 'preflight' | 'execution'
export type FirmwareProgressEventName =
  | 'operation_started'
  | 'stage_started'
  | 'stage_progress'
  | 'stage_completed'
  | 'stage_failed'
  | 'operation_completed'

export interface FirmwareOperationProgressEvent {
  schemaVersion: 1
  operationId: string
  phase: FirmwareProgressPhase
  operation: FirmwareOperation
  artifactId: string
  sequence: number
  event: FirmwareProgressEventName
  stage?: string
  completedUnits?: number
  totalUnits?: number
  unit?: 'bytes' | 'segments'
  outcome?: string
  code?: string
  timestamp?: string
}

interface DevdFirmwareEventEnvelope {
  timestamp?: unknown
  kind?: unknown
  payload?: unknown
}

export function parseFirmwareOperationProgressEvent(
  data: string
): FirmwareOperationProgressEvent | null {
  try {
    const envelope = JSON.parse(data) as DevdFirmwareEventEnvelope
    if (envelope.kind !== 'firmware_operation' || !isRecord(envelope.payload)) return null
    const payload = envelope.payload
    if (
      payload.schemaVersion !== 1 ||
      typeof payload.operationId !== 'string' ||
      (payload.phase !== 'preflight' && payload.phase !== 'execution') ||
      (payload.operation !== 'update' && payload.operation !== 'install_recovery') ||
      typeof payload.artifactId !== 'string' ||
      typeof payload.sequence !== 'number' ||
      !isProgressEventName(payload.event)
    ) {
      return null
    }
    return {
      ...(payload as unknown as FirmwareOperationProgressEvent),
      timestamp: typeof envelope.timestamp === 'string' ? envelope.timestamp : undefined,
    }
  } catch {
    return null
  }
}

export function progressForFirmwareEvent(event: FirmwareOperationProgressEvent): number | null {
  if (!event.stage) return null
  const stageFraction = eventStageFraction(event)
  if (event.phase === 'preflight') {
    if (!preflightStages().includes(event.stage as FirmwarePreflightStage)) return null
    return preflightProgress(event.stage as FirmwarePreflightStage, stageFraction)
  }
  if (!executionStages(event.operation).includes(event.stage as FirmwareExecutionStage)) return null
  return executionProgress(event.operation, event.stage as FirmwareExecutionStage, stageFraction)
}

export function stageIndexForFirmwareEvent(event: FirmwareOperationProgressEvent): number | null {
  if (!event.stage) return null
  const stages = event.phase === 'preflight' ? preflightStages() : executionStages(event.operation)
  const index = (stages as string[]).indexOf(event.stage)
  return index >= 0 ? index : null
}

function eventStageFraction(event: FirmwareOperationProgressEvent) {
  if (event.event === 'stage_completed') return 1
  if (
    event.event === 'stage_progress' &&
    typeof event.completedUnits === 'number' &&
    typeof event.totalUnits === 'number' &&
    event.totalUnits > 0
  ) {
    return event.completedUnits / event.totalUnits
  }
  return 0
}

function isProgressEventName(value: unknown): value is FirmwareProgressEventName {
  return (
    value === 'operation_started' ||
    value === 'stage_started' ||
    value === 'stage_progress' ||
    value === 'stage_completed' ||
    value === 'stage_failed' ||
    value === 'operation_completed'
  )
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
