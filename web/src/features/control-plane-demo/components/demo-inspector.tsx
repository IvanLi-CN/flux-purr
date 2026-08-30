import { Clipboard, PanelRightClose, PanelRightOpen, Radio, RotateCcw, X } from 'lucide-react'
import { useMemo, useState } from 'react'
import {
  type DemoInspectorState,
  type DemoSceneId,
  defaultDemoInspectorState,
} from '../demo-inspector-state'
import type { DeviceTarget, EventLogEntry } from '../types'
import { resolveWifiSettingsAccess } from '../wifi-settings-access'

interface DemoInspectorProps {
  state: DemoInspectorState
  devices: readonly DeviceTarget[]
  selectedDeviceId: string
  onStateChange: (partial: Partial<DemoInspectorState>) => void
  onSelectDevice: (deviceId: string) => void
  onSimulate: (event: Pick<EventLogEntry, 'message' | 'tone'>) => void
}

const scenes: Array<{ id: DemoSceneId; label: string; description: string }> = [
  { id: 'normal', label: 'Normal', description: 'Nominal mock bench' },
  { id: 'degraded', label: 'Degraded', description: 'Lease and handoff fault' },
  { id: 'offline', label: 'Offline', description: 'Unavailable mock target' },
  { id: 'blocked-artifact', label: 'Artifact gate', description: 'Dry-check blocked' },
  { id: 'calibration-active', label: 'Calibration', description: 'Leave guard active' },
]

const dockedInspectorMinViewport = 1700

export function DemoInspector({
  state,
  devices,
  selectedDeviceId,
  onStateChange,
  onSelectDevice,
  onSimulate,
}: DemoInspectorProps) {
  const [open, setOpen] = useState(
    () => typeof window === 'undefined' || window.innerWidth >= dockedInspectorMinViewport
  )
  const [copied, setCopied] = useState(false)
  const stateSummary = useMemo(
    () =>
      `scene=${state.demoScene} lease=${state.demoLease} network=${state.demoNetwork} artifact=${state.demoArtifact}`,
    [state]
  )

  const copyState = async () => {
    const value = typeof window === 'undefined' ? stateSummary : window.location.href
    await navigator.clipboard?.writeText(value)
    setCopied(true)
    onSimulate({ message: 'Demo share URL copied to clipboard', tone: 'success' })
  }

  const update = (partial: Partial<DemoInspectorState>) => onStateChange(partial)

  return (
    <aside
      className={open ? 'demo-inspector is-open' : 'demo-inspector'}
      aria-label="Demo Inspector"
    >
      {open ? (
        <section className="demo-inspector__surface">
          <header className="demo-inspector__header">
            <div>
              <span className="demo-inspector__status">
                <Radio size={14} aria-hidden="true" /> DEMO / SIMULATED
              </span>
              <h2>Demo Inspector</h2>
              <p>Local fixtures only. No device or network connection is available.</p>
            </div>
            <button
              type="button"
              className="demo-inspector__icon-button"
              aria-label="收起 Demo Inspector"
              title="收起 Demo Inspector"
              onClick={() => setOpen(false)}
            >
              <PanelRightClose size={18} aria-hidden="true" />
            </button>
          </header>

          <fieldset className="demo-inspector__group">
            <legend>Scene</legend>
            <div className="demo-inspector__scene-grid">
              {scenes.map((scene) => (
                <button
                  key={scene.id}
                  type="button"
                  className={
                    state.demoScene === scene.id
                      ? 'demo-inspector__scene is-selected'
                      : 'demo-inspector__scene'
                  }
                  aria-pressed={state.demoScene === scene.id}
                  onClick={() => update({ demoScene: scene.id })}
                >
                  <strong>{scene.label}</strong>
                  <span>{scene.description}</span>
                </button>
              ))}
            </div>
          </fieldset>

          <fieldset className="demo-inspector__group">
            <legend>Target</legend>
            <div className="demo-inspector__targets">
              {devices.map((device) => (
                <button
                  key={device.id}
                  type="button"
                  className={
                    selectedDeviceId === device.id
                      ? 'demo-inspector__target is-selected'
                      : 'demo-inspector__target'
                  }
                  aria-pressed={selectedDeviceId === device.id}
                  onClick={() => onSelectDevice(device.id)}
                >
                  <strong>{device.alias}</strong>
                  <span>
                    {device.id === 'fp-kit-02'
                      ? 'SIMULATED SERIAL'
                      : device.transport.toUpperCase()}{' '}
                    · {device.firmware} · WiFi{' '}
                    {resolveWifiSettingsAccess(device).mode === 'read-write'
                      ? '可配置'
                      : resolveWifiSettingsAccess(device).mode === 'read-only'
                        ? '只读'
                        : '不可用'}
                  </span>
                </button>
              ))}
            </div>
          </fieldset>

          <fieldset className="demo-inspector__group">
            <legend>Network &amp; Safety</legend>
            <label className="demo-inspector__toggle">
              <input
                type="checkbox"
                checked={state.demoLease === 'conflict'}
                onChange={(event) =>
                  update({ demoLease: event.target.checked ? 'conflict' : 'none' })
                }
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
                onChange={(event) =>
                  update({ demoArtifact: event.target.checked ? 'blocked' : 'ready' })
                }
              />{' '}
              Block artifact dry-check
            </label>
          </fieldset>

          <fieldset className="demo-inspector__group">
            <legend>Data &amp; Actions</legend>
            <div className="demo-inspector__actions">
              <button
                type="button"
                onClick={() =>
                  onSimulate({ message: 'simulated thermal warning acknowledged', tone: 'warning' })
                }
              >
                Simulate thermal warning
              </button>
              <button
                type="button"
                onClick={() =>
                  onSimulate({ message: 'simulated fan policy settled at AUTO', tone: 'success' })
                }
              >
                Simulate fan recovery
              </button>
            </div>
          </fieldset>

          <fieldset className="demo-inspector__group">
            <legend>Share</legend>
            <div className="demo-inspector__share-row">
              <input aria-label="当前 Demo 状态" readOnly value={stateSummary} />
              <button
                type="button"
                className="demo-inspector__icon-button"
                aria-label="复制 Demo 分享链接"
                title="复制 Demo 分享链接"
                onClick={() => void copyState()}
              >
                <Clipboard size={17} aria-hidden="true" />
              </button>
            </div>
            <span className="demo-inspector__hint">
              {copied ? 'Copied current URL' : 'URL updates without reloading'}
            </span>
          </fieldset>

          <button
            type="button"
            className="demo-inspector__reset"
            onClick={() => onStateChange(defaultDemoInspectorState)}
          >
            <RotateCcw size={15} aria-hidden="true" /> Reset demo state
          </button>
        </section>
      ) : (
        <button
          type="button"
          className="demo-inspector__bubble"
          aria-label="打开 Demo Inspector"
          title="打开 Demo Inspector"
          onClick={() => setOpen(true)}
        >
          <PanelRightOpen size={19} aria-hidden="true" />
          <span>Demo</span>
        </button>
      )}
      {open ? (
        <button
          type="button"
          className="demo-inspector__mobile-close"
          aria-label="关闭 Demo Inspector"
          title="关闭 Demo Inspector"
          onClick={() => setOpen(false)}
        >
          <X size={18} aria-hidden="true" />
        </button>
      ) : null}
    </aside>
  )
}
