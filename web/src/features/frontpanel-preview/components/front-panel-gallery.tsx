import type { FrontPanelScreen } from '../types'
import { FrontPanelDisplay } from './front-panel-display'

interface FrontPanelGalleryProps {
  screens: ReadonlyArray<FrontPanelScreen>
}

export function FrontPanelGallery({ screens }: FrontPanelGalleryProps) {
  return (
    <div
      data-testid="front-panel-gallery"
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(2, minmax(0, 1fr))',
        gap: '36px 32px',
        alignItems: 'start',
      }}
    >
      {screens.map((screen, index) => (
        <section key={screen.kind} style={{ display: 'grid', gap: '12px' }}>
          <div
            style={{
              display: 'grid',
              gap: '8px',
              paddingTop: '14px',
              borderTop: index < 2 ? 'none' : '1px solid rgba(148, 163, 184, 0.18)',
            }}
          >
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'baseline',
                gap: '16px',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'baseline', gap: '10px' }}>
                <span
                  style={{
                    color: '#ff9a3c',
                    fontSize: '12px',
                    fontWeight: 700,
                    letterSpacing: '0.22em',
                    textTransform: 'uppercase',
                  }}
                >
                  {String(index + 1).padStart(2, '0')}
                </span>
                <h3
                  style={{
                    margin: 0,
                    color: '#f8fafc',
                    fontSize: '22px',
                    lineHeight: 1.1,
                    fontWeight: 700,
                  }}
                >
                  {screen.title}
                </h3>
              </div>
              <span
                style={{
                  color: '#8ea3c6',
                  fontSize: '12px',
                  textTransform: 'uppercase',
                  letterSpacing: '0.18em',
                  whiteSpace: 'nowrap',
                }}
              >
                {screen.kind}
              </span>
            </div>
            {screen.subtitle ? (
              <p
                style={{
                  margin: 0,
                  color: '#94a3b8',
                  fontSize: '14px',
                  lineHeight: 1.5,
                }}
              >
                {screen.subtitle}
              </p>
            ) : null}
          </div>

          <FrontPanelDisplay
            screen={screen}
            scale={4}
            showFrame={false}
            showMeta={false}
            className="w-full"
            frameClassName="p-0"
          />
        </section>
      ))}
    </div>
  )
}
