import { useBlocker, useNavigate, useRouterState } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { persistAppVariant } from '@/app-mode'
import {
  type CalibrationRouteGuard,
  type ConsoleNavigationAdapter,
  ControlPlaneDemo,
} from '@/features/control-plane-demo'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'
import { Route as RootRoute } from '@/routes/__root'
import { consoleRoutePath, parseConsoleRoute, routeLabel } from '@/routing/console-route'
import { appVariantFromSearch } from '@/routing/search'
import { UiDemo } from '@/ui-demo'

function App() {
  const search = RootRoute.useSearch()
  const variant = appVariantFromSearch(search)
  const navigate = useNavigate()
  const location = useRouterState({ select: (state) => state.location })
  const routeState = useMemo(() => parseConsoleRoute(location.pathname), [location.pathname])
  const [calibrationGuard, setCalibrationGuard] = useState<CalibrationRouteGuard | null>(null)

  useEffect(() => persistAppVariant(variant), [variant])

  useEffect(() => {
    const expectedDemo = search.demo ? 'true' : 'false'
    const rawSearch = new URLSearchParams(location.searchStr)
    const needsDemoNormalization = rawSearch.get('demo') !== expectedDemo
    const needsUiDemoPathNormalization = Boolean(search.uiDemo) && location.pathname !== '/'
    if (!needsDemoNormalization && !needsUiDemoPathNormalization) return
    void navigate({
      to: (needsUiDemoPathNormalization ? '/' : location.pathname) as '/',
      search,
      replace: true,
    })
  }, [location.pathname, location.searchStr, navigate, search])

  const blocker = useBlocker({
    shouldBlockFn: ({ current, next }) => {
      if (!calibrationGuard) return false
      const sameSearch =
        current.search.demo === next.search.demo && current.search.uiDemo === next.search.uiDemo
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
      return true
    },
    enableBeforeUnload: Boolean(calibrationGuard),
    disabled: !calibrationGuard,
    withResolver: true,
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

  const navigation = useMemo<ConsoleNavigationAdapter | undefined>(() => {
    if (!routeState) return undefined
    return {
      state: routeState,
      variant,
      search,
      navigate: navigateConsole,
      blockedNavigation:
        blocker.status === 'blocked'
          ? {
              next: parseConsoleRoute(blocker.next.pathname),
              nextLabel: routeLabel(parseConsoleRoute(blocker.next.pathname)),
              proceed: blocker.proceed,
              reset: blocker.reset,
            }
          : null,
      onCalibrationGuardChange,
    }
  }, [blocker, navigateConsole, onCalibrationGuardChange, routeState, search, variant])

  if (search.uiDemo) return <UiDemo />
  if (!routeState || !navigation) return null
  const isLive = variant === 'live'

  return (
    <ControlPlaneDemo
      scenario={isLive ? liveControlPlaneScenario : controlPlaneScenario}
      navigation={navigation}
      allowDemoControls={!isLive}
      devd={{
        enabled: isLive,
        includeMockDevices: false,
      }}
      webSerial={{
        enabled: isLive,
      }}
    />
  )
}

export default App
