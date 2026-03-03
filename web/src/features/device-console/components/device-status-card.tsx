import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import type { DeviceStatus } from '../types'

function modeTone(mode: DeviceStatus['mode']) {
  if (mode === 'sampling') return 'bg-emerald-100 text-emerald-700 border-emerald-200'
  if (mode === 'fault') return 'bg-rose-100 text-rose-700 border-rose-200'
  return 'bg-slate-100 text-slate-700 border-slate-200'
}

interface DeviceStatusCardProps {
  status: DeviceStatus
}

export function DeviceStatusCard({ status }: DeviceStatusCardProps) {
  return (
    <Card className="border-sky-100/80 shadow-lg shadow-sky-200/40">
      <CardHeader className="pb-3">
        <CardTitle className="flex items-center justify-between text-lg">
          Device Runtime
          <Badge className={modeTone(status.mode)}>{status.mode.toUpperCase()}</Badge>
        </CardTitle>
        <CardDescription>实时状态由 MCU 周期采样并通过 HTTP 推送。</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <div className="grid grid-cols-2 gap-3">
          <Metric label="Voltage" value={`${status.voltage.toFixed(2)} V`} />
          <Metric label="Current" value={`${status.current.toFixed(2)} A`} />
          <Metric label="Board Temp" value={`${status.boardTempC.toFixed(1)} °C`} />
          <Metric label="Wi-Fi RSSI" value={`${status.wifiRssi} dBm`} />
          <Metric label="PD Request" value={`${status.pdRequestMv} mV`} />
          <Metric label="PD Contract" value={`${status.pdContractMv} mV`} />
          <Metric label="USB Route" value={status.usbRoute.toUpperCase()} />
          <Metric
            label="Fan"
            value={status.fanEnabled ? `${status.fanPwmPermille} ‰` : 'Disabled'}
          />
        </div>
        <Separator />
        <div className="flex items-center justify-between text-muted-foreground">
          <span>PD State</span>
          <span className="font-medium text-foreground">{status.pdState}</span>
        </div>
        <div className="flex items-center justify-between text-muted-foreground">
          <span>Front Panel</span>
          <span>{status.frontpanelKey ?? 'none'}</span>
        </div>
        <div className="flex items-center justify-between text-muted-foreground">
          <span>Firmware</span>
          <code className="rounded bg-muted px-2 py-0.5 text-xs">{status.fwVersion}</code>
        </div>
        <div className="flex items-center justify-between text-muted-foreground">
          <span>Last Sync</span>
          <span>{status.lastSync}</span>
        </div>
      </CardContent>
    </Card>
  )
}

interface MetricProps {
  label: string
  value: string
}

function Metric({ label, value }: MetricProps) {
  return (
    <div className="rounded-xl bg-gradient-to-br from-sky-50 to-cyan-50 p-3">
      <div className="text-xs uppercase tracking-wide text-muted-foreground">{label}</div>
      <div className="mt-1 font-semibold text-foreground">{value}</div>
    </div>
  )
}
