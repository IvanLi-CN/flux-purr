import { Download, FileUp, RotateCcw, Server, ShieldCheck, Usb, Zap } from 'lucide-react'
import { useMemo, useState } from 'react'

import {
  connectBrowserLoader,
  preflightBrowserLayout,
  preflightBrowserTarget,
  verifyBrowserRuntime,
  writeBrowserBundle,
} from './browser-esptool'
import { validateFirmwareBundle } from './bundle'
import { fetchOfficialBundle } from './release-catalog'
import { firmwareStages } from './state-machine'
import type {
  FirmwareChannel,
  FirmwareOperation,
  FirmwareOutcome,
  FirmwareTransport,
  ValidatedFirmwareBundle,
} from './types'

export interface FirmwareWorkbenchProps {
  devdAvailable: boolean
  browserAvailable: boolean
  updateEligible: boolean
  currentVersion?: string
  currentTemperatureC?: number
  heaterEnabled?: boolean
  busy?: boolean
  outcome?: FirmwareOutcome
  progress?: number
  message?: string
  devdBaseUrl?: string | null
  deviceId?: string
  leaseId?: string | null
  onPreflight: (operation: FirmwareOperation, transport: FirmwareTransport) => void
  onInstall: (operation: FirmwareOperation, transport: FirmwareTransport) => void
}

