import {
  type FirmwareOperationProgressEvent,
  parseFirmwareOperationProgressEvent,
} from './operation-progress'
import type { OfficialFirmwareArtifact } from './release-catalog'
import type { FirmwareOperation } from './types'

export interface DevdFirmwareErrorEnvelope {
  error?: {
    code?: string
    message?: string
    retryable?: boolean
  }
  message?: string
}

export function devdFirmwareResponseMessage(
  response: Pick<Response, 'status'>,
  payload: DevdFirmwareErrorEnvelope,
  fallback: string
) {
  return (
    payload.message?.trim() ||
    payload.error?.message?.trim() ||
    payload.error?.code?.trim() ||
    `${fallback} (${response.status}).`
  )
}

function sortArtifactsByPublishedAt(
  artifacts: OfficialFirmwareArtifact[]
): OfficialFirmwareArtifact[] {
  return [...artifacts].sort((left, right) => right.publishedAt.localeCompare(left.publishedAt))
}

function preferredArtifactId(artifacts: OfficialFirmwareArtifact[]): string | null {
  const sorted = sortArtifactsByPublishedAt(artifacts)
  return sorted.find((artifact) => artifact.channel === 'stable')?.id ?? sorted[0]?.id ?? null
}

export function resolveCatalogSelection(
  artifacts: OfficialFirmwareArtifact[],
  selectedArtifactId: string | null,
  selectionIsExplicit: boolean
): OfficialFirmwareArtifact | null {
  const selected = artifacts.find((artifact) => artifact.id === selectedArtifactId) ?? null
  if (selectionIsExplicit && selected) return selected
  const preferredId = preferredArtifactId(artifacts)
  return artifacts.find((artifact) => artifact.id === preferredId) ?? null
}

export interface DevdFirmwareProgressMonitor {
  ready: Promise<void>
  arm: () => void
  bindOperationId: (operationId: string) => void
  close: () => void
}

export function startDevdFirmwareProgressMonitor({
  devdBaseUrl,
  deviceId,
  phase,
  operation,
  artifactId,
  onEvent,
}: {
  devdBaseUrl: string
  deviceId: string
  phase: 'preflight' | 'execution'
  operation: FirmwareOperation
  artifactId: string
  onEvent: (event: FirmwareOperationProgressEvent) => void
}): DevdFirmwareProgressMonitor {
  if (typeof EventSource === 'undefined') {
    return {
      ready: Promise.resolve(),
      arm() {},
      bindOperationId() {},
      close() {},
    }
  }

  const source = new EventSource(
    `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/events`
  )
  let armedAt = Number.POSITIVE_INFINITY
  let capturedOperationId: string | null = null
  let boundOperationId: string | null = null
  const seen = new Set<string>()
  let settleReady: (() => void) | null = null
  const ready = new Promise<void>((resolve) => {
    settleReady = resolve
  })
  const readyTimeout = window.setTimeout(() => settleReady?.(), 1_500)
  const settle = () => {
    window.clearTimeout(readyTimeout)
    settleReady?.()
    settleReady = null
  }
  source.onopen = settle
  source.onerror = settle

  const handleEvent = (message: MessageEvent<string>) => {
    const event = parseFirmwareOperationProgressEvent(message.data)
    if (
      !event ||
      event.phase !== phase ||
      event.operation !== operation ||
      event.artifactId !== artifactId
    ) {
      return
    }
    const eventAt = parseFirmwareEventTimestamp(event.timestamp)
    if (Number.isFinite(eventAt) && eventAt < armedAt) return
    if (boundOperationId && event.operationId !== boundOperationId) return
    if (!capturedOperationId) {
      if (event.event !== 'operation_started') return
      capturedOperationId = event.operationId
    }
    if (event.operationId !== capturedOperationId) return
    const key = `${event.operationId}:${event.sequence}`
    if (seen.has(key)) return
    seen.add(key)
    onEvent(event)
  }
  source.addEventListener('firmware_operation', handleEvent)

  return {
    ready,
    arm() {
      armedAt = Date.now()
    },
    bindOperationId(operationId) {
      boundOperationId = operationId
    },
    close() {
      settle()
      source.removeEventListener('firmware_operation', handleEvent)
      source.close()
    },
  }
}

function parseFirmwareEventTimestamp(timestamp: string | undefined) {
  if (!timestamp) return Number.NaN
  const numeric = Number(timestamp)
  if (Number.isFinite(numeric)) return numeric
  return Date.parse(timestamp)
}
