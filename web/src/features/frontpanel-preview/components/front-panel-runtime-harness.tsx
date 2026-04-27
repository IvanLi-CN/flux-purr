import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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
const DOUBLE_PRESS_WINDOW_MS = 300
const LONG_PRESS_MS = 500
const REPEAT_INITIAL_INTERVAL_MS = 200
const REPEAT_FAST_AFTER_MS = 1_500
const REPEAT_FAST_INTERVAL_MS = 100

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

interface ActivePress {
  key: FrontPanelKeyId
  startedAt: number
  longFired: boolean
}

interface PendingShortPress {
  key: FrontPanelKeyId
  timeoutId: number
}

function keyFromKeyboardEvent(event: KeyboardEvent): FrontPanelKeyId | null {
  switch (event.key.toLowerCase()) {
    case 'arrowup':
    case 'w':
      return 'up'
    case 'arrowdown':
    case 's':
      return 'down'
    case 'arrowleft':
    case 'a':
      return 'left'
    case 'arrowright':
    case 'd':
      return 'right'
    case ' ':
    case 'enter':
      return 'center'
    default:
      return null
  }
}

function isRepeatKey(key: FrontPanelKeyId) {
  return key === 'up' || key === 'down'
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
  const [activeKey, setActiveKey] = useState<FrontPanelKeyId | null>(null)
  const [lastGesture, setLastGesture] = useState<KeyGestureId | null>(null)
  const activePressRef = useRef<ActivePress | null>(null)
  const longTimeoutRef = useRef<number | null>(null)
  const repeatTimeoutRef = useRef<number | null>(null)
  const pendingShortRef = useRef<PendingShortPress | null>(null)

  useEffect(() => {
    setState(seedState)
    setActiveKey(null)
    setLastGesture(null)
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

  const clearLongTimer = useCallback(() => {
    if (longTimeoutRef.current == null) return
    window.clearTimeout(longTimeoutRef.current)
    longTimeoutRef.current = null
  }, [])

  const clearRepeatTimer = useCallback(() => {
    if (repeatTimeoutRef.current == null) return
    window.clearTimeout(repeatTimeoutRef.current)
    repeatTimeoutRef.current = null
  }, [])

  const clearPendingShort = useCallback(() => {
    if (pendingShortRef.current == null) return
    window.clearTimeout(pendingShortRef.current.timeoutId)
    pendingShortRef.current = null
  }, [])

  const emitInteraction = useCallback((key: FrontPanelKeyId, gesture: KeyGestureId) => {
    if (gesture === 'repeat' && !isRepeatKey(key)) return

    const interaction = {
      key,
      gesture,
    } satisfies FrontPanelRuntimeInteraction

    setLastGesture(gesture)
    setState((current) => applyFrontPanelInteraction(current, interaction))
  }, [])

  const scheduleRepeat = useCallback(
    (key: FrontPanelKeyId) => {
      clearRepeatTimer()
      const activePress = activePressRef.current
      if (!activePress || activePress.key !== key || !activePress.longFired || !isRepeatKey(key)) {
        return
      }

      const elapsedMs = window.performance.now() - activePress.startedAt
      const intervalMs =
        elapsedMs >= LONG_PRESS_MS + REPEAT_FAST_AFTER_MS
          ? REPEAT_FAST_INTERVAL_MS
          : REPEAT_INITIAL_INTERVAL_MS

      repeatTimeoutRef.current = window.setTimeout(() => {
        const currentPress = activePressRef.current
        if (!currentPress || currentPress.key !== key || !currentPress.longFired) return
        emitInteraction(key, 'repeat')
        scheduleRepeat(key)
      }, intervalMs)
    },
    [clearRepeatTimer, emitInteraction]
  )

  const handlePressStart = useCallback(
    (key: FrontPanelKeyId) => {
      if (activePressRef.current != null) return

      activePressRef.current = {
        key,
        startedAt: window.performance.now(),
        longFired: false,
      }
      setActiveKey(key)

      clearLongTimer()
      longTimeoutRef.current = window.setTimeout(() => {
        const activePress = activePressRef.current
        if (!activePress || activePress.key !== key) return

        activePress.longFired = true
        emitInteraction(key, 'long')
        scheduleRepeat(key)
      }, LONG_PRESS_MS)
    },
    [clearLongTimer, emitInteraction, scheduleRepeat]
  )

  const handlePressEnd = useCallback(
    (key: FrontPanelKeyId) => {
      const activePress = activePressRef.current
      if (!activePress || activePress.key !== key) return

      activePressRef.current = null
      setActiveKey(null)
      clearLongTimer()
      clearRepeatTimer()

      if (activePress.longFired) return

      const pendingShort = pendingShortRef.current
      if (pendingShort && pendingShort.key === key) {
        clearPendingShort()
        emitInteraction(key, 'double')
        return
      }

      clearPendingShort()
      pendingShortRef.current = {
        key,
        timeoutId: window.setTimeout(() => {
          pendingShortRef.current = null
          emitInteraction(key, 'short')
        }, DOUBLE_PRESS_WINDOW_MS),
      }
    },
    [clearLongTimer, clearPendingShort, clearRepeatTimer, emitInteraction]
  )

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = keyFromKeyboardEvent(event)
      if (!key || event.repeat) return

      event.preventDefault()
      handlePressStart(key)
    }

    const handleKeyUp = (event: KeyboardEvent) => {
      const key = keyFromKeyboardEvent(event)
      if (!key) return

      event.preventDefault()
      handlePressEnd(key)
    }

    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('keyup', handleKeyUp)

    return () => {
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('keyup', handleKeyUp)
    }
  }, [handlePressEnd, handlePressStart])

  useEffect(() => {
    return () => {
      clearLongTimer()
      clearRepeatTimer()
      clearPendingShort()
    }
  }, [clearLongTimer, clearPendingShort, clearRepeatTimer])

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
            Keyboard: arrows or WASD for direction, Space or Enter for center.
          </p>
        </div>

        <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_auto]">
          <div className="grid content-start gap-2">
            <div className="frontpanel-section-label">Gesture</div>
            <div className="grid grid-cols-2 gap-2">
              {gestureOptions.map((gesture) => {
                const isActive = gesture.id === lastGesture
                return (
                  <div
                    key={gesture.id}
                    className={[
                      'frontpanel-gesture-button',
                      isActive ? 'frontpanel-gesture-button-active' : '',
                    ].join(' ')}
                    data-testid={`frontpanel-gesture-${gesture.id}`}
                    data-active={isActive}
                  >
                    {gesture.label}
                  </div>
                )
              })}
            </div>
          </div>

          <div className="grid justify-items-center gap-2">
            <div className="frontpanel-section-label">Cabinet controls</div>
            <div className="frontpanel-switch-plate grid grid-cols-3 grid-rows-3 gap-2">
              {switchKeys.map((switchKey) => {
                const isActive = activeKey === switchKey.id
                return (
                  <button
                    key={switchKey.id}
                    type="button"
                    className={[
                      switchKey.className,
                      'frontpanel-switch-key',
                      isActive ? 'frontpanel-switch-key-active' : '',
                    ].join(' ')}
                    data-testid={`frontpanel-switch-${switchKey.id}`}
                    aria-label={switchKey.label}
                    onPointerDown={(event) => {
                      event.preventDefault()
                      if (event.currentTarget.hasPointerCapture?.(event.pointerId) === false) {
                        try {
                          event.currentTarget.setPointerCapture(event.pointerId)
                        } catch {
                          // Synthetic Storybook pointer events do not always create a capturable pointer.
                        }
                      }
                      handlePressStart(switchKey.id)
                    }}
                    onPointerCancel={() => handlePressEnd(switchKey.id)}
                    onPointerUp={(event) => {
                      event.preventDefault()
                      handlePressEnd(switchKey.id)
                    }}
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
