import type { DeviceStatus, TelemetryPoint, WifiConfig } from '../types'
import { DeviceStatusCard } from './device-status-card'
import { TelemetryTrendCard } from './telemetry-trend-card'
import { WifiConfigForm } from './wifi-config-form'

interface ConsoleLayoutProps {
  title: string
  subtitle: string
  status: DeviceStatus
  telemetry: TelemetryPoint[]
  wifiConfig: WifiConfig
}

export function ConsoleLayout({
  title,
  subtitle,
  status,
  telemetry,
  wifiConfig,
}: ConsoleLayoutProps) {
  return (
    <main className="mx-auto max-w-6xl space-y-8 px-4 py-10 sm:px-6 lg:px-8">
      <header className="space-y-3">
        <p className="text-sm font-medium tracking-wide text-cyan-700">Flux Purr Device Console</p>
        <h1 className="text-4xl font-semibold tracking-tight text-slate-900">{title}</h1>
        <p className="max-w-2xl text-sm text-slate-600">{subtitle}</p>
      </header>

      <section className="grid gap-6 lg:grid-cols-3">
        <div className="lg:col-span-1">
          <DeviceStatusCard status={status} />
        </div>
        <div className="lg:col-span-2">
          <TelemetryTrendCard points={telemetry} />
        </div>
      </section>

      <section className="grid gap-6 lg:grid-cols-2">
        <WifiConfigForm initialConfig={wifiConfig} />
      </section>
    </main>
  )
}
