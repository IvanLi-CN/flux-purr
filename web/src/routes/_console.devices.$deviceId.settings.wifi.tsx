import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/settings/wifi')({
  component: () => null,
})
