import type { FrontPanelScreen } from '../types'
import { FrontPanelDisplay } from './front-panel-display'

interface FrontPanelGalleryProps {
  screens: ReadonlyArray<FrontPanelScreen>
}

export function FrontPanelGallery({ screens }: FrontPanelGalleryProps) {
  return (
    <div
      data-testid="front-panel-gallery"
      className="grid gap-6 xl:grid-cols-3 md:grid-cols-2"
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
        gap: '24px',
        alignItems: 'start',
      }}
    >
      {screens.map((screen) => (
        <section
          key={screen.kind}
          className="rounded-[32px] border border-slate-800 bg-slate-950/90 p-5 shadow-[0_20px_60px_rgba(2,6,23,0.45)]"
          style={{
            borderRadius: '32px',
            border: '1px solid #1e293b',
            background: 'rgba(2, 6, 23, 0.92)',
            padding: '20px',
            color: '#e2e8f0',
            boxShadow: '0 20px 60px rgba(2, 6, 23, 0.45)',
          }}
        >
          <FrontPanelDisplay screen={screen} scale={4} frameClassName="p-3" />
        </section>
      ))}
    </div>
  )
}
