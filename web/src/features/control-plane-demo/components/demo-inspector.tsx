import {
  CircleHelp,
  Clipboard,
  MoreHorizontal,
  PanelBottom,
  PanelLeft,
  PanelRightClose,
  PanelRightOpen,
  Radio,
  RotateCcw,
} from 'lucide-react'
import type {
  KeyboardEvent,
  MouseEvent as ReactMouseEvent,
  ReactNode,
  PointerEvent as ReactPointerEvent,
  RefObject,
} from 'react'
import { useEffect, useId, useRef, useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import {
  canUseDemoInspectorPlacement,
  type DemoInspectorPlacement,
  type DemoInspectorViewport,
  defaultDemoInspectorPlacement,
  readDemoInspectorPlacement,
  resolveDemoInspectorPlacement,
  writeDemoInspectorPlacement,
} from '../demo-inspector-preferences'
import {
  type DemoAction,
  type DemoActionKind,
  type DemoInspectorState,
  type DemoSceneId,
  demoPersonas,
} from '../demo-inspector-state'
import type { DeviceTarget } from '../types'
import { resolveWifiSettingsAccess } from '../wifi-settings-access'

interface DemoInspectorProps {
  state: DemoInspectorState
  devices: readonly DeviceTarget[]
  selectedDeviceId: string
  onStateChange: (partial: Partial<DemoInspectorState>) => void
  onSelectDevice: (deviceId: string) => void
  shareUrl: string
  clipboard?: { writeText(value: string): Promise<void> } | null
  recentActions: readonly DemoAction[]
  onSimulate: (kind: DemoActionKind) => void
  onReset: () => void | Promise<void>
}

interface DemoInspectorContentProps {
  state: DemoInspectorState
  devices: readonly DeviceTarget[]
  selectedDeviceId: string
  onStateChange: (partial: Partial<DemoInspectorState>) => void
  onSelectDevice: (deviceId: string) => void
  shareUrl: string
  recentActions: readonly DemoAction[]
  onSimulate: (kind: DemoActionKind) => void
  onReset: () => void | Promise<void>
  closeButtonRef: RefObject<HTMLButtonElement | null>
  onClose: () => void
  onCollapse: () => void
  copyShareUrl: () => Promise<void>
  copyState: 'idle' | 'success' | 'error'
  placement: DemoInspectorPlacement
  viewport: DemoInspectorViewport
  layoutMenuOpen: boolean
  onPlacementChange: (placement: DemoInspectorPlacement) => void
}

const scenes: Array<{ id: DemoSceneId; label: string; description: string }> = [
  { id: 'normal', label: 'Normal', description: 'Nominal mock bench' },
  { id: 'degraded', label: 'Degraded', description: 'Lease and handoff fault' },
  { id: 'offline', label: 'Offline', description: 'Unavailable mock target' },
  { id: 'blocked-artifact', label: 'Artifact gate', description: 'Dry-check blocked' },
  { id: 'calibration-active', label: 'Calibration', description: 'Leave guard active' },
]

const placementOptions: Array<{
  id: DemoInspectorPlacement
  label: string
  description: string
}> = [
  { id: 'left', label: '左侧', description: '占用工作区左边缘' },
  { id: 'right', label: '右侧', description: '占用工作区右边缘' },
  { id: 'bottom', label: '底部', description: '占用工作区底部' },
  { id: 'floating', label: '悬浮', description: '覆盖在工作区上方' },
]

const dockedInspectorMinViewport = 1700
const drawerMaxViewport = 1179

const actionLabels: Record<DemoActionKind, string> = {
  'thermal-warning': 'Thermal warning',
  'fan-recovery': 'Fan recovery',
}

interface DemoInspectorSelectProps {
  label: string
  ariaLabel: string
  value: string
  options: readonly { value: string; label: string; disabled?: boolean }[]
  onValueChange: (value: string) => void
}

interface DemoInspectorHelpProps {
  label: string
  title: string
  className?: string
  children: ReactNode
}

function DemoInspectorHelp({ label, title, className, children }: DemoInspectorHelpProps) {
  const [open, setOpen] = useState(false)
  const helpRef = useRef<HTMLDivElement>(null)
  const helpId = useId()

  useEffect(() => {
    if (!open) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!helpRef.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [open])

  return (
    <div
      ref={helpRef}
      className={className ? `demo-inspector__help ${className}` : 'demo-inspector__help'}
    >
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              className="demo-inspector__help-trigger"
              aria-label={label}
              aria-expanded={open}
              aria-controls={open ? helpId : undefined}
              onKeyDown={(event: KeyboardEvent<HTMLButtonElement>) => {
                if (event.key !== 'Escape' || !open) return
                event.preventDefault()
                event.stopPropagation()
                setOpen(false)
              }}
              onClick={() => setOpen((current) => !current)}
            >
              <CircleHelp size={16} aria-hidden="true" />
            </button>
          </TooltipTrigger>
          <TooltipContent className="demo-inspector__help-tooltip">{label}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
      {open ? (
        <div id={helpId} className="demo-inspector__help-panel" role="note" aria-label={title}>
          <strong>{title}</strong>
          <p>{children}</p>
        </div>
      ) : null}
    </div>
  )
}

function DemoInspectorSelect({
  label,
  ariaLabel,
  value,
  options,
  onValueChange,
}: DemoInspectorSelectProps) {
  return (
    <div className="demo-inspector__select">
      <span className="demo-inspector__select-label">{label}</span>
      <Select value={value} onValueChange={onValueChange}>
        <SelectTrigger className="demo-inspector__select-trigger" aria-label={ariaLabel}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent className="demo-inspector__select-content" position="popper">
          {options.map((option) => (
            <SelectItem
              key={option.value}
              value={option.value}
              data-value={option.value}
              disabled={option.disabled}
              className="demo-inspector__select-item"
            >
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function matchesDrawerViewport() {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia(`(max-width: ${drawerMaxViewport}px)`).matches
  )
}

function getViewport(): DemoInspectorViewport {
  if (typeof window === 'undefined') return { width: 0, height: 0 }
  return { width: window.innerWidth, height: window.innerHeight }
}

function readStoredPlacement() {
  if (typeof window === 'undefined') return defaultDemoInspectorPlacement
  try {
    return readDemoInspectorPlacement(window.localStorage)
  } catch {
    return defaultDemoInspectorPlacement
  }
}

function persistPlacement(placement: DemoInspectorPlacement) {
  if (typeof window === 'undefined') return
  try {
    writeDemoInspectorPlacement(placement, window.localStorage)
  } catch {
    writeDemoInspectorPlacement(placement)
  }
}

function targetSummary(device: DeviceTarget) {
  const wifiMode = resolveWifiSettingsAccess(device).mode
  const wifiLabel =
    wifiMode === 'read-write' ? '可配置' : wifiMode === 'read-only' ? '只读' : '不可用'
  return `${device.id === 'fp-kit-02' ? 'SIMULATED SERIAL' : device.transport.toUpperCase()} · ${device.firmware} · WiFi ${wifiLabel}`
}

function PlacementIcon({ placement }: { placement: DemoInspectorPlacement }) {
  if (placement === 'left') return <PanelLeft size={15} aria-hidden="true" />
  if (placement === 'bottom') return <PanelBottom size={15} aria-hidden="true" />
  if (placement === 'right') return <PanelRightOpen size={15} aria-hidden="true" />
  return <MoreHorizontal size={15} aria-hidden="true" />
}

function DemoInspectorContent({
  state,
  devices,
  selectedDeviceId,
  onStateChange,
  onSelectDevice,
  shareUrl,
  recentActions,
  onSimulate,
  onReset,
  closeButtonRef,
  onClose,
  onCollapse,
  copyShareUrl,
  copyState,
  placement,
  viewport,
  layoutMenuOpen,
  onPlacementChange,
}: DemoInspectorContentProps) {
  const update = (partial: Partial<DemoInspectorState>) => onStateChange(partial)
  const currentScene = scenes.find((scene) => scene.id === state.demoScene) ?? scenes[0]
  const currentTarget = devices.find((device) => device.id === selectedDeviceId) ?? devices[0]
  const persona = demoPersonas.find((item) => item.id === state.demoPersona) ?? demoPersonas[0]

  const globalSettings = (
    <fieldset className="demo-inspector__group" data-scope="global">
      <legend>Global demo settings</legend>
      <p className="demo-inspector__scope-hint">Applies across this demo session.</p>
      <DemoInspectorSelect
        label="Persona"
        ariaLabel="Persona"
        value={state.demoPersona}
        onValueChange={(value) =>
          update({ demoPersona: value as DemoInspectorState['demoPersona'] })
        }
        options={demoPersonas.map((item) => ({ value: item.id, label: item.label }))}
      />
      <DemoInspectorHelp
        className="demo-inspector__help--permission"
        label="查看当前 Persona 权限"
        title="Persona permissions"
      >
        {persona.permissions}. {persona.description}
      </DemoInspectorHelp>
    </fieldset>
  )

  const currentCase = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Current demo case</legend>
      <p className="demo-inspector__scope-hint">
        Scene, target, and simulated conditions for this route.
      </p>
      <DemoInspectorSelect
        label="Scene"
        ariaLabel="Current scene"
        value={state.demoScene}
        onValueChange={(value) => update({ demoScene: value as DemoSceneId })}
        options={scenes.map((scene) => ({ value: scene.id, label: scene.label }))}
      />
      <div className="demo-inspector__selection-summary" aria-live="polite">
        <strong>{currentScene.label}</strong>
        <span>{currentScene.description}</span>
      </div>
      <DemoInspectorSelect
        label="Target"
        ariaLabel="Current target"
        value={selectedDeviceId}
        onValueChange={onSelectDevice}
        options={devices.map((device) => ({
          value: device.id,
          label: `${device.alias} · ${targetSummary(device)}`,
        }))}
      />
      {currentTarget ? (
        <div className="demo-inspector__selection-summary">
          <strong>{currentTarget.alias}</strong>
          <span>{targetSummary(currentTarget)}</span>
        </div>
      ) : null}
    </fieldset>
  )

  const networkSafety = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Network &amp; Safety</legend>
      <label className="demo-inspector__toggle">
        <input
          type="checkbox"
          checked={state.demoLease === 'conflict'}
          onChange={(event) => update({ demoLease: event.target.checked ? 'conflict' : 'none' })}
        />{' '}
        Simulate lease conflict
      </label>
      <label className="demo-inspector__toggle">
        <input
          type="checkbox"
          checked={state.demoNetwork === 'timeout'}
          onChange={(event) =>
            update({ demoNetwork: event.target.checked ? 'timeout' : 'healthy' })
          }
        />{' '}
        Simulate network timeout
      </label>
      <label className="demo-inspector__toggle">
        <input
          type="checkbox"
          checked={state.demoArtifact === 'blocked'}
          onChange={(event) => update({ demoArtifact: event.target.checked ? 'blocked' : 'ready' })}
        />{' '}
        Block artifact dry-check
      </label>
    </fieldset>
  )

  const capabilitiesData = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Capabilities &amp; Data</legend>
      <DemoInspectorSelect
        label="WiFi capability"
        ariaLabel="WiFi capability"
        value={state.demoWifiCapability}
        onValueChange={(value) =>
          update({
            demoWifiCapability: value as DemoInspectorState['demoWifiCapability'],
          })
        }
        options={[
          { value: 'default', label: 'Default capability' },
          { value: 'read-only', label: 'Read-only WiFi' },
          { value: 'unavailable', label: 'Unavailable' },
        ]}
      />
      <DemoInspectorSelect
        label="Event log"
        ariaLabel="Event log"
        value={state.demoLogs}
        onValueChange={(value) => update({ demoLogs: value as 'normal' | 'empty' })}
        options={[
          { value: 'normal', label: 'Seeded trace' },
          { value: 'empty', label: 'Empty trace' },
        ]}
      />
      <DemoInspectorSelect
        label="Telemetry"
        ariaLabel="Telemetry"
        value={state.demoTelemetry}
        onValueChange={(value) => update({ demoTelemetry: value as 'available' | 'unavailable' })}
        options={[
          { value: 'available', label: 'Available measurements' },
          { value: 'unavailable', label: 'Measurements unavailable' },
        ]}
      />
    </fieldset>
  )

  const actions = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Data &amp; Actions</legend>
      <div className="demo-inspector__actions">
        <button
          type="button"
          className="demo-inspector__action--primary"
          disabled={state.demoPersona === 'observer'}
          onClick={() => onSimulate('thermal-warning')}
        >
          Simulate thermal warning
        </button>
        <button
          type="button"
          disabled={state.demoPersona === 'observer'}
          onClick={() => onSimulate('fan-recovery')}
        >
          Simulate fan recovery
        </button>
      </div>
      {state.demoPersona === 'observer' ? (
        <span className="demo-inspector__hint">Observer permissions keep actions read-only.</span>
      ) : null}
    </fieldset>
  )

  const history = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Recent simulated mutations</legend>
      <div className="demo-inspector__history" aria-live="polite">
        {recentActions.length === 0 ? (
          <span className="demo-inspector__hint">No simulated mutations yet.</span>
        ) : (
          recentActions
            .slice(-4)
            .reverse()
            .map((action) => {
              const target = devices.find((device) => device.id === action.targetDeviceId)
              return (
                <div
                  key={`${action.sequence}-${action.kind}-${action.targetDeviceId}`}
                  className="demo-inspector__history-item"
                >
                  <strong>{actionLabels[action.kind]}</strong>
                  <span>
                    #{action.sequence} · {target?.alias ?? action.targetDeviceId}
                  </span>
                </div>
              )
            })
        )}
      </div>
    </fieldset>
  )

  const share = (
    <fieldset className="demo-inspector__group" data-scope="case">
      <legend>Share current case</legend>
      <div className="demo-inspector__share-row">
        <input aria-label="当前 Demo 分享链接" readOnly value={shareUrl} />
        <button
          type="button"
          className="demo-inspector__icon-button"
          aria-label="复制 Demo 分享链接"
          title="复制 Demo 分享链接"
          onClick={() => void copyShareUrl()}
        >
          <Clipboard size={17} aria-hidden="true" />
        </button>
      </div>
      <span className="demo-inspector__hint">
        {copyState === 'success'
          ? 'Copied initial scene URL'
          : copyState === 'error'
            ? 'Copy failed; select the URL manually'
            : 'Share restores initial scene conditions only'}
      </span>
    </fieldset>
  )

  const reset = (
    <button type="button" className="demo-inspector__reset" onClick={() => void onReset()}>
      <RotateCcw size={15} aria-hidden="true" /> Reset current case
    </button>
  )

  return (
    <>
      <header className="demo-inspector__header">
        <div>
          <span className="demo-inspector__status">
            <Radio size={14} aria-hidden="true" /> DEMO / SIMULATED
          </span>
          <h2>Demo Inspector</h2>
        </div>
        <div className="demo-inspector__header-actions">
          <DemoInspectorHelp
            className="demo-inspector__help--header"
            label="查看 Demo Inspector 说明"
            title="Demo scope"
          >
            Local fixtures only. No device or network connection is available.
          </DemoInspectorHelp>
          <button
            ref={closeButtonRef}
            type="button"
            className="demo-inspector__icon-button"
            aria-label="收起 Demo Inspector"
            title="左键收起；右键或长按设置布局"
            onClick={onClose}
          >
            <PanelRightClose size={18} aria-hidden="true" />
          </button>
        </div>
      </header>

      {layoutMenuOpen ? (
        <div className="demo-inspector__layout-menu" role="menu" aria-label="Demo Inspector 布局">
          <span className="demo-inspector__layout-menu-label">Inspector layout</span>
          {placementOptions.map((option) => {
            const available = canUseDemoInspectorPlacement(option.id, viewport)
            return (
              <button
                key={option.id}
                type="button"
                role="menuitemradio"
                aria-checked={placement === option.id}
                disabled={!available}
                onClick={() => onPlacementChange(option.id)}
              >
                <PlacementIcon placement={option.id} />
                <span>
                  <strong>{option.label}</strong>
                  <small>{available ? option.description : '当前视口空间不足'}</small>
                </span>
              </button>
            )
          })}
          <button type="button" role="menuitem" onClick={onCollapse}>
            <PanelRightClose size={15} aria-hidden="true" />
            <span>
              <strong>收起</strong>
              <small>保留当前布局选择</small>
            </span>
          </button>
        </div>
      ) : null}

      {placement === 'bottom' && viewport.width > drawerMaxViewport ? (
        <div className="demo-inspector__bottom-columns">
          <ScrollArea className="demo-inspector__bottom-column" ariaLabel="Global demo settings">
            {globalSettings}
          </ScrollArea>
          <ScrollArea className="demo-inspector__bottom-column" ariaLabel="Current demo case">
            {currentCase}
          </ScrollArea>
          <ScrollArea className="demo-inspector__bottom-column" ariaLabel="Network and data">
            {networkSafety}
            {capabilitiesData}
          </ScrollArea>
          <ScrollArea className="demo-inspector__bottom-column" ariaLabel="Data and actions">
            {actions}
            {history}
            {share}
            {reset}
          </ScrollArea>
        </div>
      ) : (
        <ScrollArea className="demo-inspector__scroll">
          {globalSettings}
          {currentCase}
          {networkSafety}
          {capabilitiesData}
          {actions}
          {history}
          {share}
          {reset}
        </ScrollArea>
      )}
    </>
  )
}

export function DemoInspector({
  state,
  devices,
  selectedDeviceId,
  onStateChange,
  onSelectDevice,
  shareUrl,
  clipboard,
  recentActions,
  onSimulate,
  onReset,
}: DemoInspectorProps) {
  const [open, setOpen] = useState(
    () => typeof window === 'undefined' || window.innerWidth >= dockedInspectorMinViewport
  )
  const [isDrawerViewport, setIsDrawerViewport] = useState(matchesDrawerViewport)
  const [viewport, setViewport] = useState<DemoInspectorViewport>(getViewport)
  const [preferredPlacement, setPreferredPlacement] = useState(readStoredPlacement)
  const [layoutMenuOpen, setLayoutMenuOpen] = useState(false)
  const [copyState, setCopyState] = useState<'idle' | 'success' | 'error'>('idle')
  const inspectorRef = useRef<HTMLElement>(null)
  const closeButtonRef = useRef<HTMLButtonElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const longPressTimerRef = useRef<number | null>(null)
  const suppressCloseClickRef = useRef(false)
  const wasOpenRef = useRef(open)
  const placement = resolveDemoInspectorPlacement(preferredPlacement, viewport)

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    const mediaQuery = window.matchMedia(`(max-width: ${drawerMaxViewport}px)`)
    const updateViewport = () => {
      setIsDrawerViewport(mediaQuery.matches)
      setViewport(getViewport())
    }
    updateViewport()
    mediaQuery.addEventListener('change', updateViewport)
    window.addEventListener('resize', updateViewport)
    return () => {
      mediaQuery.removeEventListener('change', updateViewport)
      window.removeEventListener('resize', updateViewport)
    }
  }, [])

  useEffect(() => {
    if (!layoutMenuOpen) return
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const menu = inspectorRef.current?.querySelector('.demo-inspector__layout-menu')
      if (!menu?.contains(event.target as Node)) {
        suppressCloseClickRef.current = false
        setLayoutMenuOpen(false)
      }
    }
    document.addEventListener('pointerdown', closeOnOutsidePointer)
    return () => document.removeEventListener('pointerdown', closeOnOutsidePointer)
  }, [layoutMenuOpen])

  useEffect(() => {
    const wasOpen = wasOpenRef.current
    if (open && !wasOpen && isDrawerViewport) {
      requestAnimationFrame(() => closeButtonRef.current?.focus())
    } else if (!open && wasOpen) {
      triggerRef.current?.focus()
    }
    wasOpenRef.current = open
  }, [isDrawerViewport, open])

  useEffect(() => {
    return () => {
      if (longPressTimerRef.current !== null) window.clearTimeout(longPressTimerRef.current)
    }
  }, [])

  const copyShareUrl = async () => {
    try {
      const targetClipboard = clipboard === undefined ? navigator.clipboard : clipboard
      if (!targetClipboard) throw new Error('Clipboard API unavailable')
      await targetClipboard.writeText(shareUrl)
      setCopyState('success')
    } catch {
      setCopyState('error')
    }
  }

  const clearLongPress = () => {
    if (longPressTimerRef.current === null) return
    window.clearTimeout(longPressTimerRef.current)
    longPressTimerRef.current = null
  }

  const handlePointerDown = (event: ReactPointerEvent<HTMLElement>) => {
    const isCloseButton = closeButtonRef.current?.contains(event.target as Node) ?? false
    if (!isCloseButton || event.pointerType === 'mouse' || event.button !== 0) return
    clearLongPress()
    suppressCloseClickRef.current = false
    longPressTimerRef.current = window.setTimeout(() => {
      if (isCloseButton) suppressCloseClickRef.current = true
      setLayoutMenuOpen(true)
      longPressTimerRef.current = null
    }, 520)
  }

  const handleContextMenu = (event: ReactMouseEvent<HTMLElement>) => {
    if (!(closeButtonRef.current?.contains(event.target as Node) ?? false)) return
    event.preventDefault()
    setLayoutMenuOpen(true)
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === 'Escape' && layoutMenuOpen) {
      event.preventDefault()
      suppressCloseClickRef.current = false
      setLayoutMenuOpen(false)
      closeButtonRef.current?.focus()
      return
    }
    if (event.key === 'Escape' && open) {
      if (document.querySelector('[data-slot="select-content"][data-state="open"]')) {
        return
      }
      if (event.target instanceof Element && event.target.closest('[data-slot="select-trigger"]')) {
        return
      }
      event.preventDefault()
      setOpen(false)
      return
    }
    if (!open || !isDrawerViewport || event.key !== 'Tab') return
    const focusable = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    ).filter((element) => element.getClientRects().length > 0)
    if (focusable.length === 0) return
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault()
      last.focus()
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault()
      first.focus()
    }
  }

  const applyPlacement = (nextPlacement: DemoInspectorPlacement) => {
    if (!canUseDemoInspectorPlacement(nextPlacement, viewport)) return
    setPreferredPlacement(nextPlacement)
    persistPlacement(nextPlacement)
    suppressCloseClickRef.current = false
    setLayoutMenuOpen(false)
    requestAnimationFrame(() => closeButtonRef.current?.focus())
  }

  const closeInspector = () => {
    suppressCloseClickRef.current = false
    setLayoutMenuOpen(false)
    setOpen(false)
  }

  const handleCloseButtonClick = () => {
    if (suppressCloseClickRef.current) {
      suppressCloseClickRef.current = false
      return
    }
    closeInspector()
  }

  const inspectorContent = (
    <DemoInspectorContent
      state={state}
      devices={devices}
      selectedDeviceId={selectedDeviceId}
      onStateChange={onStateChange}
      onSelectDevice={onSelectDevice}
      shareUrl={shareUrl}
      recentActions={recentActions}
      onSimulate={onSimulate}
      onReset={onReset}
      closeButtonRef={closeButtonRef}
      onClose={handleCloseButtonClick}
      onCollapse={closeInspector}
      copyShareUrl={copyShareUrl}
      copyState={copyState}
      placement={placement}
      viewport={viewport}
      layoutMenuOpen={layoutMenuOpen}
      onPlacementChange={applyPlacement}
    />
  )

  return (
    <>
      {open && isDrawerViewport ? (
        <button
          type="button"
          className="demo-inspector__backdrop"
          aria-label="关闭 Demo Inspector"
          tabIndex={-1}
          onClick={() => setOpen(false)}
        />
      ) : null}
      <aside
        ref={inspectorRef}
        className={open ? 'demo-inspector is-open' : 'demo-inspector'}
        data-placement={placement}
        aria-label="Demo Inspector"
        onContextMenu={handleContextMenu}
        onPointerDown={handlePointerDown}
        onPointerUp={clearLongPress}
        onPointerCancel={clearLongPress}
        onPointerLeave={clearLongPress}
        onKeyDown={handleKeyDown}
      >
        {open ? (
          isDrawerViewport ? (
            <dialog
              open
              className="demo-inspector__surface"
              aria-label="Demo Inspector"
              aria-modal="true"
              onKeyDown={handleKeyDown}
            >
              {inspectorContent}
            </dialog>
          ) : (
            <section className="demo-inspector__surface">{inspectorContent}</section>
          )
        ) : (
          <button
            ref={triggerRef}
            type="button"
            className="demo-inspector__bubble"
            aria-label="打开 Demo Inspector"
            title="打开 Demo Inspector"
            onClick={() => setOpen((current) => !current)}
          >
            <PanelRightOpen size={19} aria-hidden="true" />
            <span>Demo</span>
          </button>
        )}
      </aside>
    </>
  )
}
