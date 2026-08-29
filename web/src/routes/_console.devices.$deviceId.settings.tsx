import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/settings')({
  beforeLoad: ({ location, params, search }) => {
    if (!location.pathname.endsWith('/settings')) {
      return
    }
    throw redirect({
      to: '/devices/$deviceId/settings/presets',
      params,
      search,
      replace: true,
    })
  },
  component: () => null,
})
