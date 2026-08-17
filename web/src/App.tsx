import { useBlocker, useNavigate, useRouterState } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { persistAppVariant } from '@/app-mode'
import {
  type CalibrationRouteGuard,
  type ConsoleNavigationAdapter,
  ControlPlaneDemo,
} from '@/features/control-plane-demo'
import { DemoInspector } from '@/features/control-plane-demo/components/demo-inspector'
import {
  type DemoInspectorState,
  demoInspectorSearch,
  demoInspectorStateFromSearch,
  deriveDemoScenario,
} from '@/features/control-plane-demo/demo-inspector-state'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import type { EventLogEntry } from '@/features/control-plane-demo/types'
import { isPublicDemoBuild } from '@/public-demo'
import { Route as RootRoute } from '@/routes/__root'
import { consoleRoutePath, parseConsoleRoute, routeLabel } from '@/routing/console-route'
import { appVariantFromSearch } from '@/routing/search'
import { UiDemo } from '@/ui-demo'

function App() {
  const search = RootRoute.useSearch()
  const variant = appVariantFromSearch(search)
  const publicDemo = isPublicDemoBuild()
  const navigate = useNavigate()
  const location = useRouterState({ select: (state) => state.location })
  const routeState = useMemo(() => parseConsoleRoute(location.pathname), [location.pathname])
  const [calibrationGuard, setCalibrationGuard] = useState<CalibrationRouteGuard | null>(null)
  const [blockedNavigation, setBlockedNavigation] =
    useState<ConsoleNavigationAdapter['blockedNavigation']>(null)
  const pendingBlockRef = useRef<{
    id: symbol
    resolve: (shouldBlock: boolean) => void
  } | null>(null)
  const [inspectorEvents, setInspectorEvents] = useState<EventLogEntry[]>([])
  const inspectorState = useMemo(() => demoInspectorStateFromSearch(search), [search])
  const inspectorStateRef = useRef(inspectorState)
  const inspectorSearchRef = useRef(search)
  const inspectorPathnameRef = useRef(location.pathname)
  const inspectorNavigationQueueRef = useRef(Promise.resolve())
  const inspectorPendingNavigationCountRef = useRef(0)
  useEffect(() => {
    if (inspectorPendingNavigationCountRef.current > 0) return
    inspectorStateRef.current = inspectorState
    inspectorSearchRef.current = search
    inspectorPathnameRef.current = location.pathname
  }, [inspectorState, location.pathname, search])
  const isLive = variant === 'live'
  const activeScenario = useMemo(
    () => (isLive ? liveControlPlaneScenario : deriveDemoScenario(inspectorState, inspectorEvents)),
    [inspectorEvents, inspectorState, isLive]
  )

  useEffect(() => {
    const routedDeviceId = routeState?.kind === 'device' ? routeState.deviceId : null
    if (
      isLive ||
      inspectorState.demoScene !== 'calibration-active' ||
      !routedDeviceId ||
      routedDeviceId === activeScenario.selectedDeviceId
    ) {
      return
    }
    const {
      demoScene: _demoScene,
      demoLease: _demoLease,
      demoNetwork: _demoNetwork,
      demoArtifact: _demoArtifact,
      ...searchWithoutInspector
    } = search
    void navigate({
      to: location.pathname as '/',
      search: searchWithoutInspector,
      replace: true,
      ignoreBlocker: true,
    })
  }, [
    activeScenario.selectedDeviceId,
    inspectorState.demoScene,
    isLive,
    location.pathname,
    navigate,
    routeState,
    search,
  ])

  useEffect(() => {
    if (!publicDemo) persistAppVariant(variant)
  }, [publicDemo, variant])

  useEffect(() => {
    const expectedDemo = search.demo ? 'true' : 'false'
    const rawSearch = new URLSearchParams(location.searchStr)
    const needsDemoNormalization = rawSearch.get('demo') !== expectedDemo
    const needsUiDemoPathNormalization = Boolean(search.uiDemo) && location.pathname !== '/'
    const needsPublicUiDemoRemoval = publicDemo && rawSearch.has('uiDemo')
    if (!needsDemoNormalization && !needsUiDemoPathNormalization && !needsPublicUiDemoRemoval)
      return
    void navigate({
      to: (needsUiDemoPathNormalization ? '/' : location.pathname) as '/',
      search,
      replace: true,
    })
  }, [location.pathname, location.searchStr, navigate, publicDemo, search])

  const settlePendingBlock = useCallback((id: symbol, shouldBlock: boolean) => {
    const pending = pendingBlockRef.current
    if (!pending || pending.id !== id) return
    pendingBlockRef.current = null
    setBlockedNavigation(null)
    pending.resolve(shouldBlock)
  }, [])

  useEffect(
    () => () => {
      pendingBlockRef.current?.resolve(true)
      pendingBlockRef.current = null
    },
    []
  )

  useEffect(() => {
    if (calibrationGuard || !pendingBlockRef.current) return
    const pending = pendingBlockRef.current
    pendingBlockRef.current = null
    setBlockedNavigation(null)
    pending.resolve(false)
  }, [calibrationGuard])

  useBlocker({
    shouldBlockFn: async ({ current, next }) => {
      if (!calibrationGuard) return false
      const sameSearch =
        current.search.demo === next.search.demo &&
        current.search.uiDemo === next.search.uiDemo &&
        current.search.demoScene === next.search.demoScene &&
        current.search.demoLease === next.search.demoLease &&
        current.search.demoNetwork === next.search.demoNetwork &&
        current.search.demoArtifact === next.search.demoArtifact
      if (current.pathname === next.pathname && sameSearch) return false
      const activeCalibrationPath = consoleRoutePath({
        kind: 'device',
        deviceId: calibrationGuard.deviceId,
        view: 'calibration',
        calibrationTab: calibrationGuard.workspaceTab,
      })
      if (
        next.pathname === activeCalibrationPath &&
        current.pathname !== activeCalibrationPath &&
        sameSearch
      ) {
        return false
      }
      pendingBlockRef.current?.resolve(true)
      const id = Symbol('blocked-navigation')
      return new Promise<boolean>((resolve) => {
        pendingBlockRef.current = { id, resolve }
        const nextRoute = parseConsoleRoute(next.pathname)
        setBlockedNavigation({
          next: nextRoute,
          nextLabel: routeLabel(nextRoute),
          proceed: () => settlePendingBlock(id, false),
          reset: () => settlePendingBlock(id, true),
        })
      })
    },
    enableBeforeUnload: Boolean(calibrationGuard),
    disabled: false,
    withResolver: false,
  })

  const navigateConsole = useCallback<ConsoleNavigationAdapter['navigate']>(
    async (next, options) => {
      if (next.kind === 'add-device') {
        await navigate({
          to: '/devices/new',
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      const params = { deviceId: next.deviceId }
      if (next.view === 'dashboard') {
        await navigate({
          to: '/devices/$deviceId/overview',
          params,
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      if (next.view === 'settings') {
        await navigate({
          to: '/devices/$deviceId/settings',
          params,
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      if (next.view === 'update') {
        await navigate({
          to: '/devices/$deviceId/update',
          params,
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      const tab = next.calibrationTab ?? 'heater_curve'
      if (tab === 'rtd_adc') {
        await navigate({
          to: '/devices/$deviceId/calibration/rtd-adc',
          params,
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      if (tab === 'vin_adc') {
        await navigate({
          to: '/devices/$deviceId/calibration/vin-adc',
          params,
          search,
          replace: options?.replace,
          ignoreBlocker: options?.ignoreBlocker,
        })
        return
      }
      await navigate({
        to: '/devices/$deviceId/calibration/heater-curve',
        params,
        search,
        replace: options?.replace,
        ignoreBlocker: options?.ignoreBlocker,
      })
    },
    [navigate, search]
  )

  const onCalibrationGuardChange = useCallback((next: CalibrationRouteGuard | null) => {
    setCalibrationGuard((current) => {
      if (current?.deviceId === next?.deviceId && current?.workspaceTab === next?.workspaceTab) {
        return current
      }
      return next
    })
  }, [])

  const updateInspectorState = useCallback(
    (nextState: Partial<DemoInspectorState>) => {
      const previousState = inspectorStateRef.current
      const mergedState = { ...previousState, ...nextState }
      inspectorStateRef.current = mergedState
      inspectorPendingNavigationCountRef.current += 1

      const runNavigation = async () => {
        const {
          demoScene: _demoScene,
          demoLease: _demoLease,
          demoNetwork: _demoNetwork,
          demoArtifact: _demoArtifact,
          ...searchWithoutInspector
        } = inspectorSearchRef.current
        const nextSearch = {
          ...searchWithoutInspector,
          ...demoInspectorSearch(mergedState),
        }
        inspectorSearchRef.current = nextSearch
        const sceneChanged = mergedState.demoScene !== previousState.demoScene
        const nextScenario = deriveDemoScenario(mergedState, inspectorEvents)
        const nextPathname = sceneChanged
          ? mergedState.demoScene === 'calibration-active'
            ? `/devices/${nextScenario.selectedDeviceId}/calibration/heater-curve`
            : `/devices/${nextScenario.selectedDeviceId}/overview`
          : inspectorPathnameRef.current
        inspectorPathnameRef.current = nextPathname
        const nextRoute = nextPathname.includes('/calibration/')
          ? '/devices/$deviceId/calibration/heater-curve'
          : '/devices/$deviceId/overview'
        const nextRouteState = parseConsoleRoute(nextPathname)
        const nextDeviceId =
          nextRouteState?.kind === 'device'
            ? nextRouteState.deviceId
            : nextScenario.selectedDeviceId
        if (sceneChanged) {
          await navigate({
            to: nextRoute as '/',
            params: { deviceId: nextDeviceId },
            search: nextSearch,
            replace: true,
          })
          return
        }
        await navigate({
          to: nextRoute as '/',
          params: { deviceId: nextDeviceId },
          search: nextSearch,
          replace: true,
        })
      }

      const queuedNavigation = inspectorNavigationQueueRef.current.then(
        runNavigation,
        runNavigation
      )
      inspectorNavigationQueueRef.current = queuedNavigation.then(
        () => {
          inspectorPendingNavigationCountRef.current -= 1
        },
        () => {
          inspectorPendingNavigationCountRef.current -= 1
        }
      )
      return queuedNavigation
    },
    [inspectorEvents, navigate]
  )

  const selectInspectorDevice = useCallback(
    async (deviceId: string) => {
      const runNavigation = async () => {
        const currentState = inspectorStateRef.current
        const nextInspectorState =
          currentState.demoScene === 'calibration-active'
            ? { ...currentState, demoScene: 'normal' as const }
            : currentState
        const {
          demoScene: _demoScene,
          demoLease: _demoLease,
          demoNetwork: _demoNetwork,
          demoArtifact: _demoArtifact,
          ...searchWithoutInspector
        } = inspectorSearchRef.current
        const nextSearch = { ...searchWithoutInspector, ...demoInspectorSearch(nextInspectorState) }
        const nextPathname = `/devices/${deviceId}/overview`
        const targetNavigation = {
          to: '/devices/$deviceId/overview',
          params: { deviceId },
          search: nextSearch,
        } as const
        if (currentState.demoScene === 'calibration-active') {
          void navigate(targetNavigation).catch(() => undefined)
          return
        }
        await navigate(targetNavigation)
        inspectorPathnameRef.current = nextPathname
        inspectorSearchRef.current = nextSearch
        inspectorStateRef.current = nextInspectorState
      }
      inspectorPendingNavigationCountRef.current += 1
      const queuedNavigation = inspectorNavigationQueueRef.current.then(
        runNavigation,
        runNavigation
      )
      inspectorNavigationQueueRef.current = queuedNavigation.then(
        () => {
          inspectorPendingNavigationCountRef.current -= 1
        },
        () => {
          inspectorPendingNavigationCountRef.current -= 1
        }
      )
      await queuedNavigation
    },
    [navigate]
  )

  const simulateInspectorEvent = useCallback((event: Pick<EventLogEntry, 'message' | 'tone'>) => {
    setInspectorEvents((current) =>
      [
        ...current,
        {
          time: `20:19:${String(10 + current.length * 3).padStart(2, '0')}`,
          source: 'demo',
          ...event,
        },
      ].slice(-12)
    )
  }, [])

  const navigation = useMemo<ConsoleNavigationAdapter | undefined>(() => {
    if (!routeState) return undefined
    return {
      state: routeState,
      variant,
      search,
      navigate: navigateConsole,
      blockedNavigation,
      onCalibrationGuardChange,
    }
  }, [blockedNavigation, navigateConsole, onCalibrationGuardChange, routeState, search, variant])

  if (search.uiDemo) return <UiDemo />
  if (!routeState || !navigation) return null
  return (
    <>
      <ControlPlaneDemo
        scenario={activeScenario}
        navigation={navigation}
        allowDemoControls={!isLive}
        mockOnly={publicDemo}
        devd={{
          enabled: isLive && !publicDemo,
          includeMockDevices: false,
        }}
        webSerial={{
          enabled: isLive && !publicDemo,
        }}
      />
      {!isLive && !search.uiDemo ? (
        <DemoInspector
          state={inspectorState}
          devices={activeScenario.devices}
          selectedDeviceId={
            routeState?.kind === 'device' ? routeState.deviceId : activeScenario.selectedDeviceId
          }
          onStateChange={(next) => void updateInspectorState(next)}
          onSelectDevice={(deviceId) => void selectInspectorDevice(deviceId)}
          onSimulate={simulateInspectorEvent}
        />
      ) : null}
    </>
  )
}

export default App
