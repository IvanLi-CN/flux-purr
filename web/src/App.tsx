import { ConsoleLayout } from '@/features/device-console/components/console-layout'
import {
  mockStatus,
  mockTelemetrySeries,
  mockWifiConfig,
} from '@/features/device-console/mock-data'

function App() {
  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_top_left,_#dff5ff,_#f6fbff_35%,_#f9fff8)]">
      <ConsoleLayout
        title="S3 Runtime Cockpit"
        subtitle="React 控制台与固件控制平面契约对齐：HTTP 读写 + WebSocket telemetry。"
        status={mockStatus}
        telemetry={mockTelemetrySeries}
        wifiConfig={mockWifiConfig}
      />
    </div>
  )
}

export default App
