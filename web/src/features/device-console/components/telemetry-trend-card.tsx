import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import type { TelemetryPoint } from '../types'

interface TelemetryTrendCardProps {
  points: TelemetryPoint[]
}

export function TelemetryTrendCard({ points }: TelemetryTrendCardProps) {
  const maxCurrent = Math.max(...points.map((point) => point.current), 0.1)

  return (
    <Card className="border-emerald-100/80 shadow-lg shadow-emerald-200/35">
      <CardHeader>
        <CardTitle className="text-lg">Telemetry Trend</CardTitle>
        <CardDescription>最近 5 个采样周期的电流趋势与电压表。</CardDescription>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="space-y-2 rounded-xl bg-gradient-to-r from-emerald-50 to-cyan-50 p-4">
          <p className="text-xs uppercase tracking-wide text-muted-foreground">Current trend</p>
          <div className="grid grid-cols-5 items-end gap-2">
            {points.map((point) => {
              const height = Math.round((point.current / maxCurrent) * 100)
              return (
                <div key={point.ts} className="space-y-1 text-center">
                  <div
                    className="mx-auto w-full rounded-md bg-gradient-to-t from-emerald-500 to-cyan-400"
                    style={{ height: `${Math.max(height, 12)}px` }}
                  />
                  <p className="text-[11px] text-muted-foreground">{point.ts}</p>
                </div>
              )
            })}
          </div>
        </div>

        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Time</TableHead>
              <TableHead className="text-right">Voltage (V)</TableHead>
              <TableHead className="text-right">Current (A)</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {points.map((point) => (
              <TableRow key={point.ts}>
                <TableCell>{point.ts}</TableCell>
                <TableCell className="text-right">{point.voltage.toFixed(2)}</TableCell>
                <TableCell className="text-right">{point.current.toFixed(2)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  )
}
