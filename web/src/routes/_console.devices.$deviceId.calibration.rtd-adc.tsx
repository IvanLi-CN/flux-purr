import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_console/devices/$deviceId/calibration/rtd-adc')({
  component: () => null,
})
