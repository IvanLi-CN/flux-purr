import { createFileRoute } from '@tanstack/react-router'
import { redirectFromDeviceIndex } from '@/routing/redirects'
import { UiDemo } from '@/ui-demo'

export const Route = createFileRoute('/_console/')({
  beforeLoad: ({ search }) => redirectFromDeviceIndex(search),
  component: UiDemo,
})
