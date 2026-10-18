import { Clipboard, PanelRightClose, PanelRightOpen, Radio, RotateCcw, X } from 'lucide-react'
import { useState } from 'react'
import { ScrollArea } from '@/components/ui/scroll-area'
import type { DemoActionKind, DemoInspectorState, DemoSceneId } from '../demo-inspector-state'
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
  onSimulate: (kind: DemoActionKind) => void
  onReset: () => void | Promise<void>
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
  shareUrl,
  clipboard,
  onSimulate,
  onReset,
}: DemoInspectorProps) {
  const [open, setOpen] = useState(
    () => typeof window === 'undefined' || window.innerWidth >= dockedInspectorMinViewport
  )
  const [copyState, setCopyState] = useState<'idle' | 'success' | 'error'>('idle')

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

          <ScrollArea className="demo-inspector__scroll">
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
              <legend>Capabilities &amp; Data</legend>
              <label className="demo-inspector__select">
                WiFi capability
                <select
                  value={state.demoWifiCapability}
                  onChange={(event) =>
                    update({
                      demoWifiCapability: event.target
                        .value as DemoInspectorState['demoWifiCapability'],
                    })
                  }
                >
                  <option value="default">Default capability</option>
                  <option value="read-only">Read-only WiFi</option>
                  <option value="unavailable">Unavailable</option>
                </select>
              </label>
              <label className="demo-inspector__select">
                Event log
                <select
                  value={state.demoLogs}
                  onChange={(event) =>
                    update({ demoLogs: event.target.value as 'normal' | 'empty' })
                  }
                >
                  <option value="normal">Seeded trace</option>
                  <option value="empty">Empty trace</option>
                </select>
              </label>
              <label className="demo-inspector__select">
                Telemetry
                <select
                  value={state.demoTelemetry}
                  onChange={(event) =>
                    update({ demoTelemetry: event.target.value as 'available' | 'unavailable' })
                  }
                >
                  <option value="available">Available measurements</option>
                  <option value="unavailable">Measurements unavailable</option>
                </select>
              </label>
            </fieldset>

            <fieldset className="demo-inspector__group">
              <legend>Data &amp; Actions</legend>
              <div className="demo-inspector__actions">
                <button type="button" onClick={() => onSimulate('thermal-warning')}>
                  Simulate thermal warning
                </button>
                <button type="button" onClick={() => onSimulate('fan-recovery')}>
                  Simulate fan recovery
                </button>
              </div>
            </fieldset>

            <fieldset className="demo-inspector__group">
              <legend>Share</legend>
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

            <button type="button" className="demo-inspector__reset" onClick={() => void onReset()}>
              <RotateCcw size={15} aria-hidden="true" /> Reset demo state
            </button>
          </ScrollArea>
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
