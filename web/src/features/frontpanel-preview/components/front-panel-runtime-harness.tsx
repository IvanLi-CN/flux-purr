import { useEffect, useMemo, useState } from 'react'
import {
  applyFrontPanelInteraction,
  buildRuntimeScreenSnapshot,
  type FrontPanelRuntimeInteraction,
  type FrontPanelRuntimeMode,
  type FrontPanelRuntimeState,
  frontPanelRuntimeToScreen,
  tickFrontPanelRuntime,
} from '../runtime'
import type { FrontPanelKeyId, KeyGestureId } from '../types'
import { FrontPanelDisplay } from './front-panel-display'

interface FrontPanelRuntimeHarnessProps {
  initialState?: FrontPanelRuntimeState
  mode?: FrontPanelRuntimeMode
  scale?: number
}

const keyOrder: ReadonlyArray<FrontPanelKeyId> = ['up', 'down', 'left', 'right', 'center']
const gestureOrder: ReadonlyArray<KeyGestureId> = ['short', 'double', 'long']
const RUNTIME_TICK_MS = 100
const COOLING_DISABLED_PULSE_START_TEMP_C = 100
const COOLING_DISABLED_HEATER_LOCK_TEMP_C = 350

const shortcuts: ReadonlyArray<FrontPanelRuntimeInteraction> = keyOrder.flatMap((key) =>
  gestureOrder.map((gesture) => ({ key, gesture }))
)

function interactionLabel(key: FrontPanelKeyId, gesture: KeyGestureId) {
  return `${key} ${gesture}`
}

export function FrontPanelRuntimeHarness({
  initialState,
  mode = 'app',
  scale = 5,
}: FrontPanelRuntimeHarnessProps) {
  const seedState = useMemo(
    () => initialState ?? buildRuntimeScreenSnapshot(mode),
    [initialState, mode]
  )
  const [state, setState] = useState<FrontPanelRuntimeState>(seedState)

  useEffect(() => {
    setState(seedState)
  }, [seedState])

  useEffect(() => {
    const needsPulseTick =
      state.currentTempC > COOLING_DISABLED_PULSE_START_TEMP_C &&
      state.currentTempC <= COOLING_DISABLED_HEATER_LOCK_TEMP_C &&
      (state.heaterEnabled || !state.activeCoolingEnabled)
    const needsTick = state.heaterLockReason != null || needsPulseTick

    if (!needsTick) {
      return undefined
    }

    const intervalId = window.setInterval(() => {
      setState((current) => tickFrontPanelRuntime(current, RUNTIME_TICK_MS))
    }, RUNTIME_TICK_MS)

    return () => window.clearInterval(intervalId)
  }, [state.activeCoolingEnabled, state.currentTempC, state.heaterEnabled, state.heaterLockReason])

  const screen = useMemo(() => frontPanelRuntimeToScreen(state), [state])

  return (
    <div
      className="grid gap-6 lg:grid-cols-[auto_minmax(320px,1fr)]"
      data-testid="frontpanel-runtime-harness"
    >
      <FrontPanelDisplay screen={screen} scale={scale} />

      <div className="grid gap-4 rounded-3xl border border-slate-700/70 bg-slate-950/90 p-5 text-slate-100 shadow-[0_20px_60px_rgba(2,6,23,0.35)]">
        <div className="grid gap-2">
          <h3 className="text-lg font-semibold">Interaction driver</h3>
          <p className="text-sm text-slate-400">
            Deterministic mock controls for Storybook play coverage and local review.
          </p>
        </div>

        <div className="grid gap-2 sm:grid-cols-3">
          {shortcuts.map((interaction) => {
            const label = interactionLabel(interaction.key, interaction.gesture)
            return (
              <button
                key={label}
                type="button"
                className="rounded-2xl border border-slate-700 bg-slate-900 px-4 py-3 text-left text-sm font-medium text-slate-100 transition hover:border-cyan-400/70 hover:text-cyan-200"
                data-testid={`frontpanel-action-${interaction.key}-${interaction.gesture}`}
                onClick={() =>
                  setState((current) => applyFrontPanelInteraction(current, interaction))
                }
              >
                {label}
              </button>
            )
          })}
        </div>

        <div
          className="grid gap-2 rounded-2xl border border-slate-800 bg-slate-900/80 p-4 text-sm"
          data-testid="frontpanel-runtime-debug"
        >
          <div>
            <span className="text-slate-400">route:</span> {state.route}
          </div>
          <div>
            <span className="text-slate-400">targetTempC:</span> {state.targetTempC}
          </div>
          <div>
            <span className="text-slate-400">heaterEnabled:</span> {String(state.heaterEnabled)}
          </div>
          <div>
            <span className="text-slate-400">heaterOutputPercent:</span> {state.heaterOutputPercent}
          </div>
          <div>
            <span className="text-slate-400">fanRuntimeEnabled:</span>{' '}
            {String(state.fanRuntimeEnabled)}
          </div>
          <div>
            <span className="text-slate-400">fanDisplayState:</span> {state.fanDisplayState}
          </div>
          <div>
            <span className="text-slate-400">selectedMenuItem:</span> {state.selectedMenuItem}
          </div>
          <div>
            <span className="text-slate-400">selectedPresetIndex:</span> {state.selectedPresetIndex}
          </div>
          <div>
            <span className="text-slate-400">activeCoolingEnabled:</span>{' '}
            {String(state.activeCoolingEnabled)}
          </div>
          <div>
            <span className="text-slate-400">heaterLockReason:</span>{' '}
            {state.heaterLockReason ?? 'none'}
          </div>
          <div>
            <span className="text-slate-400">warningVisible:</span>{' '}
            {String(state.dashboardWarningVisible)}
          </div>
          <div>
            <span className="text-slate-400">pdContractMv:</span> {state.pdContractMv}
          </div>
          <div>
            <span className="text-slate-400">keyTest:</span> {state.keyTest.rawKeyLabel} /{' '}
            {state.keyTest.logicalKeyLabel} / {state.keyTest.gestureLabel}
          </div>
        </div>
      </div>
    </div>
  )
}
