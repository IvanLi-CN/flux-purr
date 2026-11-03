import { RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { bootstrapDemoRuntime } from './demo-runtime'
import { router } from './router'

const rootElement = document.getElementById('root')
if (!rootElement) throw new Error('Missing root element')

let demoRuntimeError: unknown = null
try {
  bootstrapDemoRuntime()
} catch (error) {
  demoRuntimeError = error
  console.error('Flux Purr demo runtime failed to initialize.', error)
}

function DemoRuntimeFailure() {
  return (
    <main className="demo-runtime-error" role="alert">
      <div>
        <span className="demo-runtime-error__eyebrow">DEMO / SAFE STOP</span>
        <h1>Demo unavailable</h1>
        <p>The mock runtime could not be initialized. No live connection was attempted.</p>
      </div>
    </main>
  )
}

createRoot(rootElement).render(
  <StrictMode>
    {demoRuntimeError ? <DemoRuntimeFailure /> : <RouterProvider router={router} />}
  </StrictMode>
)
