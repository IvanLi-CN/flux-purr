import {
  ArrowDownToLine,
  ChevronRight,
  Download,
  PackageOpen,
  RotateCcw,
  Server,
  ShieldCheck,
  Usb,
  Zap,
} from 'lucide-react'
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'

import {
  type BrowserSerialPort,
  getBrowserSerial,
  isFluxPurrUsbSerialPort,
  WebSerialControlPlaneClient,
} from '../control-plane-demo/web-serial'

import {
  type BrowserRuntimeVerificationEvent,
  type BrowserWriteProgressEvent,
  connectBrowserLoader,
  disconnectBrowserLoader,
  preflightBrowserLoader,
  verifyBrowserRuntime,
  writeBrowserBundle,
} from './browser-esptool'
import {
  type BrowserPreflightTraceEvent,
  beginBrowserUsbPreflight,
} from './browser-preflight-trace'
import { validateFirmwareBundle } from './bundle'
import {
  type DevdFirmwareErrorEnvelope,
  type DevdFirmwareProgressMonitor,
  devdFirmwareResponseMessage,
  resolveCatalogSelection,
  startDevdFirmwareProgressMonitor,
} from './firmware-workbench-logic'
import {
  type FirmwareOperationProgressEvent,
  progressForFirmwareEvent,
  stageIndexForFirmwareEvent,
} from './operation-progress'
import {
  fetchOfficialBundle,
  fetchOfficialCatalog,
  type OfficialFirmwareArtifact,
} from './release-catalog'
import {
  executionProgress,
  executionStages,
  type FirmwareExecutionStage,
  type FirmwarePreflightStage,
  preflightProgress,
  preflightStages,
} from './state-machine'
import type {
  FirmwareChannel,
  FirmwareOperation,
  FirmwareOutcome,
  FirmwareStage,
  FirmwareTransport,
  ValidatedFirmwareBundle,
} from './types'

export interface FirmwareWorkbenchProps {
  browserAvailable: boolean
  nativeTargets: FirmwareNativeTarget[]
  devdBaseUrl?: string | null
  officialArtifacts?: OfficialFirmwareArtifact[]
  artifactBlocked?: boolean
  executionMode?: FirmwareWorkbenchExecutionMode
  onActivity?: (entry: FirmwareActivityInput) => void
}

export type FirmwareWorkbenchExecutionMode = 'live' | 'mock'

export interface FirmwareActivityInput {
  event: string
  detail: string
  tone: 'info' | 'success' | 'warning' | 'error'
}

export interface FirmwareActivityEntry extends FirmwareActivityInput {
  id: string
  time: string
}

