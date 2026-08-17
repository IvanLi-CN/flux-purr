import { redirect } from '@tanstack/react-router'
import { isPublicDemoBuild } from '@/public-demo'
import { readRoutePreferences } from './route-preferences'
import { type AppSearch, appVariantFromSearch } from './search'

export function redirectFromDeviceIndex(search: AppSearch) {
  if (isPublicDemoBuild()) {
    const { uiDemo: _uiDemo, ...publicDemoSearch } = search
    throw redirect({
      to: '/devices/$deviceId/overview',
      params: { deviceId: 'fp-lab-01' },
      search: publicDemoSearch,
      replace: true,
    })
  }
  if (search.uiDemo) return
  const variant = appVariantFromSearch(search)
  const deviceId = readRoutePreferences().lastDeviceByVariant[variant]
  if (deviceId) {
    throw redirect({
      to: '/devices/$deviceId/overview',
      params: { deviceId },
      search,
      replace: true,
    })
  }
  throw redirect({ to: '/devices/new', search, replace: true })
}