export function FirmwareWorkbench({
  devdAvailable,
  browserAvailable,
  updateEligible,
  currentVersion,
  currentTemperatureC,
  heaterEnabled,
  busy = false,
  outcome = 'idle',
  progress = 0,
  message = '选择任务和固件来源后运行完整预检。',
  devdBaseUrl,
  deviceId,
  leaseId,
}: FirmwareWorkbenchProps) {
  const [operation, setOperation] = useState<FirmwareOperation>(
    updateEligible ? 'update' : 'install_recovery'
  )
  const [transport, setTransport] = useState<FirmwareTransport>(devdAvailable ? 'devd' : 'browser')
  const [channel, setChannel] = useState<FirmwareChannel>('stable')
  const [localBundle, setLocalBundle] = useState<ValidatedFirmwareBundle | null>(null)
  const [localBundleBytes, setLocalBundleBytes] = useState<Uint8Array | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const [browserLoader, setBrowserLoader] = useState<Awaited<
    ReturnType<typeof connectBrowserLoader>
  > | null>(null)
  const [browserOutcome, setBrowserOutcome] = useState<FirmwareOutcome | null>(null)
  const [browserProgress, setBrowserProgress] = useState(0)
  const [browserMessage, setBrowserMessage] = useState<string | null>(null)
  const [devdArtifactId, setDevdArtifactId] = useState<string | null>(null)
  const [approvalToken, setApprovalToken] = useState<string | null>(null)
  const [allowDowngrade, setAllowDowngrade] = useState(false)
  const effectiveOutcome = browserOutcome ?? outcome
  const effectiveProgress = browserOutcome ? browserProgress : progress
  const effectiveMessage = browserMessage ?? message
  const stages = useMemo(() => firmwareStages(operation), [operation])
  const activeStageIndex = firmwareStageIndex(stages.length, effectiveOutcome, effectiveProgress)
  const transportAvailable = transport === 'devd' ? devdAvailable : browserAvailable
  const canRun = transportAvailable && !busy && (channel !== 'local' || localBundle !== null)

  const selectedBundle = async () => {
    if (channel === 'local') {
      if (!localBundle || !localBundleBytes) throw new Error('请选择有效的本地 .fluxpurr-fw。')
      return { bundle: localBundle, bytes: localBundleBytes }
    }
    setBrowserMessage(`正在读取官方 ${channel.toUpperCase()} 固件目录。`)
    return fetchOfficialBundle(channel)
  }

  const runPreflight = async () => {
    if (operation === 'update') {
      if (
        !updateEligible ||
        heaterEnabled !== false ||
        typeof currentTemperatureC !== 'number' ||
        !Number.isFinite(currentTemperatureC) ||
        currentTemperatureC > 40
      ) {
        setBrowserOutcome('blocked')
        setBrowserMessage('更新要求已验证 Flux Purr 运行时、加热关闭且有效温度不高于 40 C。')
        return
      }
    }
    if (transport === 'devd') {
      if (!devdBaseUrl || !deviceId || !leaseId) {
        setBrowserOutcome('blocked')
        setBrowserMessage('devd 预检需要在线设备和有效租约。')
        return
      }
      setBrowserOutcome('running')
      setBrowserProgress(15)
      setBrowserMessage('正在通过 devd 导入并验证固件包。')
      try {
        const selected = await selectedBundle()
        const imported = await fetch(`${devdBaseUrl}/api/v1/firmware-bundles`, {
          method: 'POST',
          headers: { 'content-type': 'application/vnd.flux-purr.firmware-bundle+zip' },
          body: Uint8Array.from(selected.bytes).buffer,
        })
        if (!imported.ok) throw new Error(`devd bundle import failed (${imported.status}).`)
        const artifactId = ((await imported.json()) as { artifactId: string }).artifactId
        setDevdArtifactId(artifactId)
        const response = await fetch(
          `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/firmware`,
          {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ leaseId, artifactId, operation, dryRun: true, allowDowngrade }),
          }
        )
        const result = (await response.json()) as { approvalToken?: string; message?: string }
        if (!response.ok || !result.approvalToken)
          throw new Error(result.message ?? 'devd preflight failed.')
        setApprovalToken(result.approvalToken)
        setBrowserOutcome('preflight_passed')
        setBrowserProgress(100)
        setBrowserMessage('devd 完整预检已通过；授权令牌五分钟内单次有效。')
      } catch (error) {
        setApprovalToken(null)
        setBrowserOutcome('blocked')
        setBrowserProgress(0)
        setBrowserMessage(error instanceof Error ? error.message : 'devd preflight failed.')
      }
      return
    }
    setBrowserOutcome('running')
    setBrowserProgress(20)
    setBrowserMessage('正在连接 ESP32-S3 ROM；必要时按住 BOOT 后点按 RESET。')
    try {
      const selected = await selectedBundle()
      if (
        operation === 'update' &&
        currentVersion &&
        compareSemver(selected.bundle.manifest.identity.version, currentVersion) < 0 &&
        !allowDowngrade
      ) {
        throw new Error('目标固件版本更旧；必须先启用高级降级确认。')
      }
      const loader = await connectBrowserLoader()
      await preflightBrowserTarget(loader)
      await preflightBrowserLayout(loader, selected.bundle, operation)
      setLocalBundle(selected.bundle)
      setLocalBundleBytes(selected.bytes)
      setBrowserLoader(loader)
      setBrowserOutcome('preflight_passed')
      setBrowserProgress(100)
      setBrowserMessage('ROM、Flash 容量和安全状态已通过；可开始完整写入。')
    } catch (error) {
      setBrowserLoader(null)
      setBrowserOutcome('blocked')
      setBrowserProgress(0)
      setBrowserMessage(error instanceof Error ? error.message : 'Browser ROM preflight failed.')
    }
  }

  const runInstall = async () => {
    if (transport === 'devd') {
      if (!devdBaseUrl || !deviceId || !leaseId || !devdArtifactId || !approvalToken) return
      setBrowserOutcome('running')
      setBrowserProgress(5)
      setBrowserMessage('devd 已取得串口独占，正在执行受保护的固件事务。')
      try {
        const response = await fetch(
          `${devdBaseUrl}/api/v1/devices/${encodeURIComponent(deviceId)}/firmware`,
          {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({
              leaseId,
              artifactId: devdArtifactId,
              operation,
              dryRun: false,
              approvalToken,
              confirm: operation === 'update' ? 'FLASH' : 'ERASE_INSTALL',
              allowDowngrade,
            }),
          }
        )
        const result = (await response.json()) as { outcome?: FirmwareOutcome; message?: string }
        if (!response.ok) throw new Error(result.message ?? 'devd firmware transaction failed.')
        setBrowserOutcome(result.outcome ?? 'failed')
        setBrowserProgress(100)
        setBrowserMessage(result.message ?? 'devd firmware transaction completed.')
        setApprovalToken(null)
      } catch (error) {
        setApprovalToken(null)
        setBrowserOutcome('failed')
        setBrowserProgress(0)
        setBrowserMessage(
          error instanceof Error ? error.message : 'devd firmware transaction failed.'
        )
      }
      return
    }
    if (!browserLoader || !localBundle) return
    setBrowserOutcome('running')
    setBrowserProgress(1)
    setBrowserMessage('写入期间请保持 USB 连接；中断后必须重新运行完整预检。')
    try {
      await writeBrowserBundle(browserLoader, localBundle, operation, (_index, written, total) => {
        setBrowserProgress(Math.max(1, Math.round((written / Math.max(total, 1)) * 90)))
      })
      setBrowserOutcome('write_complete_unverified')
      setBrowserProgress(95)
      setBrowserMessage('ROM MD5 已通过，正在等待 Flux Purr 运行时身份与安装状态验证。')
      try {
        await verifyBrowserRuntime(browserLoader, localBundle)
        setBrowserOutcome('verified')
        setBrowserProgress(100)
        setBrowserMessage('固件字节、运行时身份、布局与安装状态均已验证。')
      } catch (error) {
        setBrowserMessage(
          error instanceof Error
            ? `写入已完成，但运行时未验证：${error.message}`
            : '写入已完成，但运行时未验证。'
        )
      }
    } catch (error) {
      setBrowserLoader(null)
      setBrowserOutcome('failed')
      setBrowserProgress(0)
      setBrowserMessage(error instanceof Error ? error.message : 'Browser firmware write failed.')
    }
  }

  const resetAuthorization = () => {
    if (browserLoader) void browserLoader.transport.disconnect().catch(() => undefined)
    setBrowserLoader(null)
    setApprovalToken(null)
    setDevdArtifactId(null)
    setBrowserOutcome(null)
    setBrowserMessage(null)
    setBrowserProgress(0)
  }

  const chooseOperation = (next: FirmwareOperation) => {
    resetAuthorization()
    setOperation(next)
  }

  const importLocal = async (file?: File) => {
    setLocalBundle(null)
    setLocalBundleBytes(null)
    setLocalError(null)
    if (!file) return
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      setLocalBundle(await validateFirmwareBundle(bytes))
      setLocalBundleBytes(bytes)
    } catch (error) {
      setLocalError(error instanceof Error ? error.message : '固件包校验失败。')
    }
  }

  const downloadDiagnostic = () => {
    const report = {
      schemaVersion: 1,
      operation,
      transport,
      channel,
      outcome: effectiveOutcome,
      progress: effectiveProgress,
      message: effectiveMessage,
      bundleSha256: localBundle?.bundleSha256 ?? null,
      generatedAt: new Date().toISOString(),
    }
    const url = URL.createObjectURL(
      new Blob([`${JSON.stringify(report, null, 2)}\n`], { type: 'application/json' })
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
          disabled={!updateEligible || busy}
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
          <span>{transport === 'devd' ? '本机受保护事务' : '浏览器直接连接 ROM'}</span>
        </div>
        <fieldset>
          <legend>连接引擎</legend>
          <label>
            <input
              type="radio"
              name="firmwareTransport"
              checked={transport === 'devd'}
              disabled={!devdAvailable || busy}
              onChange={() => {
                resetAuthorization()
                setTransport('devd')
              }}
            />
            <Server size={16} aria-hidden="true" />
            <span>
              本机 devd<small>{devdAvailable ? '推荐' : '不可用'}</small>
            </span>
          </label>
          <label>
            <input
              type="radio"
              name="firmwareTransport"
              checked={transport === 'browser'}
              disabled={!browserAvailable || busy}
              onChange={() => {
                resetAuthorization()
                setTransport('browser')
              }}
            />
            <Usb size={16} aria-hidden="true" />
            <span>
              浏览器 USB<small>{browserAvailable ? 'Chrome / Edge' : '不可用'}</small>
            </span>
          </label>
        </fieldset>

        <label className="firmware-workbench__source">
          <span>固件来源</span>
          <select
            value={channel}
            disabled={busy}
            aria-label="固件来源"
            onChange={(event) => {
              resetAuthorization()
              setChannel(event.currentTarget.value as FirmwareChannel)
            }}
          >
            <option value="stable">最新稳定版</option>
            <option value="rc">候选版（RC）</option>
            <option value="local">本地 .fluxpurr-fw</option>
          </select>
        </label>

        {channel === 'local' ? (
          <label className="firmware-workbench__file">
            <FileUp size={17} aria-hidden="true" />
            <span>{localBundle?.manifest.identity.version ?? '选择 .fluxpurr-fw'}</span>
            <input
              type="file"
              accept=".fluxpurr-fw,application/vnd.flux-purr.firmware-bundle+zip"
              disabled={busy}
              onChange={(event) => void importLocal(event.currentTarget.files?.[0])}
            />
          </label>
        ) : null}
      </div>

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

      {operation === 'install_recovery' ? (
        <p className="firmware-workbench__erase-notice">
          <Zap size={15} aria-hidden="true" />
          MCU Flash 将完整擦除；外置 EEPROM 不在擦除范围内。
        </p>
      ) : null}

      {localError ? (
        <p className="firmware-workbench__error" role="alert">
          {localError}
        </p>
      ) : null}

      <div className="firmware-workbench__status" data-outcome={effectiveOutcome}>
        <div className="firmware-workbench__status-heading">
          <span className="firmware-workbench__status-icon">
            <ShieldCheck size={20} aria-hidden="true" />
          </span>
          <span>
            <strong>{outcomeLabel(effectiveOutcome)}</strong>
            <small>{effectiveMessage}</small>
          </span>
          <output aria-label="固件操作进度">{Math.round(effectiveProgress)}%</output>
        </div>
        <div
          className="firmware-workbench__progress"
          role="progressbar"
          aria-label="固件操作进度"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={effectiveProgress}
        >
          <span
            style={{
              transform: `scaleX(${Math.max(0, Math.min(effectiveProgress, 100)) / 100})`,
            }}
          />
        </div>
        <ol aria-label="固件操作阶段">
          {stages.map((stage, index) => (
            <li
              key={stage}
              data-state={
                index < activeStageIndex
                  ? 'done'
                  : index === activeStageIndex
                    ? 'active'
                    : 'pending'
              }
            >
              <span aria-hidden="true" />
              {firmwareStageLabel(stage)}
            </li>
          ))}
        </ol>
      </div>

      <div className="firmware-workbench__actions">
        <button
          type="button"
          className={`industrial-button ${effectiveOutcome === 'preflight_passed' ? 'industrial-button--secondary' : 'industrial-button--primary'}`}
          disabled={!canRun}
          onClick={() => void runPreflight()}
        >
          <ShieldCheck size={17} aria-hidden="true" />
          运行预检
        </button>
        <button
          type="button"
          className={`industrial-button firmware-workbench__install ${effectiveOutcome === 'preflight_passed' ? 'industrial-button--primary' : 'industrial-button--secondary'}`}
          disabled={!canRun || effectiveOutcome !== 'preflight_passed'}
          onClick={() => void runInstall()}
        >
          <Zap size={17} aria-hidden="true" />
          {operation === 'update' ? '开始更新' : '擦除并安装'}
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

function firmwareStageIndex(length: number, outcome: FirmwareOutcome, progress: number) {
  if (outcome === 'verified') return length - 1
  if (outcome === 'write_complete_unverified') return Math.max(0, length - 2)
  if (outcome === 'preflight_passed') return Math.min(length - 1, 5)
  if (outcome === 'running') {
    return Math.min(length - 1, Math.max(0, Math.floor((progress / 100) * length)))
  }
  return 0
}

function firmwareStageLabel(stage: ReturnType<typeof firmwareStages>[number]) {
  const labels = {
    artifact: '固件包',
    transport: '连接',
    rom_reset: 'ROM 模式',
    chip_flash_security: '芯片安全',
    layout_config: '布局配置',
    preflight: '预检',
    erase: '擦除',
    write_segments: '写入',
    rom_md5: 'ROM 校验',
    reset: '复位',
    runtime_reconnect: '运行时重连',
    runtime_verify: '身份验证',
  } satisfies Record<ReturnType<typeof firmwareStages>[number], string>
  return labels[stage]
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

function outcomeLabel(outcome: FirmwareOutcome) {
  switch (outcome) {
    case 'running':
      return '正在执行'
    case 'blocked':
      return '预检阻止'
    case 'preflight_passed':
      return '预检已通过'
    case 'failed':
      return '操作失败'
    case 'write_complete_unverified':
      return '写入完成，设备未验证'
    case 'verified':
      return '安装已验证'
    default:
      return '等待预检'
  }
}
