import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/calibration/heater-curve')({
  component: () => null,
})
