import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import type { WifiConfig } from '../types'

interface WifiConfigFormProps {
  initialConfig: WifiConfig
  onSave?: (next: WifiConfig) => void
}

export function WifiConfigForm({ initialConfig, onSave }: WifiConfigFormProps) {
  const [config, setConfig] = useState(initialConfig)

  return (
    <Card className="border-cyan-100/80 shadow-lg shadow-cyan-200/35">
      <CardHeader>
        <CardTitle className="text-lg">Network & Telemetry</CardTitle>
        <CardDescription>配置 AP/STA 参数与上报节奏（ms）。</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <Label htmlFor="ssid">SSID</Label>
          <Input
            id="ssid"
            value={config.ssid}
            onChange={(event) => setConfig((prev) => ({ ...prev, ssid: event.target.value }))}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="password">Password (masked)</Label>
          <Input
            id="password"
            value={config.passwordMasked}
            onChange={(event) =>
              setConfig((prev) => ({ ...prev, passwordMasked: event.target.value }))
            }
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor="interval">Telemetry Interval (ms)</Label>
          <Input
            id="interval"
            type="number"
            min={100}
            value={config.telemetryIntervalMs}
            onChange={(event) =>
              setConfig((prev) => ({
                ...prev,
                telemetryIntervalMs: Number(event.target.value) || prev.telemetryIntervalMs,
              }))
            }
          />
        </div>
        <div className="flex items-center justify-between rounded-xl bg-muted/40 p-3">
          <div>
            <p className="text-sm font-medium">Auto reconnect</p>
            <p className="text-xs text-muted-foreground">掉线后由设备自动回连。</p>
          </div>
          <Switch
            checked={config.autoReconnect}
            onCheckedChange={(next) => setConfig((prev) => ({ ...prev, autoReconnect: next }))}
          />
        </div>
        <Button className="w-full" onClick={() => onSave?.(config)}>
          Save Configuration
        </Button>
      </CardContent>
    </Card>
  )
}
