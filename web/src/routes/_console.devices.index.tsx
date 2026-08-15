import { createFileRoute } from '@tanstack/react-router'
import { redirectFromDeviceIndex } from '@/routing/redirects'

export const Route = createFileRoute('/_console/devices/')({
  beforeLoad: ({ search }) => redirectFromDeviceIndex(search),
  component: () => null,
})
