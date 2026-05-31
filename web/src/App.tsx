import { useAppVariant } from '@/app-mode'
import { ControlPlaneDemo } from '@/features/control-plane-demo'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'

function App() {
  const variant = useAppVariant()
  const isLive = variant === 'live'

  return (
    <ControlPlaneDemo
      scenario={isLive ? liveControlPlaneScenario : controlPlaneScenario}
      allowDemoControls={!isLive}
      devd={{
        enabled: isLive,
        includeMockDevices: false,
      }}
      webSerial={{
        enabled: isLive,
      }}
    />
  )
}

export default App
