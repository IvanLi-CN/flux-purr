import { useAppVariant } from '@/app-mode'
import { ControlPlaneDemo } from '@/features/control-plane-demo'
import { liveControlPlaneScenario } from '@/features/control-plane-demo/live-scenario'
import { controlPlaneScenario } from '@/features/control-plane-demo/mock-data'
import { UiDemo } from '@/ui-demo'

function App() {
  const variant = useAppVariant()
  if (new URLSearchParams(window.location.search).has('uiDemo')) {
    return <UiDemo />
  }
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
