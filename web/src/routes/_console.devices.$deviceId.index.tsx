import { createFileRoute, redirect } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/')({
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: '/devices/$deviceId/overview',
      params,
      search,
      replace: true,
    })
  },
  component: () => null,
})
