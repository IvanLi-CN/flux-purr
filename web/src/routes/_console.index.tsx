import { createFileRoute } from '@tanstack/react-router'
import { isPublicDemoBuild } from '@/public-demo'
import { redirectFromDeviceIndex } from '@/routing/redirects'
import { UiDemo } from '@/ui-demo'

export const Route = createFileRoute('/_console/')({
  beforeLoad: ({ location, search }) => {
    const rawSearch = new URLSearchParams(
      typeof window === 'undefined' ? location.searchStr : window.location.search
    )
    if (!isPublicDemoBuild() && rawSearch.get('demo') === 'true') return
    return redirectFromDeviceIndex(search)
  },
  component: UiDemo,
})
