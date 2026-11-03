import { isPublicDemoBuild } from './public-demo'

export interface DemoRuntime {
  variant: 'live' | 'demo'
  mockOnly: boolean
  restore?: () => void
}

export interface DemoRuntimeWindow {
  fetch: typeof window.fetch
  location: Pick<Location, 'href' | 'origin'>
  document: { documentElement: { dataset: Record<string, string | undefined> } }
}

interface BootstrapOptions {
  publicDemo?: boolean
  targetWindow?: DemoRuntimeWindow
}

export function bootstrapDemoRuntime(options: BootstrapOptions = {}): DemoRuntime {
  const publicDemo = options.publicDemo ?? isPublicDemoBuild()
  if (!publicDemo) return { variant: 'live', mockOnly: false }

  const targetWindow =
    options.targetWindow ??
    (typeof window === 'undefined' ? undefined : (window as DemoRuntimeWindow))
  if (!targetWindow || typeof targetWindow.fetch !== 'function') {
    throw new Error('The public demo mock runtime is unavailable.')
  }

  const nativeFetch = targetWindow.fetch.bind(targetWindow)
  const guardedFetch: typeof window.fetch = (input, init) => {
    const requestUrl = new URL(
      typeof input === 'string' || input instanceof URL ? input.toString() : input.url,
      targetWindow.location.href
    )
    const isLiveRequest =
      requestUrl.origin !== targetWindow.location.origin ||
      /^\/(?:api|devd)(?:\/|$)/.test(requestUrl.pathname)
    if (isLiveRequest) {
      return Promise.reject(new Error('Live requests are disabled in the public demo.'))
    }
    return nativeFetch(input, init)
  }

  targetWindow.fetch = guardedFetch
  targetWindow.document.documentElement.dataset.fluxPurrRuntime = 'demo-mock-only'

  return {
    variant: 'demo',
    mockOnly: true,
    restore: () => {
      if (targetWindow.fetch === guardedFetch) targetWindow.fetch = nativeFetch
    },
  }
}
