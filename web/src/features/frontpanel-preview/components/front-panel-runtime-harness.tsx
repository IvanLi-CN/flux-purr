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

const RUNTIME_TICK_MS = 100
const COOLING_DISABLED_PULSE_START_TEMP_C = 100
const COOLING_DISABLED_HEATER_LOCK_TEMP_C = 350

const gestureOptions: ReadonlyArray<{ id: KeyGestureId; label: string }> = [
  { id: 'short', label: 'Tap' },
  { id: 'double', label: 'Double' },
  { id: 'long', label: 'Hold' },
  { id: 'repeat', label: 'Repeat' },
]

const switchKeys: ReadonlyArray<{
  id: FrontPanelKeyId
  label: string
  className: string
  symbol: string
}> = [
  { id: 'up', label: 'Up', className: 'col-start-2 row-start-1', symbol: 'U' },
  { id: 'left', label: 'Left', className: 'col-start-1 row-start-2', symbol: 'L' },
  { id: 'center', label: 'Center', className: 'col-start-2 row-start-2', symbol: 'C' },
  { id: 'right', label: 'Right', className: 'col-start-3 row-start-2', symbol: 'R' },
  { id: 'down', label: 'Down', className: 'col-start-2 row-start-3', symbol: 'D' },
]

function canApplyGesture(key: FrontPanelKeyId, gesture: KeyGestureId) {
  return gesture !== 'repeat' || key === 'up' || key === 'down'
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
  const [selectedGesture, setSelectedGesture] = useState<KeyGestureId>('short')

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

  function applyKey(key: FrontPanelKeyId) {
    if (!canApplyGesture(key, selectedGesture)) return

    const interaction = {
      key,
      gesture: selectedGesture,
    } satisfies FrontPanelRuntimeInteraction

    setState((current) => applyFrontPanelInteraction(current, interaction))
  }

  return (
    <div
      className="frontpanel-arcade-shell grid gap-6 p-4 sm:p-6 2xl:grid-cols-[minmax(0,1fr)_minmax(320px,0.36fr)]"
      data-testid="frontpanel-runtime-harness"
    >
      <div className="frontpanel-crt-stage">
        <div className="frontpanel-marquee">FLUX PURR CRT BAY</div>
        <div className="frontpanel-crt-screen">
          <FrontPanelDisplay screen={screen} scale={scale} className="frontpanel-crt-display" />
        </div>
        <div className="frontpanel-coin-slot" aria-hidden="true">
          INSERT COIN
        </div>
      </div>

      <div className="frontpanel-control-deck">
        <div className="grid gap-1">
          <div className="frontpanel-panel-kicker">NEON INPUT RIG</div>
          <h3 className="text-xl font-bold text-[oklch(0.94_0.04_205)]">Five-way switch</h3>
          <p className="max-w-[54ch] text-sm leading-6 text-[oklch(0.72_0.05_230)]">
            One physical control surface drives the front-panel runtime, with repeat locked to the
            vertical temperature path.
          </p>
        </div>

        <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_auto]">
          <div className="grid content-start gap-2">
            <div className="frontpanel-section-label">Gesture</div>
            <div className="grid grid-cols-2 gap-2">
              {gestureOptions.map((gesture) => {
                const isSelected = gesture.id === selectedGesture
                return (
                  <button
                    key={gesture.id}
                    type="button"
                    className={[
                      'frontpanel-gesture-button',
                      isSelected ? 'frontpanel-gesture-button-active' : '',
                    ].join(' ')}
                    data-testid={`frontpanel-gesture-${gesture.id}`}
                    aria-pressed={isSelected}
                    onClick={() => setSelectedGesture(gesture.id)}
                  >
                    {gesture.label}
                  </button>
                )
              })}
            </div>
          </div>

          <div className="grid justify-items-center gap-2">
            <div className="frontpanel-section-label">Cabinet controls</div>
            <div className="frontpanel-switch-plate grid grid-cols-3 grid-rows-3 gap-2">
              {switchKeys.map((switchKey) => {
                const disabled = !canApplyGesture(switchKey.id, selectedGesture)
                return (
                  <button
                    key={switchKey.id}
                    type="button"
                    className={[
                      switchKey.className,
                      'frontpanel-switch-key',
                      disabled ? 'frontpanel-switch-key-disabled' : '',
                    ].join(' ')}
                    data-testid={`frontpanel-switch-${switchKey.id}`}
                    aria-label={`${switchKey.label} ${selectedGesture}`}
                    disabled={disabled}
                    onClick={() => applyKey(switchKey.id)}
                  >
                    <span aria-hidden="true" className="text-sm font-bold">
                      {switchKey.symbol}
                    </span>
                  </button>
                )
              })}
            </div>
          </div>
        </div>

        <div
          className="frontpanel-debug-readout grid gap-2 text-sm"
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
