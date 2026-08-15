import { createRootRoute, Link, Outlet, redirect, retainSearchParams } from '@tanstack/react-router'
import { AlertTriangle, Home } from 'lucide-react'
import { validateAppSearch } from '@/routing/search'

export const Route = createRootRoute({
  validateSearch: validateAppSearch,
  beforeLoad: ({ location, search }) => {
    if (search.uiDemo && location.pathname !== '/') {
      throw redirect({ to: '/', search, replace: true })
    }
  },
  search: {
    middlewares: [retainSearchParams(['demo', 'uiDemo'])],
  },
  component: Outlet,
  notFoundComponent: NotFoundPage,
})

function NotFoundPage() {
  const search = Route.useSearch()
  return (
    <main className="industrial-shell text-[var(--industrial-text)]">
      <section className="industrial-route-message" aria-labelledby="route-not-found-title">
        <AlertTriangle aria-hidden="true" />
        <div>
          <h1 id="route-not-found-title">路径不存在</h1>
          <p>此地址不属于 Flux Purr 控制台。返回设备入口后可继续连接或选择目标。</p>
        </div>
        <Link className="industrial-route-message__action" to="/devices" search={search} replace>
          <Home aria-hidden="true" size={18} />
          返回设备入口
        </Link>
      </section>
    </main>
  )
}