export interface FirmwareNativeTarget {
  id: string
  label: string
  detail: string
  leaseId?: string | null
  updateEligible: boolean
  currentTemperatureC?: number
  heaterEnabled?: boolean
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

export function FirmwareWorkbench({
  browserAvailable,
  nativeTargets,
  devdBaseUrl,
  officialArtifacts,
  artifactBlocked = false,
  executionMode = 'live',
  onActivity,
}: FirmwareWorkbenchProps) {
  const mockExecution = executionMode === 'mock'
  const [devdHealth, setDevdHealth] = useState<'checking' | 'available' | 'unavailable'>(() =>
    devdBaseUrl ? 'checking' : 'unavailable'
  )
  // Default to the non-destructive path; recovery remains an explicit choice because it erases MCU flash.
  const [operation, setOperation] = useState<FirmwareOperation>('update')
  const [transport, setTransport] = useState<FirmwareTransport>('browser')
  const [nativeTargetId, setNativeTargetId] = useState<string | null>(null)
  const [channel, setChannel] = useState<FirmwareChannel>('stable')
  const [includeRc, setIncludeRc] = useState(false)
  const [artifactDialogOpen, setArtifactDialogOpen] = useState(false)
  const [artifactDialogTab, setArtifactDialogTab] = useState<'release' | 'local'>('release')
  const [catalogStatus, setCatalogStatus] = useState<'loading' | 'ready' | 'empty' | 'failed'>(
    () => (officialArtifacts ? (officialArtifacts.length > 0 ? 'ready' : 'empty') : 'loading')
  )
  const [catalogArtifacts, setCatalogArtifacts] = useState<OfficialFirmwareArtifact[]>(
    () => officialArtifacts ?? []
  )
  const [catalogError, setCatalogError] = useState<string | null>(null)
  const [selectedOfficialArtifactId, setSelectedOfficialArtifactId] = useState<string | null>(() =>
    preferredArtifactId(officialArtifacts ?? [])
  )
  const [pendingOfficialArtifactId, setPendingOfficialArtifactId] = useState<string | null>(() =>
    preferredArtifactId(officialArtifacts ?? [])
  )
  const [localBundle, setLocalBundle] = useState<ValidatedFirmwareBundle | null>(null)
  const [localBundleBytes, setLocalBundleBytes] = useState<Uint8Array | null>(null)
  const [mockLocalBundleSelected, setMockLocalBundleSelected] = useState(false)
  const [localError, setLocalError] = useState<string | null>(null)
  const [browserLoader, setBrowserLoader] = useState<Awaited<
    ReturnType<typeof connectBrowserLoader>
  > | null>(null)
  const [browserOutcome, setBrowserOutcome] = useState<FirmwareOutcome | null>(null)
  const [browserProgress, setBrowserProgress] = useState(0)
  const [browserPreflightStageIndex, setBrowserPreflightStageIndex] = useState(0)
  const browserPreflightTraceRef = useRef<BrowserPreflightTraceEvent[]>([])
  const browserPortSelectionTraceRef = useRef<BrowserPreflightTraceEvent[]>([])
  const [browserSelectedPort, setBrowserSelectedPort] = useState<BrowserSerialPort | null>(null)
  const [browserMessage, setBrowserMessage] = useState<string | null>(null)
  const [executionOutcome, setExecutionOutcome] = useState<FirmwareOutcome>('idle')
  const [executionProgressValue, setExecutionProgressValue] = useState(0)
  const [executionStageIndex, setExecutionStageIndex] = useState(0)
  const [executionMessage, setExecutionMessage] = useState<string | null>(null)
  const [devdArtifactId, setDevdArtifactId] = useState<string | null>(null)
  const [approvalToken, setApprovalToken] = useState<string | null>(null)
  const [allowDowngrade, setAllowDowngrade] = useState(false)
  const transportWasSelected = useRef(false)
  const browserSerial = useMemo(() => (mockExecution ? null : getBrowserSerial()), [mockExecution])
  const browserPreauthorizedPortsRef = useRef<readonly BrowserSerialPort[] | undefined>(undefined)
  const hasExplicitOfficialArtifactSelection = useRef(false)
  const progressPresentationQueue = useRef<Promise<void>>(Promise.resolve())
  const browserRuntimePhaseRef = useRef<'reconnect' | 'verify' | null>(null)
  const devdAvailable = devdHealth === 'available'
  const nativeTarget = useMemo(
    () => nativeTargets.find((target) => target.id === nativeTargetId) ?? null,
    [nativeTargetId, nativeTargets]
  )
  const visibleOfficialArtifacts = useMemo(
    () =>
      sortArtifactsByPublishedAt(
        catalogArtifacts.filter((artifact) => includeRc || artifact.channel !== 'rc')
      ),
    [catalogArtifacts, includeRc]
  )
  const selectedOfficialArtifact = useMemo(
    () =>
      visibleOfficialArtifacts.find((artifact) => artifact.id === selectedOfficialArtifactId) ??
      null,
    [selectedOfficialArtifactId, visibleOfficialArtifacts]
  )
  const pendingOfficialArtifact = useMemo(
    () =>
      visibleOfficialArtifacts.find((artifact) => artifact.id === pendingOfficialArtifactId) ??
      null,
    [pendingOfficialArtifactId, visibleOfficialArtifacts]
  )
  const updateEligible = nativeTarget?.updateEligible === true
  const currentTemperatureC = nativeTarget?.currentTemperatureC
  const heaterEnabled = nativeTarget?.heaterEnabled
  const preflightOutcome = browserOutcome ?? 'idle'
  const busy = preflightOutcome === 'running' || executionOutcome === 'running'
  const showingExecution = executionOutcome !== 'idle'
  const preflightStageList = useMemo(() => preflightStages(), [])
  const executionStageList = useMemo(() => executionStages(operation), [operation])
  const effectiveMessage = showingExecution
    ? (executionMessage ?? '预检凭据已确认；等待开始固件写入。')
    : (browserMessage ?? '选择连接引擎和固件来源后运行完整预检。')
  const transportAvailable =
    transport === 'devd' ? devdAvailable && nativeTarget !== null : browserAvailable
  const canRun =
    operation !== null &&
    transportAvailable &&
    !artifactBlocked &&
    !busy &&
    (channel === 'local'
      ? mockExecution
        ? mockLocalBundleSelected
        : localBundle !== null
      : catalogStatus === 'ready' && selectedOfficialArtifact !== null)
  const canInstall =
    canRun &&
    preflightOutcome === 'preflight_passed' &&
    executionOutcome === 'idle' &&
    (mockExecution ||
      (transport === 'devd'
        ? approvalToken !== null && devdArtifactId !== null
        : browserLoader !== null && localBundle !== null))
  const artifactEntryTitle =
    channel === 'local'
      ? mockExecution
        ? mockLocalBundleSelected
          ? 'demo-local.fluxpurr-fw'
          : '选择演示本地固件包'
        : (localBundle?.manifest.identity.version ?? '本地固件包未完成校验')
      : (selectedOfficialArtifact?.version ?? '选择发布版本')
  const artifactEntryDetail =
    channel === 'local'
      ? mockExecution
        ? '演示内存包 · 不读取本地文件系统'
        : '本地 .fluxpurr-fw · 已通过结构与哈希校验'
      : selectedOfficialArtifact
        ? artifactBlocked
          ? '当前目标不接受该固件包'
          : `发布版本 · ${catalogArtifactLabel(selectedOfficialArtifact)}`
        : catalogStatus === 'loading'
          ? '正在读取发布目录'
          : catalogStatus === 'failed'
            ? '发布目录不可用'
            : catalogStatus === 'empty'
              ? '发布目录中没有可用固件包'
              : '没有可用的发布版本'

  useEffect(() => {
    if (mockExecution || !devdBaseUrl) {
      setDevdHealth('unavailable')
      return
    }

    const controller = new AbortController()
    setDevdHealth('checking')
    void fetch(`${devdBaseUrl}/health`, { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(`devd health failed (${response.status}).`)
        const health = (await response.json()) as { name?: unknown }
        if (health.name !== 'flux-purr-devd') throw new Error('Unexpected devd health response.')
        if (controller.signal.aborted) return
        setDevdHealth('available')
        if (!transportWasSelected.current) setTransport('devd')
      })
      .catch(() => {
        if (!controller.signal.aborted) setDevdHealth('unavailable')
      })

    return () => controller.abort()
  }, [devdBaseUrl, mockExecution])

  useEffect(() => {
    browserPreauthorizedPortsRef.current = undefined
    if (mockExecution || !browserSerial?.getPorts) return

    let cancelled = false
    void browserSerial
      .getPorts()
      .then((ports) => {
        if (!cancelled) {
          browserPreauthorizedPortsRef.current = ports.filter(isFluxPurrUsbSerialPort)
        }
      })
      .catch(() => {
        if (!cancelled) browserPreauthorizedPortsRef.current = undefined
      })
    return () => {
      cancelled = true
    }
  }, [browserSerial, mockExecution])

  useEffect(() => {
    if (officialArtifacts) {
      setCatalogArtifacts(officialArtifacts)
      setCatalogStatus(officialArtifacts.length > 0 ? 'ready' : 'empty')
      setCatalogError(null)
      return
    }

    if (mockExecution) {
      setCatalogArtifacts([])
      setCatalogStatus('empty')
      setCatalogError(null)
      return
    }

    let cancelled = false
    setCatalogStatus('loading')
    setCatalogError(null)
    void fetchOfficialCatalog()
      .then((artifacts) => {
        if (cancelled) return
        setCatalogArtifacts(artifacts)
        setCatalogStatus(artifacts.length > 0 ? 'ready' : 'empty')
      })
      .catch((error) => {
        if (cancelled) return
        setCatalogArtifacts([])
        setCatalogStatus('failed')
        setCatalogError(error instanceof Error ? error.message : '无法读取官方固件目录。')
      })
    return () => {
      cancelled = true
    }
  }, [mockExecution, officialArtifacts])

  useEffect(() => {
    if (channel === 'local') return
    const next = resolveCatalogSelection(
      visibleOfficialArtifacts,
      selectedOfficialArtifactId,
      hasExplicitOfficialArtifactSelection.current
    )
    if (!next) return
    if (next.id !== selectedOfficialArtifactId) setSelectedOfficialArtifactId(next.id)
    if (!hasExplicitOfficialArtifactSelection.current && !artifactDialogOpen) {
      setPendingOfficialArtifactId(next.id)
    }
    const nextChannel = next.channel === 'rc' ? 'rc' : 'stable'
    if (nextChannel !== channel) setChannel(nextChannel)
  }, [artifactDialogOpen, channel, selectedOfficialArtifactId, visibleOfficialArtifacts])

  const selectedBundle = async () => {
    if (mockExecution) {
      throw new Error('Mock firmware workbench must not read an external firmware bundle.')
    }
    if (channel === 'local') {
      if (!localBundle || !localBundleBytes) throw new Error('请选择有效的本地 .fluxpurr-fw。')
      return { bundle: localBundle, bytes: localBundleBytes }
    }
    if (!selectedOfficialArtifact) throw new Error('请选择可用的官方固件。')
    setBrowserMessage(`正在下载并校验 ${selectedOfficialArtifact.version}。`)
    return fetchOfficialBundle(selectedOfficialArtifact)
  }

  const appendBrowserPreflightTrace = (entry: BrowserPreflightTraceEvent) => {
    browserPreflightTraceRef.current = [...browserPreflightTraceRef.current, entry].slice(-32)
    onActivity?.({
      event: entry.event,
      detail: entry.detail,
      tone: entry.tone,
    })
  }

  const appendBrowserPortSelectionTrace = (entry: BrowserPreflightTraceEvent) => {
    browserPortSelectionTraceRef.current = [...browserPortSelectionTraceRef.current, entry].slice(
      -8
    )
    onActivity?.({
      event: entry.event,
      detail: entry.detail,
      tone: entry.tone,
    })
  }

  const showPreflightStage = (
    stage: FirmwarePreflightStage,
    stageProgress = 0,
    message?: string
  ) => {
    setBrowserPreflightStageIndex(preflightStageList.indexOf(stage))
    setBrowserProgress(preflightProgress(stage, stageProgress))
    if (message) setBrowserMessage(message)
  }

  const showExecutionStage = (
    stage: FirmwareExecutionStage,
    stageProgress = 0,
    message?: string
  ) => {
    setExecutionStageIndex(executionStageList.indexOf(stage))
    setExecutionProgressValue(executionProgress(operation, stage, stageProgress))
    if (message) setExecutionMessage(message)
  }

  const reportBrowserRuntimeStage = (event: BrowserRuntimeVerificationEvent) => {
    const reconnectProgress =
      event.stage === 'disconnecting_rom'
        ? 0
        : event.stage === 'waiting_for_runtime'
          ? 0.25
          : event.stage === 'opening_runtime'
            ? 0.65
            : null
    if (typeof reconnectProgress === 'number') {
      showExecutionStage(
        'runtime_reconnect',
        reconnectProgress,
        event.stage === 'opening_runtime'
          ? `正在打开重新枚举的运行时端口（第 ${event.attempt ?? 1} 次）。`
          : '设备已复位，正在等待 Flux Purr 运行时重新枚举。'
      )
      if (browserRuntimePhaseRef.current !== 'reconnect') {
        browserRuntimePhaseRef.current = 'reconnect'
        onActivity?.({
          event: '运行时重连开始',
          detail: 'ROM 写入与校验完成，正在等待 Flux Purr 运行时端口。',
          tone: 'info',
        })
      }
      return
    }
    showExecutionStage(
      'runtime_verify',
      0,
      event.stage === 'requesting_identity'
        ? `正在请求运行时身份与安装状态（第 ${event.attempt ?? 1} 次）。`
        : '运行时已连接；正在读取身份、布局与安装状态。'
    )
    if (event.stage === 'requesting_identity') {
      onActivity?.({
        event: '运行时身份查询',
        detail: `使用当前已授权 Browser USB 端口发起第 ${event.attempt ?? 1} 次身份查询。`,
        tone: 'info',
      })
    }
    if (browserRuntimePhaseRef.current !== 'verify') {
      browserRuntimePhaseRef.current = 'verify'
      onActivity?.({
        event: '运行时身份验证开始',
        detail: '正在读取固件身份、布局与安装状态。',
        tone: 'info',
      })
    }
  }

  const reportBrowserWriteStage = (event: BrowserWriteProgressEvent) => {
    switch (event.stage) {
      case 'erase_started':
        showExecutionStage('erase', 0, '正在擦除 MCU internal Flash。')
        return
      case 'erase_completed':
        showExecutionStage('erase', 1, 'MCU internal Flash 已擦除；准备写入固件分段。')
        return
      case 'write_started':
        showExecutionStage('write_segments', 0, '开始写入三个固件分段。')
        return
      case 'write_progress': {
        const segmentSizes = localBundle?.manifest.segments.map((segment) => segment.length) ?? []
        const index = event.segmentIndex ?? 0
        const completedBefore = segmentSizes
          .slice(0, Math.max(0, index))
          .reduce((sum, length) => sum + length, 0)
        const totalBytes = event.totalBytes ?? segmentSizes.reduce((sum, length) => sum + length, 0)
        const completedBytes = completedBefore + Math.min(event.written ?? 0, event.total ?? 0)
        showExecutionStage(
          'write_segments',
          completedBytes / Math.max(totalBytes, 1),
          `正在写入固件分段 ${Math.min(index + 1, segmentSizes.length)}/${segmentSizes.length}。`
        )
        return
      }
      case 'rom_md5_started':
        showExecutionStage('rom_md5', 0, '写入完成；正在逐段执行 ROM MD5 校验。')
        return
      case 'rom_md5_progress':
        showExecutionStage(
          'rom_md5',
          (event.completedSegments ?? 0) / Math.max(event.totalSegments ?? 1, 1),
          `ROM MD5 已校验 ${event.completedSegments ?? 0}/${event.totalSegments ?? 0} 段。`
        )
        return
      case 'reset_started':
        showExecutionStage('reset', 0, 'ROM MD5 已通过；正在复位设备。')
        return
      case 'reset_completed':
        showExecutionStage('reset', 1, '设备已复位；正在等待运行时重连。')
    }
  }

  const applyDevdProgressEvent = (event: FirmwareOperationProgressEvent) => {
    const progress = progressForFirmwareEvent(event)
    const stageIndex = stageIndexForFirmwareEvent(event)
    if (event.phase === 'preflight') {
      if (stageIndex !== null) setBrowserPreflightStageIndex(stageIndex)
      if (progress !== null) setBrowserProgress((current) => Math.max(current, progress))
      if (event.event === 'stage_started' && event.stage) {
        setBrowserMessage(`devd 正在执行${firmwareStageLabel(event.stage)}。`)
        onActivity?.({
          event: `${firmwareStageLabel(event.stage)}开始`,
          detail: 'devd 已进入该预检阶段。',
          tone: 'info',
        })
      }
      if (event.event === 'stage_failed') setBrowserOutcome('blocked')
      return
    }
    if (stageIndex !== null) setExecutionStageIndex(stageIndex)
    if (progress !== null) {
      setExecutionProgressValue((current) => Math.max(current, progress))
    }
    if (event.event === 'stage_started' && event.stage) {
      setExecutionMessage(`devd 正在执行${firmwareStageLabel(event.stage)}。`)
      onActivity?.({
        event: `${firmwareStageLabel(event.stage)}开始`,
        detail: 'devd 已进入该固件执行阶段。',
        tone: 'info',
      })
    }
    if (event.event === 'stage_failed') setExecutionOutcome('failed')
  }

  const queueDevdProgressEvent = (event: FirmwareOperationProgressEvent) => {
    progressPresentationQueue.current = progressPresentationQueue.current.then(async () => {
      applyDevdProgressEvent(event)
      if (event.event === 'stage_started') await firmwareStagePresentationDelay()
    })
  }

  const runPreflight = async () => {
    if (!operation) return
    setExecutionOutcome('idle')
    setExecutionProgressValue(0)
    setExecutionStageIndex(0)
    setExecutionMessage(null)
    setBrowserOutcome('running')
    showPreflightStage('artifact', 0, '正在校验固件包并准备预检。')

    if (mockExecution) {
      browserPreflightTraceRef.current = []
      setBrowserMessage('正在执行确定性的演示预检。')
      onActivity?.({
        event: '演示预检开始',
        detail: `${operation === 'update' ? '更新现有设备' : '安装或恢复'} · 浏览器 USB 模拟。`,
        tone: 'info',
      })
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '模拟固件包校验完成',
        detail: '内存中的演示固件包已通过确定性结构和哈希样本校验。',
        tone: 'success',
      })
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '模拟浏览器 USB 已确认',
        detail: '演示未调用 navigator.serial、requestPort() 或任何系统串口选择器。',
        tone: 'success',
      })
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '模拟 ROM 安全校验通过',
        detail: 'ESP32-S3、4 MiB Flash、ROM 安全响应和布局由内存样本提供。',
        tone: 'success',
      })
      for (const stage of preflightStageList.slice(1)) {
        await firmwareStagePresentationDelay()
        showPreflightStage(stage, 0)
      }
      await firmwareStagePresentationDelay()
      setBrowserOutcome('preflight_passed')
      setBrowserProgress(100)
      setBrowserMessage('演示预检已通过；未请求浏览器 USB、devd、网络或真实固件文件。')
      onActivity?.({
        event: '演示预检通过',
        detail: '固件、传输、ROM、安全、布局和配置阶段均由内存样本结算。',
        tone: 'success',
      })
      return
    }

    browserPreflightTraceRef.current = []
    const browserPortRequest =
      transport === 'browser'
        ? beginBrowserUsbPreflight({
            serial: browserSerial,
            preauthorizedPorts: browserSelectedPort
              ? [browserSelectedPort]
              : browserPreauthorizedPortsRef.current,
            onTrace: appendBrowserPreflightTrace,
          })
        : null

    onActivity?.({
      event: '开始预检',
      detail: `${operation === 'update' ? '更新现有设备' : '安装或恢复'} · ${transportLabel(transport)}`,
      tone: 'info',
    })

    if (transport === 'devd') {
      if (
        operation === 'update' &&
        (!updateEligible ||
          heaterEnabled !== false ||
          typeof currentTemperatureC !== 'number' ||
          !Number.isFinite(currentTemperatureC) ||
          currentTemperatureC > 40)
      ) {
        setBrowserOutcome('blocked')
        setBrowserMessage('更新要求已验证 Flux Purr 运行时、加热关闭且有效温度不高于 40 C。')
        onActivity?.({
          event: '预检被阻止',
          detail: '更新目标未通过运行时、停热或 40 C 温度门禁。',
          tone: 'warning',
        })
        return
      }
      if (!devdBaseUrl || !nativeTarget?.leaseId) {
        setBrowserOutcome('blocked')
        setBrowserMessage('请选择具有有效租约的本机固件目标。')
        onActivity?.({
          event: '预检被阻止',
          detail: '本机 devd 目标缺少有效租约。',
          tone: 'warning',
        })
        return
      }
      showPreflightStage('artifact', 0, '正在通过 devd 导入并验证固件包。')
      let progressMonitor: DevdFirmwareProgressMonitor | null = null
      try {
        const selected = await selectedBundle()
        const imported = await fetch(`${devdBaseUrl}/api/v1/firmware-bundles`, {
          method: 'POST',
          headers: {
            'content-type': 'application/vnd.flux-purr.firmware-bundle+zip',
          },
          body: Uint8Array.from(selected.bytes).buffer,
        })
        if (!imported.ok) throw new Error(`devd bundle import failed (${imported.status}).`)
        const artifactId = ((await imported.json()) as { artifactId: string }).artifactId
        setDevdArtifactId(artifactId)
        showPreflightStage('transport', 0, '固件包已导入；正在建立 devd 预检事务。')
        progressMonitor = startDevdFirmwareProgressMonitor({
          devdBaseUrl,
          deviceId: nativeTarget.id,
          phase: 'preflight',
          operation,
          artifactId,
          onEvent: queueDevdProgressEvent,
        })
        await progressMonitor.ready
        progressMonitor.arm()
        const response = await fetch(
          `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(nativeTarget.id)}/firmware`,
          {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              leaseId: nativeTarget.leaseId,
              artifactId,
              operation,
              dryRun: true,
              allowDowngrade,
            }),
          }
        )
        const result = (await response.json()) as DevdFirmwareErrorEnvelope & {
          approvalToken?: string
          operationId?: string
        }
        if (result.operationId) progressMonitor.bindOperationId(result.operationId)
        if (!response.ok || !result.approvalToken)
          throw new Error(devdFirmwareResponseMessage(response, result, 'devd preflight failed'))
        await progressPresentationQueue.current
        setApprovalToken(result.approvalToken)
        setBrowserOutcome('preflight_passed')
        setBrowserProgress(100)
        setBrowserPreflightStageIndex(preflightStageList.length - 1)
        setBrowserMessage('devd 完整预检已通过；授权令牌五分钟内单次有效。')
        onActivity?.({
          event: '预检通过',
          detail: 'devd 已验证目标、包、布局、安全状态和写入授权。',
          tone: 'success',
        })
      } catch (error) {
        setApprovalToken(null)
        setBrowserOutcome('blocked')
        setBrowserMessage(error instanceof Error ? error.message : 'devd preflight failed.')
        onActivity?.({
          event: '预检失败',
          detail: error instanceof Error ? error.message : 'devd 固件预检失败。',
          tone: 'error',
        })
      } finally {
        progressMonitor?.close()
      }
      return
    }

    if (browserLoader) {
      await disconnectBrowserLoader(browserLoader)
      setBrowserLoader(null)
    }
    showPreflightStage('artifact', 0)
    let loader: Awaited<ReturnType<typeof connectBrowserLoader>> | null = null
    try {
      if (!browserPortRequest) throw new Error('浏览器 USB 端口请求未启动。请重新点击运行预检。')
      const selectedPort = await browserPortRequest
      browserPreauthorizedPortsRef.current = [selectedPort]
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '固件包校验开始',
        detail: '浏览器 USB 端口已确认，正在读取并校验固件包。',
        tone: 'info',
      })
      const selected = await selectedBundle()
      showPreflightStage('transport', 0, '固件包已校验；浏览器 USB 端口已确认。')
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '固件包校验完成',
        detail: '固件包已通过当前预检使用的结构与哈希校验。',
        tone: 'success',
      })
      let runtimePort: BrowserSerialPort = selectedPort

      if (operation === 'update') {
        setBrowserMessage('正在验证 Flux Purr 运行时、安装状态并停止加热。')
        appendBrowserPreflightTrace({
          at: new Date().toISOString(),
          event: '运行时连接开始',
          detail: '使用已选择的浏览器 USB 端口验证 Flux Purr 运行时。',
          tone: 'info',
        })
        const runtimeClient = new WebSerialControlPlaneClient({
          preauthorizedPorts: [selectedPort],
          onDiagnostic: (diagnostic) => {
            const detail =
              diagnostic.kind === 'boot_stage'
                ? `设备已报告 boot_stage=${diagnostic.reason}。`
                : diagnostic.kind === 'reset'
                  ? `设备报告 reset_reason=${diagnostic.reason}。`
                  : `设备报告 panic=${diagnostic.reason}。`
            appendBrowserPreflightTrace({
              at: new Date().toISOString(),
              event:
                diagnostic.kind === 'boot_stage'
                  ? '固件启动阶段'
                  : diagnostic.kind === 'reset'
                    ? '固件已复位'
                    : '固件 panic',
              detail,
              tone: diagnostic.kind === 'boot_stage' ? 'info' : 'warning',
            })
          },
          onInitializationRetry: ({ remainingMs }) => {
            const remainingSeconds = Math.max(1, Math.ceil(remainingMs / 1_000))
            setBrowserMessage(`正在等待 Flux Purr 启动就绪（最多还有 ${remainingSeconds} 秒）。`)
            appendBrowserPreflightTrace({
              at: new Date().toISOString(),
              event: '等待运行时就绪',
              detail: `尚未收到匹配的 USB JSONL 响应；仅等待 boot_stage=runtime_ready 后重发一次，最多还有 ${remainingSeconds} 秒。`,
              tone: 'warning',
            })
          },
        })
        try {
          const runtime = await runtimeClient.connect()
          runtimePort = runtimeClient.connectedPort
          const installStatus = await runtimeClient.getInstallStatus()
          if (
            !runtime.identity.capabilities.includes('install_status') ||
            !installStatus.layoutId
          ) {
            throw new Error('浏览器 USB 目标不是可验证的 Flux Purr 运行时；请选择安装或恢复。')
          }

          const stopped = runtime.status.heaterEnabled
            ? await runtimeClient.configureRuntime({ heaterEnabled: false })
            : runtime.status
          if (
            stopped.heaterEnabled ||
            typeof stopped.currentTempC !== 'number' ||
            !Number.isFinite(stopped.currentTempC) ||
            stopped.currentTempC > 40
          ) {
            throw new Error('更新要求加热关闭且有效温度不高于 40 C。')
          }
          if (
            compareSemver(
              selected.bundle.manifest.identity.version,
              runtime.identity.firmwareVersion
            ) < 0 &&
            !allowDowngrade
          ) {
            throw new Error('目标固件版本更旧；必须先启用高级降级确认。')
          }
          onActivity?.({
            event: '运行时已验证',
            detail: `Flux Purr ${runtime.identity.firmwareVersion} 已停热，温度 ${stopped.currentTempC.toFixed(1)} C。`,
            tone: 'success',
          })
        } finally {
          await runtimeClient.disconnect()
        }
      }

      setBrowserMessage('正在连接 ESP32-S3 ROM；必要时按住 BOOT 后点按 RESET。')
      showPreflightStage('rom_reset', 0)
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: 'ROM 连接开始',
        detail: '正在使用已选择的端口连接 ESP32-S3 ROM bootloader。',
        tone: 'info',
      })
      loader = await connectBrowserLoader(runtimePort)
      showPreflightStage('chip_flash_security', 0, 'ROM 已连接；正在验证芯片、Flash 与安全状态。')
      await preflightBrowserLoader(loader, selected.bundle, operation)
      showPreflightStage('layout_config', 1, '芯片安全与布局配置已通过；正在结算预检。')
      setLocalBundle(selected.bundle)
      setLocalBundleBytes(selected.bytes)
      setBrowserLoader(loader)
      loader = null
      showPreflightStage('preflight', 1)
      setBrowserOutcome('preflight_passed')
      setBrowserProgress(100)
      setBrowserMessage('ROM、Flash 容量和安全状态已通过；可开始完整写入。')
      onActivity?.({
        event: '预检通过',
        detail: 'Browser 已验证 ROM、安全状态、Flash 容量与布局配置。',
        tone: 'success',
      })
    } catch (error) {
      setBrowserLoader(null)
      setBrowserOutcome('blocked')
      setBrowserMessage(error instanceof Error ? error.message : 'Browser ROM preflight failed.')
      appendBrowserPreflightTrace({
        at: new Date().toISOString(),
        event: '预检失败',
        detail: error instanceof Error ? error.message : 'Browser ROM 预检失败。',
        tone: 'error',
      })
    }
  }

  const runInstall = async () => {
    if (!operation) return
    if (
      !mockExecution &&
      ((transport === 'devd' &&
        (!devdBaseUrl || !nativeTarget?.leaseId || !devdArtifactId || !approvalToken)) ||
        (transport === 'browser' && (!browserLoader || !localBundle)))
    ) {
      return
    }
    setExecutionOutcome('running')
    setExecutionProgressValue(0)
    setExecutionStageIndex(0)
    setExecutionMessage('预检凭据已确认；正在开始固件事务。')
    if (mockExecution) {
      showExecutionStage(executionStageList[0], 0, '正在演示完整固件事务。')
      onActivity?.({
        event: '演示写入开始',
        detail: `${operation === 'update' ? '保留配置更新' : '全擦后安装'}由内存状态机模拟。`,
        tone: 'info',
      })
      for (const stage of executionStageList.slice(1)) {
        await firmwareStagePresentationDelay()
        showExecutionStage(stage, 0)
      }
      await firmwareStagePresentationDelay()
      setExecutionOutcome('verified')
      setExecutionProgressValue(100)
      setExecutionStageIndex(executionStageList.length - 1)
      setExecutionMessage('演示固件事务已验证；未连接、复位、擦除或写入任何设备。')
      onActivity?.({
        event: '演示事务已验证',
        detail: '三段写入、ROM MD5、运行时重连与身份验证均为确定性内存结果。',
        tone: 'success',
      })
      return
    }
    if (transport === 'devd') {
      if (!devdBaseUrl || !nativeTarget?.leaseId || !devdArtifactId || !approvalToken) return
      showExecutionStage('authorization', 0, 'devd 正在校验单次授权并取得串口独占。')
      onActivity?.({
        event: '开始写入',
        detail: `${operation === 'update' ? '保留配置更新' : '全擦后安装'}正由 devd 执行。`,
        tone: 'info',
      })
      const progressMonitor = startDevdFirmwareProgressMonitor({
        devdBaseUrl,
        deviceId: nativeTarget.id,
        phase: 'execution',
        operation,
        artifactId: devdArtifactId,
        onEvent: queueDevdProgressEvent,
      })
      try {
        await progressMonitor.ready
        progressMonitor.arm()
        const response = await fetch(
          `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(nativeTarget.id)}/firmware`,
          {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              leaseId: nativeTarget.leaseId,
              artifactId: devdArtifactId,
              operation,
              dryRun: false,
              approvalToken,
              confirm: operation === 'update' ? 'FLASH' : 'ERASE_INSTALL',
              allowDowngrade,
            }),
          }
        )
        const result = (await response.json()) as DevdFirmwareErrorEnvelope & {
          outcome?: FirmwareOutcome
          operationId?: string
        }
        if (result.operationId) progressMonitor.bindOperationId(result.operationId)
        if (!response.ok)
          throw new Error(
            devdFirmwareResponseMessage(response, result, 'devd firmware transaction failed')
          )
        await progressPresentationQueue.current
        const outcome = result.outcome ?? 'failed'
        setExecutionOutcome(outcome)
        if (outcome === 'verified') {
          setExecutionStageIndex(executionStageList.length - 1)
          setExecutionProgressValue(100)
        } else if (outcome === 'write_complete_unverified') {
          const runtimeVerifyIndex = executionStageList.indexOf('runtime_verify')
          setExecutionStageIndex(runtimeVerifyIndex)
          setExecutionProgressValue((current) =>
            Math.max(current, executionProgress(operation, 'runtime_verify', 0))
          )
        }
        setExecutionMessage(result.message ?? 'devd firmware transaction completed.')
        setApprovalToken(null)
        onActivity?.({
          event: result.outcome === 'verified' ? '事务已验证' : '写入已完成',
          detail: result.message ?? 'devd 固件事务已结束。',
          tone: result.outcome === 'verified' ? 'success' : 'warning',
        })
      } catch (error) {
        setApprovalToken(null)
        setExecutionOutcome('failed')
        setExecutionMessage(
          error instanceof Error ? error.message : 'devd firmware transaction failed.'
        )
        onActivity?.({
          event: '事务失败',
          detail: error instanceof Error ? error.message : 'devd 固件事务失败。',
          tone: 'error',
        })
      } finally {
        progressMonitor.close()
      }
      return
    }
    if (!browserLoader || !localBundle) return
    showExecutionStage(
      operation === 'install_recovery' ? 'erase' : 'write_segments',
      0,
      '写入期间请保持 USB 连接；中断后必须重新运行完整预检。'
    )
    onActivity?.({
      event: '开始写入',
      detail: `${operation === 'update' ? '保留配置更新' : '全擦后安装'}正由 Browser USB 执行。`,
      tone: 'info',
    })
    try {
      await writeBrowserBundle(browserLoader, localBundle, operation, {
        reportStage: reportBrowserWriteStage,
      })
      // The application reset has completed. Mirror the established Browser
      // USB lifecycle by releasing the ROM transport before opening runtime.
      await disconnectBrowserLoader(browserLoader)
      showExecutionStage(
        'runtime_reconnect',
        0,
        'ROM MD5 与复位已完成；正在等待 Flux Purr 运行时重连。'
      )
      try {
        browserRuntimePhaseRef.current = null
        await verifyBrowserRuntime(browserLoader, localBundle, {
          romTransportAlreadyDisconnected: true,
          reportStage: reportBrowserRuntimeStage,
        })
        setExecutionStageIndex(executionStageList.length - 1)
        setExecutionOutcome('verified')
        setExecutionProgressValue(100)
        setExecutionMessage('固件字节、运行时身份、布局与安装状态均已验证。')
        onActivity?.({
          event: '事务已验证',
          detail: 'ROM MD5、运行时身份、布局与安装状态一致。',
          tone: 'success',
        })
      } catch (error) {
        setExecutionOutcome('write_complete_unverified')
        showExecutionStage('runtime_verify', 0)
        setExecutionMessage(
          error instanceof Error
            ? `写入已完成，但运行时未验证：${error.message}`
            : '写入已完成，但运行时未验证。'
        )
        onActivity?.({
          event: '写入待验证',
          detail: error instanceof Error ? error.message : '运行时重连或身份验证未完成。',
          tone: 'warning',
        })
      }
    } catch (error) {
      await disconnectBrowserLoader(browserLoader)
      setBrowserLoader(null)
      setExecutionOutcome('failed')
      setExecutionMessage(error instanceof Error ? error.message : 'Browser firmware write failed.')
      onActivity?.({
        event: '事务失败',
        detail: error instanceof Error ? error.message : 'Browser 固件写入失败。',
        tone: 'error',
      })
    }
  }

  const resetAuthorization = () => {
    if (!mockExecution) void disconnectBrowserLoader(browserLoader)
    setBrowserLoader(null)
    setApprovalToken(null)
    setDevdArtifactId(null)
    setBrowserOutcome(null)
    setBrowserMessage(null)
    setBrowserProgress(0)
    setBrowserPreflightStageIndex(0)
    setExecutionOutcome('idle')
    setExecutionProgressValue(0)
    setExecutionStageIndex(0)
    setExecutionMessage(null)
    browserRuntimePhaseRef.current = null
  }

  const chooseOperation = (next: FirmwareOperation) => {
    resetAuthorization()
    setOperation(next)
    onActivity?.({
      event: '任务已选择',
      detail:
        next === 'update' ? '将保留并验证 Flux Purr 配置。' : '将擦除 MCU internal Flash 后安装。',
      tone: 'info',
    })
  }

  const chooseTransport = (next: FirmwareTransport) => {
    resetAuthorization()
    transportWasSelected.current = true
    setTransport(next)
    onActivity?.({
      event: '连接引擎已切换',
      detail: `${transportLabel(next)} 将执行已选任务的完整预检。`,
      tone: 'info',
    })
  }

  const chooseBrowserUsbPort = () => {
    if (mockExecution || !browserSerial || busy) return

    // `requestPort()` must remain in this synchronous click path. Resetting the
    // old transaction only changes local state and never selects a new device.
    resetAuthorization()
    browserPortSelectionTraceRef.current = []
    const selection = beginBrowserUsbPreflight({
      serial: browserSerial,
      forcePortSelection: true,
      selectionReason: 'change_port',
      onTrace: appendBrowserPortSelectionTrace,
    })
    void selection.then(
      (port) => {
        browserPreauthorizedPortsRef.current = [port]
        setBrowserSelectedPort(port)
        onActivity?.({
          event: '浏览器 USB 端口已固定',
          detail: '后续预检仅使用刚刚在浏览器选择器中确认的设备。',
          tone: 'success',
        })
      },
      () => undefined
    )
  }

  const chooseOfficialArtifact = (artifactId: string) => {
    const artifact = catalogArtifacts.find((candidate) => candidate.id === artifactId)
    if (!artifact) return
    resetAuthorization()
    hasExplicitOfficialArtifactSelection.current = true
    setMockLocalBundleSelected(false)
    setSelectedOfficialArtifactId(artifact.id)
    setChannel(artifact.channel === 'rc' ? 'rc' : 'stable')
    onActivity?.({
      event: '固件来源已选择',
      detail: `${artifact.version} · ${catalogArtifactLabel(artifact)}。`,
      tone: 'info',
    })
  }

  const openArtifactDialog = () => {
    setPendingOfficialArtifactId(
      selectedOfficialArtifact?.id ?? visibleOfficialArtifacts[0]?.id ?? null
    )
    setArtifactDialogTab(channel === 'local' ? 'local' : 'release')
    setArtifactDialogOpen(true)
  }

  const confirmOfficialArtifact = () => {
    if (!pendingOfficialArtifact) return
    chooseOfficialArtifact(pendingOfficialArtifact.id)
    setArtifactDialogOpen(false)
  }

  const setRcEnabled = (next: boolean) => {
    setIncludeRc(next)
    if (!next) {
      setPendingOfficialArtifactId((current) => {
        const currentIsStable = catalogArtifacts.some(
          (artifact) => artifact.id === current && artifact.channel === 'stable'
        )
        return currentIsStable ? current : preferredArtifactId(catalogArtifacts)
      })
    }
  }

  const importLocal = async (file?: File): Promise<boolean> => {
    setLocalBundle(null)
    setLocalBundleBytes(null)
    setLocalError(null)
    if (!file) return false
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const bundle = await validateFirmwareBundle(bytes)
      resetAuthorization()
      setLocalBundle(bundle)
      setLocalBundleBytes(bytes)
      setChannel('local')
      setArtifactDialogOpen(false)
      onActivity?.({
        event: '本地包已导入',
        detail: '本地 .fluxpurr-fw 已通过结构与哈希校验。',
        tone: 'success',
      })
      return true
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : '固件包校验失败。')
      onActivity?.({
        event: '本地包被拒绝',
        detail: error instanceof Error ? error.message : '本地固件包校验失败。',
        tone: 'error',
      })
      return false
    }
  }

  const chooseMockLocalBundle = () => {
    resetAuthorization()
    setLocalError(null)
    setMockLocalBundleSelected(true)
    setChannel('local')
    setArtifactDialogOpen(false)
    onActivity?.({
      event: '演示本地包已选择',
      detail: '已采用内存中的 .fluxpurr-fw 样本；没有打开系统文件选择器。',
      tone: 'success',
    })
  }

  const downloadDiagnostic = () => {
    const report = {
      schemaVersion: 1,
      operation,
      transport,
      channel,
      preflight: {
        outcome: preflightOutcome,
        progress: browserProgress,
        stage: preflightStageList[browserPreflightStageIndex],
        message: browserMessage,
      },
      execution: {
        outcome: executionOutcome,
        progress: executionProgressValue,
        stage: executionStageList[executionStageIndex],
        message: executionMessage,
      },
      bundleSha256: localBundle?.bundleSha256 ?? null,
      browserPreflightTrace: [
        ...browserPortSelectionTraceRef.current,
        ...browserPreflightTraceRef.current,
      ],
      generatedAt: new Date().toISOString(),
    }
    const url = URL.createObjectURL(
      new Blob([`${JSON.stringify(report, null, 2)}\n`], {
        type: 'application/json',
      })
    )
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = 'flux-purr-firmware-report.json'
    anchor.click()
    URL.revokeObjectURL(url)
  }

  return (
    <section className="firmware-workbench" aria-label="固件安装工作台">
      <fieldset className="firmware-workbench__tasks">
        <legend className="sr-only">固件任务</legend>
        <button
          type="button"
          className={operation === 'update' ? 'is-active' : undefined}
          aria-pressed={operation === 'update'}
          disabled={busy}
          onClick={() => chooseOperation('update')}
        >
          <span className="firmware-workbench__task-icon">
            <RotateCcw size={19} aria-hidden="true" />
          </span>
          <span>
            <strong>更新现有设备</strong>
            <small>保留设备配置并验证运行时身份</small>
          </span>
          <em>保留配置</em>
        </button>
        <button
          type="button"
          className={operation === 'install_recovery' ? 'is-active' : undefined}
          aria-pressed={operation === 'install_recovery'}
          disabled={busy}
          onClick={() => chooseOperation('install_recovery')}
        >
          <span className="firmware-workbench__task-icon">
            <Zap size={19} aria-hidden="true" />
          </span>
          <span>
            <strong>安装或恢复</strong>
            <small>适用于空片、外来固件或无法启动的设备</small>
          </span>
          <em>完整安装</em>
        </button>
      </fieldset>

      <div className="firmware-workbench__controls">
        <div className="firmware-workbench__controls-heading">
          <strong>连接与固件</strong>
          <span>
            {transport === 'devd'
              ? devdHealthLabel(devdHealth)
              : mockExecution
                ? '演示模拟 Browser USB ROM'
                : '浏览器直接连接 ROM'}
          </span>
        </div>
        <fieldset>
          <legend>连接引擎</legend>
          <label>
            <input
              type="radio"
              name="firmwareTransport"
              checked={transport === 'devd'}
              disabled={!devdAvailable || busy}
              onChange={() => chooseTransport('devd')}
            />
            <Server size={16} aria-hidden="true" />
            <span>
              本机 devd<small>{devdHealthLabel(devdHealth)}</small>
            </span>
          </label>
          <label>
            <input
              type="radio"
              name="firmwareTransport"
              checked={transport === 'browser'}
              disabled={!browserAvailable || busy}
              onClick={() => {
                if (transport === 'browser') transportWasSelected.current = true
              }}
              onChange={() => chooseTransport('browser')}
            />
            <Usb size={16} aria-hidden="true" />
            <span>
              浏览器 USB
              <small>
                {mockExecution ? '演示模拟' : browserAvailable ? 'Chrome / Edge' : '不可用'}
              </small>
            </span>
          </label>
        </fieldset>

        {transport === 'devd' ? (
          <section
            className="firmware-workbench__native-target"
            aria-labelledby="native-target-label"
          >
            <span id="native-target-label">本机固件目标</span>
            <Select
              value={nativeTargetId ?? undefined}
              disabled={!devdAvailable || busy || nativeTargets.length === 0}
              onValueChange={(targetId) => {
                resetAuthorization()
                setNativeTargetId(targetId)
                onActivity?.({
                  event: '本机目标已选择',
                  detail: '该授权目标将仅用于当前固件事务。',
                  tone: 'info',
                })
              }}
            >
              <SelectTrigger
                className="firmware-workbench__native-target-trigger"
                aria-label="本机固件目标"
              >
                <SelectValue
                  placeholder={
                    nativeTargets.length === 0 ? '没有已授权的本机目标' : '选择已授权的本机目标'
                  }
                />
              </SelectTrigger>
              <SelectContent position="popper">
                <SelectGroup>
                  <SelectLabel>已授权目标</SelectLabel>
                  {nativeTargets.map((target) => (
                    <SelectItem key={target.id} value={target.id}>
                      {target.label} · {target.detail}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </section>
        ) : null}

        {transport === 'browser' && !mockExecution ? (
          <section
            className="firmware-workbench__browser-target"
            aria-labelledby="browser-target-label"
          >
            <span id="browser-target-label">浏览器 USB 目标</span>
            <Button
              type="button"
              variant="outline"
              className="firmware-workbench__browser-target-trigger h-10 justify-start px-3"
              disabled={!browserAvailable || busy}
              onClick={chooseBrowserUsbPort}
            >
              <Usb data-icon="inline-start" aria-hidden="true" />
              <span className="grid min-w-0 flex-1 gap-0.5 text-left">
                <strong>选择 / 更换浏览器 USB 端口</strong>
                <small>
                  {browserSelectedPort
                    ? '已固定为本次在浏览器选择器中确认的设备'
                    : '打开 Chrome 选择器，仅显示 ESP32-S3 USB JTAG/serial 设备'}
                </small>
              </span>
              <ChevronRight data-icon="inline-end" aria-hidden="true" />
            </Button>
          </section>
        ) : null}

        <section className="firmware-workbench__artifact" aria-labelledby="firmware-artifact-label">
          <span id="firmware-artifact-label">固件包</span>
          <Button
            type="button"
            variant="outline"
            className="firmware-workbench__artifact-trigger h-auto min-h-10 w-full justify-between px-3 py-2 text-left"
            aria-label="选择固件包"
            disabled={busy}
            onClick={openArtifactDialog}
          >
            <PackageOpen data-icon="inline-start" aria-hidden="true" />
            <span className="grid min-w-0 flex-1 gap-0.5">
              <strong className="truncate">{artifactEntryTitle}</strong>
              <small className="truncate">{artifactEntryDetail}</small>
            </span>
            <ChevronRight data-icon="inline-end" aria-hidden="true" />
          </Button>
        </section>
      </div>

      <div className="firmware-workbench__advanced-options">
        {operation === 'update' ? (
          <label className="firmware-workbench__downgrade">
            <input
              type="checkbox"
              checked={allowDowngrade}
              disabled={busy}
              onChange={(event) => {
                resetAuthorization()
                setAllowDowngrade(event.currentTarget.checked)
              }}
            />
            <span>允许安装较旧版本</span>
          </label>
        ) : null}
      </div>

      <Dialog
        open={artifactDialogOpen}
        onOpenChange={(nextOpen) => {
          setArtifactDialogOpen(nextOpen)
          if (!nextOpen) setLocalError(null)
        }}
      >
        <DialogContent
          className={
            artifactDialogTab === 'release'
              ? 'firmware-artifact-dialog sm:h-[min(42rem,calc(100dvh-3rem))] sm:max-w-4xl sm:grid-rows-[auto_minmax(0,1fr)]'
              : 'firmware-artifact-dialog'
          }
        >
          <DialogHeader>
            <DialogTitle>选择固件包</DialogTitle>
            <DialogDescription>
              {mockExecution
                ? '发布版本和本地包均为确定性内存样本；不会访问网络、文件系统或浏览器 USB。'
                : '发布版本来自同源发布目录；本地文件同样执行完整结构、哈希与目标校验。'}
            </DialogDescription>
          </DialogHeader>

          <Tabs
            value={artifactDialogTab}
            className={artifactDialogTab === 'release' ? 'flex min-h-0 flex-col' : undefined}
            onValueChange={(value) => {
              if (value === 'release' || value === 'local') setArtifactDialogTab(value)
            }}
          >
            <div className="grid min-h-0 flex-1 gap-6 md:grid-cols-[minmax(13rem,0.72fr)_minmax(0,1.28fr)]">
              <aside className="grid content-start gap-5">
                <TabsList>
                  <TabsTrigger value="release">发布版本</TabsTrigger>
                  <TabsTrigger value="local">
                    {mockExecution ? '演示本地包' : '本地文件'}
                  </TabsTrigger>
                </TabsList>
                {artifactDialogTab === 'release' ? (
                  <>
                    <div className="grid gap-1.5">
                      <Label htmlFor="firmware-rc-channel">候选版</Label>
                      <div className="flex items-center gap-3">
                        <Switch
                          id="firmware-rc-channel"
                          checked={includeRc}
                          disabled={busy}
                          onCheckedChange={setRcEnabled}
                        />
                        <span className="text-sm">显示 RC</span>
                      </div>
                      <p className="text-muted-foreground text-sm">默认只显示稳定发布版本。</p>
                    </div>
                    <div className="grid gap-1.5 border-t pt-4">
                      <span className="text-muted-foreground text-xs font-medium">当前选择</span>
                      <strong className="truncate text-sm">
                        {pendingOfficialArtifact?.version ?? '尚未选择版本'}
                      </strong>
                      <span className="text-muted-foreground text-xs">
                        {pendingOfficialArtifact?.publishedAt.slice(0, 10) ?? '发布目录读取后显示'}
                      </span>
                    </div>
                    <p className="text-muted-foreground text-sm">
                      {catalogStatus === 'failed'
                        ? catalogError
                        : '选择后采用该版本；实际下载与校验在预检开始时执行。'}
                    </p>
                  </>
                ) : (
                  <p className="text-muted-foreground text-sm">
                    {mockExecution
                      ? '采用内存中的本地包样本，不打开系统文件选择器。'
                      : '从此计算机选择已下载的 .fluxpurr-fw 固件包。'}
                  </p>
                )}
              </aside>

              <TabsContent
                value="release"
                className="mt-0 grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] gap-2"
              >
                <div className="flex items-baseline justify-between gap-3">
                  <p id="firmware-release-list-title" className="text-sm font-medium">
                    版本列表
                  </p>
                  <span className="text-muted-foreground shrink-0 text-xs">最新发布在前</span>
                </div>
                <ScrollArea
                  className="firmware-release-list h-full min-h-0 rounded-md border"
                  scrollableNodeProps={{
                    'aria-labelledby': 'firmware-release-list-title',
                  }}
                >
                  <RadioGroup
                    value={pendingOfficialArtifact?.id ?? undefined}
                    disabled={busy || catalogStatus !== 'ready'}
                    onValueChange={setPendingOfficialArtifactId}
                    aria-label="发布版本"
                    className="content-start gap-0"
                  >
                    {catalogStatus === 'loading' ? (
                      <p className="text-muted-foreground text-sm">正在读取发布目录...</p>
                    ) : null}
                    {catalogStatus === 'failed' ? (
                      <p className="text-destructive text-sm">{catalogError}</p>
                    ) : null}
                    {catalogStatus === 'empty' ? (
                      <p className="firmware-release-empty text-sm">
                        发布目录中尚无可校验的 .fluxpurr-fw 固件包。
                      </p>
                    ) : null}
                    {catalogStatus === 'ready'
                      ? visibleOfficialArtifacts.map((artifact) => (
                          <Label
                            key={artifact.id}
                            htmlFor={`firmware-release-artifact-${artifact.id}`}
                            className="grid w-full grid-cols-[1rem_minmax(0,1fr)_auto] items-center gap-3 border-b px-3 py-3 last:border-b-0 has-[[data-state=checked]]:bg-primary/5"
                          >
                            <RadioGroupItem
                              id={`firmware-release-artifact-${artifact.id}`}
                              value={artifact.id}
                            />
                            <span className="grid min-w-0 gap-0.5">
                              <strong className="truncate">{artifact.version}</strong>
                              <small className="text-muted-foreground truncate text-xs">
                                {artifact.publishedAt.slice(0, 10)} · {artifact.target}
                              </small>
                            </span>
                            {artifact.channel === 'rc' ? (
                              <Badge variant="secondary">RC</Badge>
                            ) : artifact.source === 'local' || artifact.channel === 'local' ? (
                              <Badge variant="outline">本地构建</Badge>
                            ) : null}
                          </Label>
                        ))
                      : null}
                  </RadioGroup>
                </ScrollArea>
              </TabsContent>

              <TabsContent value="local" className="mt-0 min-h-0 min-w-0">
                <div className="grid gap-4">
                  <div className="grid gap-2">
                    {mockExecution ? (
                      <>
                        <Label>演示本地 .fluxpurr-fw</Label>
                        <Button
                          type="button"
                          variant="outline"
                          disabled={busy}
                          onClick={chooseMockLocalBundle}
                        >
                          采用演示本地包
                        </Button>
                        <p className="text-muted-foreground text-sm">
                          演示包驻留在内存中，仅模拟结构、哈希与目标校验结果。
                        </p>
                      </>
                    ) : (
                      <>
                        <Label htmlFor="firmware-local-bundle">本地 .fluxpurr-fw</Label>
                        <Input
                          id="firmware-local-bundle"
                          type="file"
                          accept=".fluxpurr-fw,application/vnd.flux-purr.firmware-bundle+zip"
                          disabled={busy}
                          onChange={(event) => void importLocal(event.currentTarget.files?.[0])}
                        />
                        <p className="text-muted-foreground text-sm">
                          本地包不标记为官方来源，仍必须通过完整校验。
                        </p>
                        {localError ? (
                          <p className="text-destructive text-sm">{localError}</p>
                        ) : null}
                      </>
                    )}
                  </div>
                </div>
              </TabsContent>
            </div>

            <DialogFooter className="mt-4 shrink-0">
              <DialogClose asChild>
                <Button type="button" variant="outline">
                  取消
                </Button>
              </DialogClose>
              {artifactDialogTab === 'release' ? (
                <Button
                  type="button"
                  disabled={busy || catalogStatus !== 'ready' || !pendingOfficialArtifact}
                  onClick={confirmOfficialArtifact}
                >
                  采用此版本
                </Button>
              ) : null}
            </DialogFooter>
          </Tabs>
        </DialogContent>
      </Dialog>

      {transport === 'browser' ? (
        <aside className="firmware-workbench__boot-guide" aria-label="Browser USB ROM 引导">
          <Usb size={17} aria-hidden="true" />
          <span>
            <strong>{mockExecution ? 'ROM 引导（模拟）' : 'ROM 引导'}</strong>
            <small>
              {mockExecution
                ? '演示使用内存中的 USB、ROM 与安全响应样本，不会请求或操作浏览器串口。'
                : '预检请求浏览器 USB 后，若未进入 ROM，请按住 BOOT、点按 RESET，再松开 BOOT。'}
            </small>
          </span>
        </aside>
      ) : null}

      {operation === 'install_recovery' ? (
        <p className="firmware-workbench__erase-notice">
          <Zap size={15} aria-hidden="true" />
          MCU Flash 将完整擦除；外置 EEPROM 不在擦除范围内。
        </p>
      ) : null}

      {showingExecution ? (
        <FirmwarePhaseProgress
          phase="execution"
          outcome={executionOutcome}
          progress={executionProgressValue}
          message={effectiveMessage}
          stages={executionStageList}
          activeStageIndex={executionStageIndex}
        />
      ) : (
        <FirmwarePhaseProgress
          phase="preflight"
          outcome={preflightOutcome}
          progress={browserProgress}
          message={effectiveMessage}
          stages={preflightStageList}
          activeStageIndex={browserPreflightStageIndex}
        />
      )}

      <div className="firmware-workbench__actions">
        <button
          type="button"
          className={`industrial-button ${preflightOutcome === 'preflight_passed' ? 'industrial-button--secondary' : 'industrial-button--primary'}`}
          disabled={!canRun}
          onClick={() => void runPreflight()}
        >
          <ShieldCheck size={17} aria-hidden="true" />
          运行预检
        </button>
        <button
          type="button"
          className={`industrial-button firmware-workbench__install ${preflightOutcome === 'preflight_passed' ? 'industrial-button--primary' : 'industrial-button--secondary'}`}
          disabled={!canInstall}
          onClick={() => void runInstall()}
        >
          <Zap size={17} aria-hidden="true" />
          {operation === 'update'
            ? '开始更新'
            : operation === 'install_recovery'
              ? '擦除并安装'
              : '选择任务'}
        </button>
        <button
          type="button"
          className="industrial-button industrial-button--ghost firmware-workbench__download"
          aria-label="下载本地诊断报告"
          title="下载本地诊断报告"
          onClick={downloadDiagnostic}
        >
          <Download size={17} aria-hidden="true" />
        </button>
      </div>
    </section>
  )
}

function FirmwarePhaseProgress({
  phase,
  outcome,
  progress,
  message,
  stages,
  activeStageIndex,
}: {
  phase: 'preflight' | 'execution'
  outcome: FirmwareOutcome
  progress: number
  message: string
  stages: readonly FirmwareStage[]
  activeStageIndex: number
}) {
  const complete =
    (phase === 'preflight' && outcome === 'preflight_passed') ||
    (phase === 'execution' && outcome === 'verified')
  const failed =
    outcome === 'blocked' || outcome === 'failed' || outcome === 'write_complete_unverified'
  const progressLabel = phase === 'preflight' ? '预检进度' : '更新进度'
  return (
    <section
      className="firmware-workbench__status"
      data-outcome={outcome}
      data-phase={phase}
      aria-labelledby={`${phase}-progress-title`}
    >
      <div className="firmware-workbench__status-heading">
        <span className="firmware-workbench__status-icon">
          <ShieldCheck size={20} aria-hidden="true" />
        </span>
        <span>
          <strong id={`${phase}-progress-title`}>
            {progressLabel} · {outcomeLabel(outcome, phase)}
          </strong>
          <small>{message}</small>
        </span>
        <output aria-label={`${progressLabel}百分比`}>{Math.round(progress)}%</output>
      </div>
      <div
        className="firmware-workbench__progress"
        role="progressbar"
        aria-label={progressLabel}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress}
      >
        <span
          style={{
            transform: `scaleX(${Math.max(0, Math.min(progress, 100)) / 100})`,
          }}
        />
      </div>
      <ol aria-label={`${progressLabel}阶段`}>
        {stages.map((stage, index) => (
          <li
            key={stage}
            data-state={
              complete || index < activeStageIndex
                ? 'done'
                : index === activeStageIndex
                  ? failed
                    ? 'failed'
                    : 'active'
                  : 'pending'
            }
          >
            <span aria-hidden="true" />
            {firmwareStageLabel(stage)}
          </li>
        ))}
      </ol>
    </section>
  )
}

function transportLabel(transport: FirmwareTransport) {
  return transport === 'devd' ? '本机 devd' : '浏览器 USB'
}

function catalogArtifactLabel(artifact: OfficialFirmwareArtifact) {
  if (artifact.source === 'local' || artifact.channel === 'local') return '本地构建'
  return artifact.channel === 'rc' ? '候选版（RC）' : '稳定版'
}

function devdHealthLabel(health: 'checking' | 'available' | 'unavailable') {
  if (health === 'available') return '推荐'
  if (health === 'checking') return '检测中'
  return '不可用'
}

export function FirmwareTransactionLog({ entries }: { entries: FirmwareActivityEntry[] }) {
  const scrollableNodeRef = useRef<HTMLDivElement | null>(null)
  const [followTail, setFollowTail] = useState(true)
  const latestEntryId = entries.at(-1)?.id

  const scrollToTail = useCallback((behavior: ScrollBehavior = 'auto') => {
    const scrollableNode = scrollableNodeRef.current
    if (!scrollableNode) return
    scrollableNode.scrollTo({ top: scrollableNode.scrollHeight, behavior })
  }, [])

  const handleFollowTail = useCallback(() => {
    scrollToTail('smooth')
    setFollowTail(true)
  }, [scrollToTail])

  useLayoutEffect(() => {
    if (followTail && latestEntryId) scrollToTail()
  }, [followTail, latestEntryId, scrollToTail])

  const handleScroll = () => {
    const scrollableNode = scrollableNodeRef.current
    if (!scrollableNode) return
    const remaining =
      scrollableNode.scrollHeight - scrollableNode.clientHeight - scrollableNode.scrollTop
    setFollowTail(remaining <= 8)
  }

  return (
    <aside className="firmware-transaction-log" aria-label="固件事务日志">
      <header className="firmware-transaction-log__header">
        <span>
          <small>FIRMWARE TRACE</small>
          <strong>固件事务日志</strong>
        </span>
        <span className="firmware-transaction-log__meta">
          {!followTail ? (
            <button
              type="button"
              className="firmware-transaction-log__tail"
              aria-label="查看最新日志"
              title="查看最新日志"
              onClick={handleFollowTail}
            >
              <ArrowDownToLine size={15} aria-hidden="true" />
            </button>
          ) : null}
          <output aria-label="日志条目数">{entries.length} 条</output>
        </span>
      </header>
      <ScrollArea
        className="firmware-transaction-log__scroll"
        scrollbarMinSize={48}
        ariaLabel="固件事务日志条目"
        scrollableNodeProps={{
          ref: scrollableNodeRef,
          'aria-live': 'polite',
          'aria-atomic': 'false',
          onScroll: handleScroll,
        }}
      >
        <ol className="firmware-transaction-log__entries">
          {entries.map((entry) => (
            <li key={entry.id} data-tone={entry.tone}>
              <time>{entry.time}</time>
              <span>
                <strong>{entry.event}</strong>
                <small>{entry.detail}</small>
              </span>
            </li>
          ))}
        </ol>
      </ScrollArea>
    </aside>
  )
}

async function firmwareStagePresentationDelay() {
  if (
    typeof window === 'undefined' ||
    window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
  ) {
    return
  }
  await new Promise<void>((resolve) => window.setTimeout(resolve, 180))
}

function firmwareStageLabel(stage: string) {
  const labels = {
    artifact: '固件包',
    transport: '连接',
    rom_reset: 'ROM 模式',
    chip_flash_security: '芯片安全',
    layout_config: '布局配置',
    preflight: '预检',
    authorization: '授权',
    erase: '擦除',
    write_segments: '写入',
    rom_md5: 'ROM 校验',
    reset: '复位',
    runtime_reconnect: '运行时重连',
    runtime_verify: '身份验证',
  } satisfies Record<FirmwareStage, string>
  return labels[stage as FirmwareStage] ?? stage
}

function compareSemver(left: string, right: string) {
  const parse = (value: string) =>
    value.replace(/^fw\//, '').replace(/^v/, '').split('-', 1)[0].split('.').map(Number)
  const a = parse(left)
  const b = parse(right)
  if (a.length !== 3 || b.length !== 3 || [...a, ...b].some((part) => !Number.isInteger(part))) {
    return 0
  }
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1
  }
  return 0
}

function outcomeLabel(outcome: FirmwareOutcome, phase: 'preflight' | 'execution') {
  switch (outcome) {
    case 'running':
      return '正在执行'
    case 'blocked':
      return '已阻止'
    case 'preflight_passed':
      return '已通过'
    case 'failed':
      return '操作失败'
    case 'write_complete_unverified':
      return '写入完成，设备未验证'
    case 'verified':
      return '已验证'
    default:
      return phase === 'preflight' ? '等待开始' : '等待写入'
  }
}
