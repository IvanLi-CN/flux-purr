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

function inspectorNavigationTargetKey(
  pathname: string,
  search: {
    demo?: unknown
    uiDemo?: unknown
    demoScene?: unknown
    demoLease?: unknown
    demoNetwork?: unknown
    demoArtifact?: unknown
  }
) {
  return [
    pathname,
    search.demo,
    search.uiDemo,
    search.demoScene,
    search.demoLease,
    search.demoNetwork,
    search.demoArtifact,
  ].join('\u0001')
}

function App() {
  const search = RootRoute.useSearch()
  const variant = appVariantFromSearch(search)
  const publicDemo = isPublicDemoBuild()
  const navigate = useNavigate()
  const location = useRouterState({ select: (state) => state.location })
  const routeState = useMemo(() => parseConsoleRoute(location.pathname), [location.pathname])
  const requestedInitialView =
    new URLSearchParams(window.location.search).get('workspace') === 'firmware'
      ? 'update'
      : undefined
  const [calibrationGuard, setCalibrationGuard] = useState<CalibrationRouteGuard | null>(null)
  const [blockedNavigation, setBlockedNavigation] =
    useState<ConsoleNavigationAdapter['blockedNavigation']>(null)
  const pendingBlockRef = useRef<{
    id: symbol
    resolve: (shouldBlock: boolean) => void
    targetKey: string
    promise: Promise<boolean>
  } | null>(null)
  const [inspectorEvents, setInspectorEvents] = useState<EventLogEntry[]>([])
  const inspectorState = useMemo(() => demoInspectorStateFromSearch(search), [search])
  const inspectorStateRef = useRef(inspectorState)
  const inspectorSearchRef = useRef(search)
  const inspectorPathnameRef = useRef(location.pathname)
  const inspectorNavigationQueueRef = useRef(Promise.resolve())
  const inspectorPendingNavigationCountRef = useRef(0)
  const inspectorNavigationAbortRef = useRef<(() => void) | null>(null)
  const inspectorNavigationTargetRef = useRef<string | null>(null)
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
    if (shouldBlock) inspectorNavigationAbortRef.current?.()
  }, [])

  useEffect(
    () => () => {
      pendingBlockRef.current?.resolve(true)
      pendingBlockRef.current = null
      inspectorNavigationAbortRef.current?.()
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
      const targetKey = inspectorNavigationTargetKey(next.pathname, next.search)
      const pending = pendingBlockRef.current
      if (pending?.targetKey === targetKey) return pending.promise
      pending?.resolve(true)
      if (inspectorNavigationTargetRef.current !== targetKey) {
        inspectorNavigationAbortRef.current?.()
      }
      const id = Symbol('blocked-navigation')
      let resolveBlock: (shouldBlock: boolean) => void = () => undefined
      const blockPromise = new Promise<boolean>((resolve) => {
        resolveBlock = resolve
      })
      pendingBlockRef.current = { id, resolve: resolveBlock, targetKey, promise: blockPromise }
      const nextRoute = parseConsoleRoute(next.pathname)
      setBlockedNavigation({
        next: nextRoute,
        nextLabel: routeLabel(nextRoute),
        proceed: () => settlePendingBlock(id, false),
        reset: () => settlePendingBlock(id, true),
      })
      return blockPromise
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

  const enqueueInspectorNavigation = useCallback((runNavigation: () => Promise<void>) => {
    inspectorPendingNavigationCountRef.current += 1
    const queuedNavigation = inspectorNavigationQueueRef.current.then(runNavigation, runNavigation)
    inspectorNavigationQueueRef.current = queuedNavigation.then(
      () => {
        inspectorPendingNavigationCountRef.current -= 1
      },
      () => {
        inspectorPendingNavigationCountRef.current -= 1
      }
    )
    return queuedNavigation
  }, [])

  // A cancelled route blocker does not settle TanStack's navigation promise.
  const settleInspectorNavigation = useCallback(
    (navigation: () => Promise<void>, targetKey: string) => {
      return new Promise<boolean>((resolve, reject) => {
        let settled = false
        const settle = (completed: boolean) => {
          if (settled) return
          settled = true
          if (inspectorNavigationTargetRef.current === targetKey) {
            inspectorNavigationTargetRef.current = null
          }
          if (inspectorNavigationAbortRef.current === abort) {
            inspectorNavigationAbortRef.current = null
          }
          resolve(completed)
        }
        const abort = () => settle(false)
        inspectorNavigationAbortRef.current = abort
        inspectorNavigationTargetRef.current = targetKey
        let navigationPromise: Promise<void>
        try {
          navigationPromise = navigation()
        } catch (error) {
          if (inspectorNavigationTargetRef.current === targetKey) {
            inspectorNavigationTargetRef.current = null
          }
          reject(error)
          return
        }
        void navigationPromise.then(
          () => settle(true),
          (error: unknown) => {
            if (!settled) {
              settled = true
              if (inspectorNavigationTargetRef.current === targetKey) {
                inspectorNavigationTargetRef.current = null
              }
              if (inspectorNavigationAbortRef.current === abort) {
                inspectorNavigationAbortRef.current = null
              }
              reject(error)
            }
          }
        )
      })
    },
    []
  )

  const updateInspectorState = useCallback(
    (nextState: Partial<DemoInspectorState>) =>
      enqueueInspectorNavigation(async () => {
        const previousState = inspectorStateRef.current
        const mergedState = { ...previousState, ...nextState }
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
        const sceneChanged = mergedState.demoScene !== previousState.demoScene
        const nextScenario = deriveDemoScenario(mergedState, inspectorEvents)
        const nextPathname = sceneChanged
          ? mergedState.demoScene === 'calibration-active'
            ? `/devices/${nextScenario.selectedDeviceId}/calibration/heater-curve`
            : `/devices/${nextScenario.selectedDeviceId}/overview`
          : inspectorPathnameRef.current
        const completed = await settleInspectorNavigation(
          () =>
            navigate({
              to: nextPathname as '/',
              search: nextSearch,
              replace: true,
            }),
          inspectorNavigationTargetKey(nextPathname, nextSearch)
        )
        if (!completed) return
        inspectorStateRef.current = mergedState
        inspectorSearchRef.current = nextSearch
        inspectorPathnameRef.current = nextPathname
      }),
    [enqueueInspectorNavigation, inspectorEvents, navigate, settleInspectorNavigation]
  )

  const selectInspectorDevice = useCallback(
    async (deviceId: string) =>
      enqueueInspectorNavigation(async () => {
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
        const completed = await settleInspectorNavigation(
          () => navigate(targetNavigation),
          inspectorNavigationTargetKey(nextPathname, nextSearch)
        )
        if (!completed) return
        inspectorPathnameRef.current = nextPathname
        inspectorSearchRef.current = nextSearch
        inspectorStateRef.current = nextInspectorState
      }),
    [enqueueInspectorNavigation, navigate, settleInspectorNavigation]
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
        initialView={requestedInitialView}
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
