import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/calibration/')({
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: '/devices/$deviceId/calibration/heater-curve',
      params,
      search,
      replace: true,
    })
  },
  component: () => null,
})
